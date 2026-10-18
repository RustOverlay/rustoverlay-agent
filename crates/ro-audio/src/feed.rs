use std::collections::VecDeque;

use crate::ShotEstimate;

const KEEP: usize = 20;

/// The most recent estimates and the capture's status line, shared with whatever displays them.
#[derive(Debug, Default)]
pub struct ShotFeed {
    status: String,
    shots: VecDeque<ShotEstimate>,
}

impl ShotFeed {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn status(&self) -> &str {
        &self.status
    }

    pub fn set_status(&mut self, status: impl Into<String>) {
        self.status = status.into();
    }

    pub fn push(&mut self, shot: ShotEstimate) {
        if self.shots.len() == KEEP {
            self.shots.pop_back();
        }
        self.shots.push_front(shot);
    }

    /// Newest first.
    pub fn shots(&self) -> impl Iterator<Item = &ShotEstimate> {
        self.shots.iter()
    }
}
