use std::io;
use std::net::{Ipv4Addr, TcpListener as StdListener};
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::OnceCell;
use tokio::task::JoinSet;
use tokio::time::timeout;

const MAX_REQUEST_LINE: usize = 8192;
const CONNECTION_TIMEOUT: Duration = Duration::from_secs(5);

const ACCEPTED_BODY: &str = "<!doctype html><meta charset=utf-8><title>RustOverlay</title>\
<p style=font-family:sans-serif>Signed in. You can close this tab.";

/// A one-shot HTTP endpoint on 127.0.0.1 that receives the enrollment code the browser is
/// redirected to. Bound to port 0, so the OS picks a free port and the agent tells the API which one
/// it got.
///
/// A raw TCP listener rather than an HTTP server: only a request line is ever parsed, which is all
/// the redirect carries.
pub struct LoopbackCallback {
    bound: Mutex<Option<StdListener>>,
    // Registered with the runtime once: a fresh try_clone per wait stops seeing accepts on Windows.
    listener: OnceCell<TcpListener>,
    port: u16,
}

impl LoopbackCallback {
    pub fn new() -> io::Result<Self> {
        let listener = StdListener::bind((Ipv4Addr::LOCALHOST, 0))?;
        listener.set_nonblocking(true)?;
        let port = listener.local_addr()?.port();
        Ok(Self { bound: Mutex::new(Some(listener)), listener: OnceCell::new(), port })
    }

    async fn listener(&self) -> io::Result<&TcpListener> {
        self.listener
            .get_or_try_init(|| async {
                let bound = self.bound.lock().unwrap_or_else(PoisonError::into_inner).take();
                TcpListener::from_std(bound.expect("taken only by this initialiser"))
            })
            .await
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    /// What the API is told to redirect the browser to. Loopback and http, both required.
    pub fn url(&self) -> String {
        format!("http://127.0.0.1:{}/done", self.port)
    }

    /// Resolves when a request arrives carrying a `code` parameter. Anything else -- a favicon
    /// probe, a stray prefetch -- is answered 404 and waited past, so an unrelated request cannot
    /// consume the single callback this flow gets. Drop the future to cancel.
    ///
    /// Connections are served concurrently: Chromium opens speculative sockets that send nothing,
    /// and one of those must not hold the real redirect in the backlog. Only the listener failing
    /// is fatal.
    pub async fn wait_for_code(&self) -> io::Result<String> {
        let listener = self.listener().await?;
        let mut connections = JoinSet::new();

        loop {
            tokio::select! {
                accepted = listener.accept() => match accepted {
                    Ok((stream, _)) => {
                        connections.spawn(serve(stream));
                    }
                    // Windows reports a client that reset while still in the backlog from accept.
                    Err(e) if matches!(e.kind(), io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted) => {
                        tracing::debug!(error = %e, "A loopback connection reset before it was accepted");
                    }
                    Err(e) => return Err(e),
                },
                Some(served) = connections.join_next() => {
                    if let Ok(Some(code)) = served {
                        return Ok(code);
                    }
                }
            }
        }
    }
}

/// The code this connection carried, if any. I/O failures and idle sockets only end this one.
async fn serve(mut stream: TcpStream) -> Option<String> {
    let line = match timeout(CONNECTION_TIMEOUT, read_request_line(&mut stream)).await {
        Ok(Ok(line)) => line,
        Ok(Err(e)) => {
            tracing::debug!(error = %e, "Dropped a loopback connection");
            return None;
        }
        Err(_) => {
            tracing::debug!("Dropped an idle loopback connection");
            return None;
        }
    };
    let code = line.as_deref().and_then(code_from);

    let response = match code {
        Some(_) => response("200 OK", ACCEPTED_BODY),
        None => response("404 Not Found", ""),
    };
    // A browser that hung up before reading the page has still delivered the code.
    let written = timeout(CONNECTION_TIMEOUT, async {
        stream.write_all(&response).await?;
        stream.flush().await
    })
    .await;
    if !matches!(written, Ok(Ok(()))) {
        tracing::debug!("Could not answer a loopback connection");
    }

    code
}

/// `GET /done?code=xyz HTTP/1.1` → `xyz`.
fn code_from(request_line: &str) -> Option<String> {
    let parts: Vec<&str> = request_line.split(' ').collect();
    if parts.len() < 2 || parts[0] != "GET" {
        return None;
    }

    let (_, query) = parts[1].split_once('?')?;
    query_value(query, "code").filter(|code| !code.is_empty())
}

/// `HttpUtility.ParseQueryString(query)[name]`: keys match case-insensitively, `+` is a space,
/// repeated keys join with commas, and a segment without `=` is a value under the null key.
// TODO(port): HttpUtility also decoded the legacy %uXXXX form; form_urlencoded does not.
fn query_value(query: &str, name: &str) -> Option<String> {
    let values: Vec<String> = query
        .split('&')
        .filter(|segment| segment.contains('='))
        .flat_map(|segment| url::form_urlencoded::parse(segment.as_bytes()))
        .filter(|(key, _)| key.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.into_owned())
        .collect();

    (!values.is_empty()).then(|| values.join(","))
}

async fn read_request_line(stream: &mut TcpStream) -> io::Result<Option<String>> {
    let mut buffer = vec![0u8; MAX_REQUEST_LINE];
    let mut filled = 0;

    while filled < MAX_REQUEST_LINE {
        let read = stream.read(&mut buffer[filled..]).await?;
        if read == 0 {
            break;
        }

        filled += read;
        if let Some(end) = buffer[..filled].iter().position(|&b| b == b'\n') {
            // Encoding.ASCII: anything above 0x7F decodes as '?'.
            let line: String =
                buffer[..end].iter().map(|&b| if b < 0x80 { b as char } else { '?' }).collect();
            return Ok(Some(line.trim_end_matches('\r').to_owned()));
        }
    }

    Ok(None)
}

fn response(status: &str, body: &str) -> Vec<u8> {
    format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_get_requests_with_a_code() {
        assert_eq!(code_from("GET /done?code=xyz HTTP/1.1").as_deref(), Some("xyz"));
        assert_eq!(code_from("GET /done?CODE=a+b HTTP/1.1").as_deref(), Some("a b"));
        assert_eq!(code_from("GET /done?code=a&code=b HTTP/1.1").as_deref(), Some("a,b"));
        assert_eq!(code_from("GET /done?code&code=b HTTP/1.1").as_deref(), Some("b"));
        assert_eq!(code_from("GET /done?code=a=b HTTP/1.1").as_deref(), Some("a=b"));
        assert_eq!(code_from("POST /done?code=xyz HTTP/1.1"), None);
        assert_eq!(code_from("GET /done?code= HTTP/1.1"), None);
        assert_eq!(code_from("GET /done HTTP/1.1"), None);
        assert_eq!(code_from("GET"), None);
    }
}
