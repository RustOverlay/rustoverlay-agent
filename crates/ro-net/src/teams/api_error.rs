use serde::{Deserialize, Serialize};

/// The API's error envelope: every DomainError serialises as `{code, message}`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ApiError {
    pub code: Option<String>,
    pub message: Option<String>,
}
