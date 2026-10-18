use std::io;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream};
use url::Url;

use super::proxy_resolver::{HttpProxy, system_proxy};
use super::relay_connection::socket_config;
use super::RelayError;

pub type RelaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

const MAX_RESPONSE_HEAD: usize = 8 * 1024;

/// Opens the relay socket through whatever proxy the system settings choose for `url`.
pub(crate) async fn connect_with_system_proxy(url: &Url) -> Result<RelaySocket, RelayError> {
    let proxy = system_proxy(url);
    if let Some(proxy) = &proxy {
        tracing::debug!(proxy = %proxy.authority, "Tunnelling the relay through a proxy");
    }
    connect(url, proxy.as_ref()).await
}

/// Opens a WebSocket to `url`, through an HTTP `CONNECT` tunnel when `proxy` is set. Nagle is off
/// either way; the caller's timeout covers the tunnel, TLS and the WebSocket handshake together.
pub async fn connect(url: &Url, proxy: Option<&HttpProxy>) -> Result<RelaySocket, RelayError> {
    let Some(proxy) = proxy else {
        let (socket, _) = tokio_tungstenite::connect_async_with_config(url.as_str(), Some(socket_config()), true).await?;
        return Ok(socket);
    };

    let target = target_authority(url)?;
    let mut tcp = TcpStream::connect(proxy.authority.as_str()).await.map_err(io_error)?;
    tcp.set_nodelay(true).map_err(io_error)?;
    open_tunnel(&mut tcp, &target, proxy).await?;

    let (socket, _) = tokio_tungstenite::client_async_tls_with_config(url.as_str(), tcp, Some(socket_config()), None).await?;
    Ok(socket)
}

async fn open_tunnel(tcp: &mut TcpStream, target: &str, proxy: &HttpProxy) -> Result<(), RelayError> {
    let mut request = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if let Some(authorization) = &proxy.authorization {
        request.push_str(&format!("Proxy-Authorization: {authorization}\r\n"));
    }
    request.push_str("\r\n");
    tcp.write_all(request.as_bytes()).await.map_err(io_error)?;

    let head = read_response_head(tcp).await?;
    let status = status_code(&head)
        .ok_or_else(|| RelayError::Protocol(format!("proxy {} sent no HTTP status", proxy.authority)))?;
    if !(200..300).contains(&status) {
        return Err(RelayError::Protocol(format!("proxy {} refused the tunnel with HTTP {status}", proxy.authority)));
    }
    Ok(())
}

/// Reads byte by byte so nothing past the blank line is consumed; those bytes belong to TLS.
async fn read_response_head(tcp: &mut TcpStream) -> Result<String, RelayError> {
    let mut head = Vec::with_capacity(256);
    while !head.ends_with(b"\r\n\r\n") {
        if head.len() >= MAX_RESPONSE_HEAD {
            return Err(RelayError::Protocol("proxy response head exceeds 8 KiB".into()));
        }
        let byte = tcp.read_u8().await.map_err(io_error)?;
        head.push(byte);
    }
    Ok(String::from_utf8_lossy(&head).into_owned())
}

fn status_code(head: &str) -> Option<u16> {
    let mut status_line = head.lines().next()?.split_whitespace();
    status_line.next().filter(|version| version.starts_with("HTTP/"))?;
    status_line.next()?.parse().ok()
}

fn target_authority(url: &Url) -> Result<String, RelayError> {
    let host = url.host_str().ok_or_else(|| RelayError::Protocol(format!("{url} has no host")))?;
    let port = url.port_or_known_default().ok_or_else(|| RelayError::Protocol(format!("{url} has no port")))?;
    Ok(format!("{host}:{port}"))
}

fn io_error(e: io::Error) -> RelayError {
    tungstenite::Error::Io(e).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_code_reads_the_status_line_only() {
        assert_eq!(status_code("HTTP/1.1 200 Connection established\r\n\r\n"), Some(200));
        assert_eq!(status_code("HTTP/1.0 407 Proxy Authentication Required\r\n\r\n"), Some(407));
        assert_eq!(status_code("SSH-2.0-OpenSSH\r\n\r\n"), None);
    }

    #[test]
    fn the_tunnel_target_defaults_the_port_by_scheme() {
        let target = |url: &str| target_authority(&Url::parse(url).unwrap()).unwrap();
        assert_eq!(target("wss://relay-eu.rustoverlay.com/v1/room"), "relay-eu.rustoverlay.com:443");
        assert_eq!(target("ws://127.0.0.1:9000/v1/room"), "127.0.0.1:9000");
        assert_eq!(target("wss://[::1]/"), "[::1]:443");
    }
}
