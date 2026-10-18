use std::sync::Arc;
use std::time::Duration;

use ro_net::enrollment::LoopbackCallback;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio::task::JoinHandle;

const PATIENCE: Duration = Duration::from_secs(10);

async fn request(port: u16, target: &str) -> String {
    let mut client = TcpStream::connect(("127.0.0.1", port)).await.unwrap();
    client.write_all(format!("GET {target} HTTP/1.1\r\nHost: localhost\r\n\r\n").as_bytes()).await.unwrap();

    let mut response = String::new();
    client.read_to_string(&mut response).await.unwrap();
    response
}

fn start() -> (Arc<LoopbackCallback>, JoinHandle<String>) {
    let callback = Arc::new(LoopbackCallback::new().unwrap());
    let waiting = callback.clone();
    let task = tokio::spawn(async move {
        tokio::time::timeout(PATIENCE, waiting.wait_for_code()).await.unwrap().unwrap()
    });
    (callback, task)
}

#[test]
fn binds_a_loopback_url_on_an_ephemeral_port() {
    let callback = LoopbackCallback::new().unwrap();

    assert_ne!(callback.port(), 0);
    assert_eq!(callback.url(), format!("http://127.0.0.1:{}/done", callback.port()));
}

#[tokio::test]
async fn yields_the_code_the_browser_was_redirected_with() {
    let (callback, waiting) = start();

    let response = request(callback.port(), "/done?code=abc123").await;

    assert_eq!(waiting.await.unwrap(), "abc123");
    assert!(response.contains("200 OK"));
}

#[tokio::test]
async fn url_decodes_the_code() {
    let (callback, waiting) = start();

    request(callback.port(), "/done?code=a%2Bb%2Fc").await;

    assert_eq!(waiting.await.unwrap(), "a+b/c");
}

/// Browsers probe for a favicon; that must not consume the one callback we get.
#[tokio::test]
async fn ignores_requests_without_a_code_and_keeps_waiting() {
    let (callback, waiting) = start();

    assert!(request(callback.port(), "/favicon.ico").await.contains("404"));
    assert!(!waiting.is_finished());

    request(callback.port(), "/done?code=real").await;
    assert_eq!(waiting.await.unwrap(), "real");
}

#[tokio::test]
async fn ignores_a_query_string_that_carries_no_code() {
    let (callback, waiting) = start();

    request(callback.port(), "/done?error=access_denied").await;
    assert!(!waiting.is_finished());

    request(callback.port(), "/done?code=real").await;
    assert_eq!(waiting.await.unwrap(), "real");
}

/// Chromium's speculative preconnect: a socket that never sends a request.
#[tokio::test]
async fn an_idle_connection_does_not_hold_up_the_callback() {
    let (callback, waiting) = start();
    let _idle = TcpStream::connect(("127.0.0.1", callback.port())).await.unwrap();

    let response = tokio::time::timeout(Duration::from_secs(2), request(callback.port(), "/done?code=real"))
        .await
        .expect("served behind an idle socket");

    assert!(response.contains("200 OK"));
    assert_eq!(waiting.await.unwrap(), "real");
}

#[tokio::test]
async fn a_connection_dropped_mid_request_is_skipped() {
    let (callback, waiting) = start();
    let mut dropped = TcpStream::connect(("127.0.0.1", callback.port())).await.unwrap();
    dropped.write_all(b"GET /favicon.ico HTTP/1.1\r\n").await.unwrap();
    dropped.set_zero_linger().unwrap();
    drop(dropped);

    request(callback.port(), "/done?code=real").await;

    assert_eq!(waiting.await.unwrap(), "real");
}

/// Cancellation is dropping the future.
#[tokio::test]
async fn stops_waiting_when_cancelled() {
    let callback = LoopbackCallback::new().unwrap();

    let cancelled = tokio::time::timeout(Duration::ZERO, callback.wait_for_code()).await;

    assert!(cancelled.is_err());
}
