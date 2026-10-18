use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TailState {
    #[default]
    Idle,
    Tailing,
    NoLog,
    Faulted,
}

type Listener = Arc<dyn Fn(&AgentStatus) + Send + Sync>;

/// What the log tail is currently doing. Shared; services set it, the UI observes.
///
/// Listeners run on whichever thread called [`set`](Self::set) — a background one. Listeners that
/// touch UI state must marshal.
#[derive(Default)]
pub struct AgentStatus {
    current: Mutex<(TailState, String)>,
    listeners: Mutex<Vec<Listener>>,
}

impl AgentStatus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn state(&self) -> TailState {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).0
    }

    pub fn detail(&self) -> String {
        self.current.lock().unwrap_or_else(PoisonError::into_inner).1.clone()
    }

    pub fn on_changed(&self, listener: impl Fn(&AgentStatus) + Send + Sync + 'static) {
        self.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::new(listener));
    }

    pub fn set(&self, state: TailState, detail: impl Into<String>) {
        *self.current.lock().unwrap_or_else(PoisonError::into_inner) = (state, detail.into());

        let listeners = self.listeners.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for listener in listeners {
            listener(self);
        }
    }
}

impl std::fmt::Debug for AgentStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AgentStatus").field("state", &self.state()).field("detail", &self.detail()).finish()
    }
}
