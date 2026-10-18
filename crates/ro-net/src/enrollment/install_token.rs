use serde::{Deserialize, Serialize};

/// What `POST /auth/exchange` returns. The token authenticates this install to the API and to the
/// relay; it is a bearer credential and never leaves a [`super::TokenStore`] in plaintext.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct InstallToken {
    pub token: String,
    #[serde(deserialize_with = "crate::web_json::number")]
    pub steam_id: i64,
    pub display_name: String,
}

impl InstallToken {
    pub fn new(token: impl Into<String>, steam_id: i64, display_name: impl Into<String>) -> Self {
        Self { token: token.into(), steam_id, display_name: display_name.into() }
    }
}
