use std::sync::{Arc, Mutex, PoisonError};

use super::TokenStore;

type Listener = Arc<dyn Fn(&AuthState) + Send + Sync>;

/// Whether this install still holds a credential the API accepts.
///
/// A token on disk is not proof of a working session: it survives revocation, and the only way to
/// learn it is dead is to use it. A 401 from `/v1/*` is that proof — the API raises it only for a
/// bad or revoked token, never for a membership failure — so `TeamsClient` reports one here and the
/// UI asks for a fresh sign-in.
///
/// The relay is deliberately not a source. Its `bad_token` also covers "not a member of that room",
/// so acting on it would sign out a user who merely left a team somewhere else.
pub struct AuthState {
    tokens: Arc<dyn TokenStore>,
    signed_out: Mutex<Option<SignedOut>>,
    listeners: Mutex<Vec<Listener>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignedOut {
    /// The API refused the token.
    Rejected,
    /// The player asked to sign out.
    ByPlayer,
}

impl AuthState {
    pub fn new(tokens: Arc<dyn TokenStore>) -> Self {
        Self { tokens, signed_out: Mutex::new(None), listeners: Mutex::new(Vec::new()) }
    }

    pub fn is_signed_out(&self) -> bool {
        self.signed_out().is_some()
    }

    pub fn signed_out(&self) -> Option<SignedOut> {
        *self.signed_out.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Listeners run on whichever thread changed the state, so UI subscribers must marshal.
    pub fn subscribe(&self, listener: impl Fn(&AuthState) + Send + Sync + 'static) {
        self.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::new(listener));
    }

    /// Discards the rejected token and announces that this install needs a new one.
    ///
    /// Clearing is what stops the broadcast: the store's change tick ends the relay session at once,
    /// and the uplink opens no new socket without a token.
    ///
    /// `rejected` is the bearer the 401 answered. A request still in flight from before a fresh
    /// sign-in comes back 401 for the old token, and must not discard the new one.
    pub fn sign_out(&self, rejected: &str) {
        if self.tokens.load().is_some_and(|stored| stored.token != rejected) {
            return;
        }

        // Concurrent calls are the normal case: several requests can be in flight when a token is
        // revoked, and they all come back 401.
        if !self.flip(Some(SignedOut::Rejected)) {
            return;
        }

        tracing::warn!("The API rejected this install's token; a new sign-in is required");
        if let Err(e) = self.tokens.clear() {
            tracing::warn!(error = %e, "Could not delete the rejected install token");
        }
        self.notify();
    }

    /// Forgets this install's token at the player's request. The server keeps it; nothing revokes.
    pub fn sign_out_by_player(&self) {
        if let Err(e) = self.tokens.clear() {
            tracing::warn!(error = %e, "Could not delete the install token");
        }
        if self.flip(Some(SignedOut::ByPlayer)) {
            self.notify();
        }
    }

    /// Records that enrolment has produced a fresh token.
    pub fn sign_in(&self) {
        if self.flip(None) {
            self.notify();
        }
    }

    /// Only the first change of a run counts, so concurrent 401s announce one sign-out.
    fn flip(&self, signed_out: Option<SignedOut>) -> bool {
        let mut state = self.signed_out.lock().unwrap_or_else(PoisonError::into_inner);
        if state.is_some() == signed_out.is_some() {
            return false;
        }
        *state = signed_out;
        true
    }

    fn notify(&self) {
        let listeners = self.listeners.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for listener in &listeners {
            listener(self);
        }
    }
}
