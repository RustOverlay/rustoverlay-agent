use std::sync::Arc;

use percent_encoding::{NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::{AUTHORIZATION, CONTENT_TYPE};
use reqwest::{Method, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use url::Url;
use uuid::Uuid;

use super::{ApiError, TeamDto, TeamPreviewDto};
use crate::enrollment::{AuthState, BackendOptions, TokenStore};

/// A failed team call, with the API's `{code, message}` unwrapped so the UI can show what failed.
#[derive(Debug)]
pub struct TeamsApiError {
    pub message: String,
    pub status: Option<StatusCode>,
    /// The API's machine-readable code: not_found, conflict, forbidden, unauthorized.
    pub code: Option<String>,
    pub source: Option<reqwest::Error>,
}

impl TeamsApiError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into(), status: None, code: None, source: None }
    }
}

impl std::fmt::Display for TeamsApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for TeamsApiError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        self.source.as_ref().map(|e| e as _)
    }
}

#[derive(Serialize)]
struct CreateBody<'a> {
    name: &'a str,
}

#[derive(Serialize)]
struct JoinBody<'a> {
    join_code: &'a str,
}

/// The team CRUD surface. Every call carries the install token as a bearer credential — this is the
/// first thing in the agent to authenticate to the API at all.
pub struct TeamsClient {
    http: reqwest::Client,
    tokens: Arc<dyn TokenStore>,
    auth: Arc<AuthState>,
    base_url: Url,
}

type Result<T> = std::result::Result<T, TeamsApiError>;

impl TeamsClient {
    pub fn new(
        http: reqwest::Client,
        tokens: Arc<dyn TokenStore>,
        auth: Arc<AuthState>,
        backend: &BackendOptions,
    ) -> Self {
        Self { http, tokens, auth, base_url: backend.base_url.clone() }
    }

    pub async fn list(&self) -> Result<Vec<TeamDto>> {
        self.send(Method::GET, "v1/teams", None::<&()>).await
    }

    pub async fn create(&self, name: &str) -> Result<TeamDto> {
        self.send(Method::POST, "v1/teams", Some(&CreateBody { name })).await
    }

    pub async fn join(&self, join_code: &str) -> Result<TeamDto> {
        self.send(Method::POST, "v1/teams/join", Some(&JoinBody { join_code })).await
    }

    pub async fn preview(&self, join_code: &str) -> Result<TeamPreviewDto> {
        let segment = path_segment(join_code).ok_or_else(|| TeamsApiError::new("That join code is not valid."))?;
        self.send(Method::GET, &format!("v1/teams/by-code/{segment}"), None::<&()>).await
    }

    pub async fn rotate_join_code(&self, team_id: Uuid) -> Result<TeamDto> {
        self.send(Method::POST, &format!("v1/teams/{team_id}/rotate-code"), None::<&()>).await
    }

    pub async fn leave(&self, team_id: Uuid) -> Result<()> {
        let path = format!("v1/teams/{team_id}/members/me");
        self.send_raw(Method::DELETE, &path, None::<&()>).await.map(drop)
    }

    pub async fn kick(&self, team_id: Uuid, steam_id: u64) -> Result<()> {
        let path = format!("v1/teams/{team_id}/members/{steam_id}");
        self.send_raw(Method::DELETE, &path, None::<&()>).await.map(drop)
    }

    async fn send<T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<T> {
        let response = self.send_raw(method, path, body).await?;
        let bytes = response.bytes().await.map_err(|e| TeamsApiError {
            source: Some(e),
            ..TeamsApiError::new("Could not reach the RustOverlay API.")
        })?;

        crate::web_json::from_slice::<Option<T>>(&bytes)
            .map_err(|e| TeamsApiError::new(format!("The API returned an unreadable response: {e}")))?
            .ok_or_else(|| TeamsApiError::new("The API returned an empty response."))
    }

    async fn send_raw(
        &self,
        method: Method,
        path: &str,
        body: Option<&impl Serialize>,
    ) -> Result<Response> {
        let token = self.tokens.load().ok_or_else(|| TeamsApiError::new("This install is not signed in."))?;
        let url = self
            .base_url
            .join(path)
            .map_err(|e| TeamsApiError::new(format!("Invalid backend URL: {e}")))?;

        let is_post = method == Method::POST;
        let mut request = self.http.request(method, url).header(AUTHORIZATION, format!("Bearer {}", token.token));
        if let Some(body) = body {
            let json = serde_json::to_vec(body).map_err(|e| TeamsApiError::new(e.to_string()))?;
            request = request.header(CONTENT_TYPE, crate::enrollment::JSON_CONTENT_TYPE).body(json);
        } else if is_post {
            // HttpClient sent an explicit Content-Length: 0 on a bodiless POST.
            request = request.body(Vec::new());
        }

        let response = request.send().await.map_err(|e| TeamsApiError {
            source: Some(e),
            ..TeamsApiError::new("Could not reach the RustOverlay API.")
        })?;

        let status = response.status();
        if status.is_success() {
            return Ok(response);
        }

        // Every call routes through here, so this is the one place the agent can learn its token has
        // been revoked -- see AuthState on why a 401 is proof and the relay's bad_token is not.
        if status == StatusCode::UNAUTHORIZED {
            self.auth.sign_out(&token.token);
        }

        Err(describe(response).await)
    }
}

/// User input as one escaped path segment. None for input that is only a dot segment: the URL
/// parser resolves `..` and `%2E%2E` alike, so no escaping keeps those in place.
fn path_segment(input: &str) -> Option<String> {
    let trimmed = input.trim();
    if matches!(trimmed, "" | "." | "..") {
        return None;
    }
    Some(utf8_percent_encode(trimmed, NON_ALPHANUMERIC).to_string())
}

async fn describe(response: Response) -> TeamsApiError {
    let status = response.status();
    let error = match response.bytes().await {
        Ok(bytes) => crate::web_json::from_slice::<Option<ApiError>>(&bytes).ok().flatten(),
        Err(_) => None,
    };
    let (code, message) = error.map_or((None, None), |e| (e.code, e.message));

    TeamsApiError {
        message: message.unwrap_or_else(|| format!("The API rejected this request ({}).", status.as_u16())),
        status: Some(status),
        code,
        source: None,
    }
}
