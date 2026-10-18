//! The Steam sign-in dialog (`SignInViewModel`).

use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ro_net::enrollment::EnrollmentService;
use tokio::runtime::Handle;

use super::Notify;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SignInState {
    #[default]
    Ready,
    Waiting,
    Failed,
    /// Enrolment produced a token; the dialog is done.
    SignedIn,
}

#[derive(Default)]
pub struct SignInStatus {
    pub state: SignInState,
    pub error: Option<String>,
}

impl SignInStatus {
    pub fn is_waiting(&self) -> bool {
        self.state == SignInState::Waiting
    }

    pub fn can_sign_in(&self) -> bool {
        !matches!(self.state, SignInState::Waiting | SignInState::SignedIn)
    }

    pub fn has_failed(&self) -> bool {
        self.state == SignInState::Failed
    }

    pub fn action_text(&self) -> &'static str {
        if self.has_failed() { "Try again" } else { "Sign in with Steam" }
    }
}

pub struct SignInModel {
    enrollment: Arc<EnrollmentService>,
    runtime: Handle,
    notify: Notify,
    status: Mutex<SignInStatus>,
}

impl SignInModel {
    pub fn new(enrollment: Arc<EnrollmentService>, runtime: Handle, notify: Notify) -> Arc<Self> {
        Arc::new(Self { enrollment, runtime, notify, status: Mutex::default() })
    }

    pub fn status(&self) -> MutexGuard<'_, SignInStatus> {
        self.status.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Never called automatically: the browser opens because the user pressed a button, having
    /// read why. A window that launches a Steam sign-in page unprompted looks like malware.
    pub fn sign_in(self: &Arc<Self>) {
        {
            let mut status = self.status();
            status.state = SignInState::Waiting;
            status.error = None;
        }
        (self.notify)();

        let this = self.clone();
        self.runtime.spawn(async move {
            let outcome = this.enrollment.enroll().await;

            let mut status = this.status();
            match outcome {
                Ok(_) => status.state = SignInState::SignedIn,
                Err(e) => {
                    // TODO(port): C# caught only EnrollmentFailedException; IO/decode failures crashed.
                    tracing::warn!(error = %e, "Sign-in failed");
                    status.error = Some(e.to_string());
                    status.state = SignInState::Failed;
                }
            }
            drop(status);
            (this.notify)();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_button_reads_try_again_only_after_a_failure() {
        let mut status = SignInStatus::default();
        assert_eq!(status.action_text(), "Sign in with Steam");
        assert!(status.can_sign_in());

        status.state = SignInState::Waiting;
        assert!(!status.can_sign_in());
        assert!(status.is_waiting());

        status.state = SignInState::Failed;
        assert_eq!(status.action_text(), "Try again");
        assert!(status.can_sign_in());
    }
}
