mod common;

use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use chrono::TimeDelta;
use common::MemoryTokenStore;
use ro_net::enrollment::{
    BackendOptions, BrowserLauncher, EnrollmentError, EnrollmentService, InstallToken, TokenStore,
};
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use sha2::{Digest, Sha256};
use url::Url;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const ISSUED: &str = r#"{"token":"tok_live","steam_id":76561198000000001,"display_name":"Hexay"}"#;

/// Stands in for the browser by actually fetching the loopback URL it is handed.
struct FakeBrowser {
    code: Option<String>,
    last_url: Mutex<Option<String>>,
}

impl FakeBrowser {
    fn new(code: Option<&str>) -> Arc<Self> {
        Arc::new(Self { code: code.map(Into::into), last_url: Mutex::new(None) })
    }

    fn last_url(&self) -> Url {
        Url::parse(self.last_url.lock().unwrap().as_deref().unwrap()).unwrap()
    }
}

fn query(url: &Url, name: &str) -> String {
    url.query_pairs().find(|(k, _)| k == name).unwrap().1.into_owned()
}

impl BrowserLauncher for FakeBrowser {
    fn open(&self, url: &str) -> io::Result<()> {
        *self.last_url.lock().unwrap() = Some(url.into());
        let Some(code) = &self.code else { return Ok(()) };

        let mut target = Url::parse(&query(&Url::parse(url).unwrap(), "cb")).unwrap();
        target.query_pairs_mut().append_pair("code", code);

        // Fire and forget: the service only starts accepting once open returns.
        tokio::spawn(async move {
            let mut client = TcpStream::connect((target.host_str().unwrap(), target.port().unwrap())).await.unwrap();
            let path = format!("{}?{}", target.path(), target.query().unwrap());
            client.write_all(format!("GET {path} HTTP/1.1\r\nHost: loopback\r\n\r\n").as_bytes()).await.unwrap();
            client.flush().await.unwrap();
        });
        Ok(())
    }
}

async fn stub_api(status: u16, body: &str) -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(ResponseTemplate::new(status).set_body_string(body)).mount(&server).await;
    server
}

/// A base URL nothing listens on.
fn unreachable_api() -> Url {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    Url::parse(&format!("http://127.0.0.1:{port}")).unwrap()
}

fn build(browser: Arc<FakeBrowser>, api: Url, timeout: Option<TimeDelta>) -> (EnrollmentService, Arc<MemoryTokenStore>) {
    let store = Arc::new(MemoryTokenStore::default());
    let options = BackendOptions { base_url: api, enrollment_timeout: timeout.unwrap_or(TimeDelta::seconds(10)) };
    let service = EnrollmentService::new(reqwest::Client::new(), store.clone(), browser, options);
    (service, store)
}

fn api_url(server: &MockServer) -> Url {
    Url::parse(&server.uri()).unwrap()
}

#[tokio::test]
async fn enrolls_and_returns_the_issued_token() {
    let api = stub_api(200, ISSUED).await;
    let (service, store) = build(FakeBrowser::new(Some("code-abc")), api_url(&api), None);

    let token = service.enroll().await.unwrap();

    assert_eq!(token, InstallToken::new("tok_live", 76561198000000001, "Hexay"));
    assert_eq!(store.stored(), Some(token));
}

#[tokio::test]
async fn sends_the_browser_to_start_with_its_own_loopback_callback() {
    let api = stub_api(200, ISSUED).await;
    let browser = FakeBrowser::new(Some("code-abc"));
    let (service, _) = build(browser.clone(), api_url(&api), None);

    service.enroll().await.unwrap();

    let url = browser.last_url();
    assert_eq!(url.path(), "/auth/steam/start");

    let cb = Url::parse(&query(&url, "cb")).unwrap();
    assert_eq!(cb.scheme(), "http");
    assert_eq!(cb.host_str(), Some("127.0.0.1"));
    assert_eq!(cb.path(), "/done");
}

#[tokio::test]
async fn exchanges_the_code_it_received_over_loopback() {
    let api = stub_api(200, ISSUED).await;
    let (service, _) = build(FakeBrowser::new(Some("code-abc")), api_url(&api), None);

    service.enroll().await.unwrap();

    let requests = api.received_requests().await.unwrap();
    let last = requests.last().unwrap();
    assert_eq!(last.method.as_str(), "POST");
    assert_eq!(last.url.path(), "/auth/exchange");

    let body: serde_json::Value = serde_json::from_slice(&last.body).unwrap();
    assert_eq!(body["code"], "code-abc");
    assert!(!body["agent_version"].as_str().unwrap().is_empty());
}

#[tokio::test]
async fn binds_the_code_with_a_challenge_that_the_exchanged_verifier_hashes_to() {
    let api = stub_api(200, ISSUED).await;
    let browser = FakeBrowser::new(Some("code-abc"));
    let (service, _) = build(browser.clone(), api_url(&api), None);

    service.enroll().await.unwrap();

    let start = browser.last_url();
    let challenge = query(&start, "code_challenge");
    assert_eq!(challenge.len(), 43);
    assert_eq!(query(&start, "code_challenge_method"), "S256");

    let requests = api.received_requests().await.unwrap();
    let body: serde_json::Value = serde_json::from_slice(&requests.last().unwrap().body).unwrap();
    let verifier = body["code_verifier"].as_str().unwrap();
    assert_ne!(verifier, challenge, "the verifier itself must never go out on start");
    assert_eq!(URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes())), challenge);
}

#[tokio::test]
async fn stores_nothing_when_the_api_rejects_the_code() {
    let api = stub_api(401, "{}").await;
    let (service, store) = build(FakeBrowser::new(Some("stale")), api_url(&api), None);

    assert!(matches!(service.enroll().await, Err(EnrollmentError::Failed { .. })));
    assert_eq!(store.stored(), None);
}

#[tokio::test]
async fn fails_when_the_api_is_unreachable() {
    let (service, store) = build(FakeBrowser::new(Some("code-abc")), unreachable_api(), None);

    assert!(matches!(service.enroll().await, Err(EnrollmentError::Failed { .. })));
    assert_eq!(store.stored(), None);
}

#[tokio::test]
async fn gives_up_when_the_user_never_completes_sign_in() {
    let api = stub_api(200, ISSUED).await;
    let (service, store) = build(FakeBrowser::new(None), api_url(&api), Some(TimeDelta::milliseconds(250)));

    assert!(matches!(service.enroll().await, Err(EnrollmentError::Failed { .. })));
    assert_eq!(store.stored(), None);
}

/// Cancelling is dropping the future: the caller sees its own timeout, never an EnrollmentError.
#[tokio::test]
async fn surfaces_caller_cancellation_rather_than_reporting_a_timeout() {
    let api = stub_api(200, ISSUED).await;
    let (service, _) = build(FakeBrowser::new(None), api_url(&api), None);

    let outcome = tokio::time::timeout(Duration::from_millis(100), service.enroll()).await;

    assert!(outcome.is_err());
}

#[tokio::test]
async fn reports_enrollment_from_whatever_the_store_holds() {
    let api = stub_api(200, ISSUED).await;
    let (service, store) = build(FakeBrowser::new(None), api_url(&api), None);

    assert!(!service.is_enrolled());

    store.save(&InstallToken::new("tok", 1, "n")).unwrap();
    assert!(service.is_enrolled());
}
