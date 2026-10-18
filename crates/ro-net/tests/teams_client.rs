mod common;

use std::sync::Arc;
use std::time::Duration;

use common::{MemoryTokenStore, hexay};
use ro_net::enrollment::{AuthState, BackendOptions, InstallToken, TokenStore};
use ro_net::teams::TeamsClient;
use url::Url;
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, ResponseTemplate};

const TEAM: &str = r#"{"id":"5f0e8b1e-0000-4000-8000-000000000001","name":"Squad","join_code":"7926ZG","owner_steam_id":1,"members":[]}"#;

/// Bind the server to a named variable: `_` drops it back into wiremock's pool, where a concurrent
/// test can remount it with its own body.
async fn build(status: u16, body: &str) -> (TeamsClient, Arc<AuthState>, MockServer) {
    let server = MockServer::start().await;
    Mock::given(any()).respond_with(ResponseTemplate::new(status).set_body_string(body)).mount(&server).await;
    let base_url = Url::parse(&server.uri()).unwrap();
    let (client, auth) = client_for(base_url);
    (client, auth, server)
}

fn client_for(base_url: Url) -> (TeamsClient, Arc<AuthState>) {
    let store = Arc::new(MemoryTokenStore::default());
    store.save(&hexay()).unwrap();

    let auth = Arc::new(AuthState::new(store.clone()));
    let options = BackendOptions { base_url, ..Default::default() };
    (TeamsClient::new(reqwest::Client::new(), store, auth.clone(), &options), auth)
}

#[tokio::test]
async fn carries_the_install_token_as_a_bearer_credential() {
    let (client, _, api) = build(200, &format!("[{TEAM}]")).await;

    let teams = client.list().await.unwrap();

    assert_eq!(teams[0].join_code, "7926ZG");
    let requests = api.received_requests().await.unwrap();
    assert_eq!(requests.last().unwrap().headers["authorization"], "Bearer tok_live");
}

/// HttpClient read responses with JsonSerializerDefaults.Web: any key casing, numbers as strings.
#[tokio::test]
async fn reads_responses_as_leniently_as_the_web_defaults() {
    let (client, _, _api) = build(200, r#"{"Name":"Squad","MEMBER_COUNT":"3"}"#).await;

    let preview = client.preview("7926ZG").await.unwrap();

    assert_eq!((preview.name.as_str(), preview.member_count), ("Squad", 3));
}

#[tokio::test]
async fn a_rejected_token_signs_the_install_out() {
    let (client, auth, _api) = build(401, r#"{"code":"unauthorized","message":"invalid install token"}"#).await;

    let error = client.list().await.unwrap_err();

    assert_eq!(error.code.as_deref(), Some("unauthorized"));
    assert_eq!(error.message, "invalid install token");
    assert!(auth.is_signed_out());
}

/// The distinction the whole design rests on: only a 401 means the credential is dead. A 403 or a
/// 404 is about the team being asked for, and signing out over one would evict a valid user.
#[tokio::test]
async fn any_other_refusal_leaves_the_install_signed_in() {
    for status in [403, 404, 409, 500] {
        let (client, auth, _api) = build(status, r#"{"code":"forbidden","message":"not the owner"}"#).await;

        let error = client.join("7926ZG").await.unwrap_err();

        assert_eq!(error.status.map(|s| s.as_u16()), Some(status));
        assert!(!auth.is_signed_out(), "{status}");
    }
}

/// C# cancelled the request; dropping the future is the Rust equivalent, and a dead socket is the
/// failure that actually reaches the client.
#[tokio::test]
async fn an_unreachable_api_is_not_a_rejected_token() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    drop(listener);
    let (client, auth) = client_for(Url::parse(&format!("http://127.0.0.1:{port}")).unwrap());

    assert!(client.list().await.is_err());

    assert!(!auth.is_signed_out());
}

#[tokio::test]
async fn a_signed_out_install_stops_sending_requests_at_all() {
    let (client, auth, api) = build(401, "{}").await;
    assert!(client.list().await.is_err());
    let after_sign_out = api.received_requests().await.unwrap().len();

    assert!(client.list().await.is_err());

    assert!(auth.is_signed_out());
    assert_eq!(api.received_requests().await.unwrap().len(), after_sign_out);
}

/// The user signed in again while a request with the revoked token was still in flight.
#[tokio::test]
async fn a_late_401_for_a_replaced_token_keeps_the_new_one() {
    let api = MockServer::start().await;
    let slow = ResponseTemplate::new(401).set_delay(Duration::from_millis(300));
    Mock::given(any()).respond_with(slow).mount(&api).await;
    let store = Arc::new(MemoryTokenStore::default());
    store.save(&hexay()).unwrap();
    let auth = Arc::new(AuthState::new(store.clone()));
    let options = BackendOptions { base_url: Url::parse(&api.uri()).unwrap(), ..Default::default() };
    let client = TeamsClient::new(reqwest::Client::new(), store.clone(), auth.clone(), &options);

    let re_enrol = async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        store.save(&InstallToken::new("tok_fresh", 1, "Hexay")).unwrap();
    };
    let (outcome, ()) = tokio::join!(client.list(), re_enrol);

    assert!(outcome.is_err());
    assert!(!auth.is_signed_out());
    assert_eq!(store.stored().map(|t| t.token).as_deref(), Some("tok_fresh"));
}

#[tokio::test]
async fn escapes_the_join_code_as_one_path_segment() {
    let (client, _, api) = build(200, r#"{"name":"Squad","member_count":1}"#).await;

    client.preview(" ab?x/.. ").await.unwrap();

    let requests = api.received_requests().await.unwrap();
    assert_eq!(requests.last().unwrap().url.path(), "/v1/teams/by-code/ab%3Fx%2F%2E%2E");
}

#[tokio::test]
async fn refuses_a_join_code_that_is_only_a_dot_segment() {
    let (client, _, api) = build(200, "{}").await;

    for code in ["", "  ", ".", ".."] {
        assert!(client.preview(code).await.is_err(), "{code:?}");
    }
    assert!(api.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn a_failure_without_an_envelope_reports_the_status() {
    let (client, _, _api) = build(502, "<html>bad gateway</html>").await;

    let error = client.list().await.unwrap_err();

    assert_eq!(error.message, "The API rejected this request (502).");
    assert_eq!(error.code, None);
}
