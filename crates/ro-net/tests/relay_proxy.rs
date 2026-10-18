//! The relay socket through an HTTP CONNECT proxy.

mod common;

use common::relay::{PATIENCE, accept, next, server_frame};
use futures_util::SinkExt;
use ro_net::proto::{self, ClientFrame, Heartbeat, client_frame, server_frame};
use ro_net::relay::{HttpProxy, RelayConnection, connect};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use url::Url;

/// A one-shot CONNECT proxy on loopback that answers `status` and, on 2xx, splices to the target
/// the request named. Yields its address and the request head it read.
async fn proxy(status: &'static str) -> (String, oneshot::Receiver<String>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let at = listener.local_addr().unwrap().to_string();
    let (head_tx, head_rx) = oneshot::channel();

    tokio::spawn(async move {
        let (mut client, _) = listener.accept().await.unwrap();
        let mut head = Vec::new();
        while !head.ends_with(b"\r\n\r\n") {
            head.push(client.read_u8().await.unwrap());
        }
        let head = String::from_utf8(head).unwrap();
        let target = head.split_whitespace().nth(1).unwrap().to_owned();
        let _ = head_tx.send(head);

        client.write_all(format!("HTTP/1.1 {status}\r\n\r\n").as_bytes()).await.unwrap();
        if status.starts_with('2') {
            let mut upstream = TcpStream::connect(target).await.unwrap();
            let _ = tokio::io::copy_bidirectional(&mut client, &mut upstream).await;
        }
    });

    (at, head_rx)
}

async fn relay() -> (TcpListener, Url) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = Url::parse(&format!("ws://{}/v1/room", listener.local_addr().unwrap())).unwrap();
    (listener, url)
}

#[tokio::test(flavor = "multi_thread")]
async fn the_socket_tunnels_through_the_proxy_with_its_credentials() {
    let (listener, url) = relay().await;
    let target = format!("{}:{}", url.host_str().unwrap(), url.port().unwrap());
    let (proxy_at, request) = proxy("200 Connection established").await;
    let proxy = HttpProxy::parse(&format!("http://user:pass@{proxy_at}")).unwrap();

    let (client, mut server) = tokio::join!(
        tokio::time::timeout(PATIENCE, connect(&url, Some(&proxy))),
        accept(&listener)
    );
    let client = RelayConnection::new(client.unwrap().unwrap());

    let heartbeat = Heartbeat { sent_at_ms: 42 };
    client.send(&ClientFrame { seq: 7, payload: Some(client_frame::Payload::Heartbeat(heartbeat)) }).await.unwrap();
    assert_eq!(next(&mut server).await.seq, 7);

    let joined = proto::JoinAccepted { room: "7926ZG".into(), region: "eu".into(), server_time_ms: 0 };
    server.send(server_frame(server_frame::Payload::Joined(joined))).await.unwrap();
    let received = client.receive(Some(PATIENCE)).await.unwrap().unwrap();
    assert!(matches!(received.payload, Some(server_frame::Payload::Joined(j)) if j.region == "eu"));

    let head = request.await.unwrap();
    assert!(head.starts_with(&format!("CONNECT {target} HTTP/1.1\r\n")), "{head}");
    assert!(head.contains(&format!("\r\nHost: {target}\r\n")), "{head}");
    assert!(head.contains("\r\nProxy-Authorization: Basic dXNlcjpwYXNz\r\n"), "{head}");
}

#[tokio::test(flavor = "multi_thread")]
async fn a_refused_tunnel_fails_with_the_proxy_status() {
    let (_listener, url) = relay().await;
    let (proxy_at, request) = proxy("407 Proxy Authentication Required").await;
    let proxy = HttpProxy::parse(&proxy_at).unwrap();

    let Err(error) = tokio::time::timeout(PATIENCE, connect(&url, Some(&proxy))).await.unwrap() else {
        panic!("the tunnel opened")
    };

    assert!(error.to_string().contains("HTTP 407"), "{error}");
    assert!(!request.await.unwrap().contains("Proxy-Authorization"));
}
