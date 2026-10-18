use std::io;

use tokio::sync::watch;

use super::InstallToken;

/// Persistence for the install token. Implementations are expected to encrypt at rest; see
/// `DpapiTokenStore`, which is why this is a trait rather than a file path.
pub trait TokenStore: Send + Sync {
    /// None when this install has never enrolled, or the stored token is unreadable.
    fn load(&self) -> Option<InstallToken>;

    fn save(&self, token: &InstallToken) -> io::Result<()>;

    fn clear(&self) -> io::Result<()>;

    /// Ticks on every successful save or clear; the relay ends its session on one.
    fn changes(&self) -> watch::Receiver<u64>;
}
