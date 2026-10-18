use std::sync::Arc;
use std::time::Duration;

use percent_encoding::{AsciiSet, NON_ALPHANUMERIC, utf8_percent_encode};
use reqwest::header::CONTENT_TYPE;
use serde::Serialize;
use tokio::time::Instant;

use super::pkce::Pkce;
use super::{BackendOptions, BrowserLauncher, InstallToken, LoopbackCallback, TokenStore};

/// `Uri.EscapeDataString`: everything but RFC 3986 unreserved characters is escaped.
const DATA_STRING: &AsciiSet = &NON_ALPHANUMERIC.remove(b'-').remove(b'.').remove(b'_').remove(b'~');

pub(crate) const JSON_CONTENT_TYPE: &str = "application/json; charset=utf-8";

#[derive(Debug)]
pub enum EnrollmentError {
    /// The flow ran and did not produce a token; the message is fit to show the user.
    Failed { message: String, source: Option<reqwest::Error> },
    /// The loopback listener, the browser launch or the token store failed.
    Io(std::io::Error),
    /// The API answered success with a body that is not an install token.
    Decode(serde_json::Error),
}

impl EnrollmentError {
    fn failed(message: impl Into<String>) -> Self {
        Self::Failed { message: message.into(), source: None }
    }
}

impl std::fmt::Display for EnrollmentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Failed { message, .. } => f.write_str(message),
            Self::Io(e) => write!(f, "{e}"),
            Self::Decode(e) => write!(f, "The API returned an unreadable response: {e}"),
        }
    }
}

impl std::error::Error for EnrollmentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Failed { source, .. } => source.as_ref().map(|e| e as _),
            Self::Io(e) => Some(e),
            Self::Decode(e) => Some(e),
        }
    }
}

impl From<std::io::Error> for EnrollmentError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

#[derive(Serialize)]
struct ExchangeRequest<'a> {
    code: &'a str,
    agent_version: &'a str,
    code_verifier: &'a str,
}

/// Drives Steam sign-in. The browser does the authenticating; this process only ever sees a
/// one-time code, and swaps it for the install token over TLS.
///
/// The code, not the token, is what comes back over loopback: that redirect URL lands in browser
/// history. See the backend's docs/open-questions.md for the whole flow.
pub struct EnrollmentService {
    http: reqwest::Client,
    store: Arc<dyn TokenStore>,
    browser: Arc<dyn BrowserLauncher>,
    options: BackendOptions,
}

impl EnrollmentService {
    pub fn new(
        http: reqwest::Client,
        store: Arc<dyn TokenStore>,
        browser: Arc<dyn BrowserLauncher>,
        options: BackendOptions,
    ) -> Self {
        Self { http, store, browser, options }
    }

    pub fn current(&self) -> Option<InstallToken> {
        self.store.load()
    }

    pub fn is_enrolled(&self) -> bool {
        self.current().is_some()
    }

    /// Drop the future to cancel; that is never reported as a timeout.
    pub async fn enroll(&self) -> Result<InstallToken, EnrollmentError> {
        let callback = LoopbackCallback::new()?;
        let timeout = self.options.enrollment_timeout.to_std().unwrap_or(Duration::ZERO);
        let deadline = Instant::now() + timeout;
        let pkce = Pkce::generate()?;

        let encoded = utf8_percent_encode(&callback.url(), DATA_STRING).to_string();
        let start = self
            .options
            .base_url
            .join(&format!(
                "/auth/steam/start?cb={encoded}&code_challenge={}&code_challenge_method=S256",
                pkce.challenge
            ))
            .map_err(|e| EnrollmentError::failed(format!("Invalid backend URL: {e}")))?;

        tracing::info!(port = callback.port(), "Opening Steam sign-in; awaiting callback");
        self.browser.open(start.as_str())?;

        let code = match tokio::time::timeout_at(deadline, callback.wait_for_code()).await {
            Ok(code) => code?,
            Err(_) => {
                let minutes = self.options.enrollment_timeout.as_seconds_f64() / 60.0;
                return Err(EnrollmentError::failed(format!(
                    "Sign-in was not completed within {:.0} minutes.",
                    minutes.round()
                )));
            }
        };

        let token = self.exchange(&code, &pkce.verifier, deadline).await?;
        self.store.save(&token)?;

        tracing::info!(display_name = %token.display_name, steam_id = token.steam_id, "Enrolled");
        Ok(token)
    }

    async fn exchange(
        &self,
        code: &str,
        code_verifier: &str,
        deadline: Instant,
    ) -> Result<InstallToken, EnrollmentError> {
        let unreachable = |source| EnrollmentError::Failed {
            message: "Could not reach the RustOverlay API.".into(),
            source,
        };

        let url = self
            .options
            .base_url
            .join("/auth/exchange")
            .map_err(|e| EnrollmentError::failed(format!("Invalid backend URL: {e}")))?;
        let body =
            serde_json::to_vec(&ExchangeRequest { code, agent_version: crate::AGENT_VERSION, code_verifier })
                .map_err(EnrollmentError::Decode)?;

        let request = self.http.post(url).header(CONTENT_TYPE, JSON_CONTENT_TYPE).body(body).send();
        let response = match tokio::time::timeout_at(deadline, request).await {
            Ok(Ok(response)) => response,
            Ok(Err(e)) => return Err(unreachable(Some(e))),
            Err(_) => return Err(unreachable(None)),
        };

        if !response.status().is_success() {
            // The code is single-use and 60s-lived, so there is nothing to retry -- a failure here
            // means starting the whole flow again.
            return Err(EnrollmentError::failed(format!(
                "The API rejected this sign-in ({}).",
                response.status().as_u16()
            )));
        }

        let bytes = match tokio::time::timeout_at(deadline, response.bytes()).await {
            Ok(Ok(bytes)) => bytes,
            Ok(Err(e)) => return Err(unreachable(Some(e))),
            Err(_) => return Err(unreachable(None)),
        };

        crate::web_json::from_slice::<Option<InstallToken>>(&bytes)
            .map_err(EnrollmentError::Decode)?
            .ok_or_else(|| EnrollmentError::failed("The API returned an empty response."))
    }
}
