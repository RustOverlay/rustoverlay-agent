//! One subscriber's bounded queue: `Channel.CreateBounded` with DropOldest or Wait.

use std::collections::VecDeque;
use std::pin::pin;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tokio::sync::Notify;

use super::game_event_bus::{ChannelClosed, DeliveryMode};
use crate::events::GameEvent;

#[derive(Default)]
struct State {
    items: VecDeque<GameEvent>,
    completed: bool,
    detached: bool,
}

pub(super) struct EventQueue {
    capacity: usize,
    mode: DeliveryMode,
    state: Mutex<State>,
    readable: Notify,
    writable: Notify,
}

impl EventQueue {
    pub(super) fn new(capacity: usize, mode: DeliveryMode) -> Self {
        assert!(capacity >= 1, "a bounded channel needs a capacity of at least 1");
        Self { capacity, mode, state: Mutex::default(), readable: Notify::new(), writable: Notify::new() }
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub(super) async fn write(&self, event: GameEvent) -> Result<(), ChannelClosed> {
        // DropOldest makes the write total, so a lossy subscriber never blocks the publisher.
        if self.mode == DeliveryMode::Lossy {
            let mut state = self.lock();
            if !state.completed {
                if state.items.len() >= self.capacity {
                    state.items.pop_front();
                }
                state.items.push_back(event);
                drop(state);
                self.readable.notify_one();
            }
            return Ok(());
        }

        loop {
            let mut room = pin!(self.writable.notified());
            room.as_mut().enable();
            {
                let mut state = self.lock();
                // A subscriber that left is skipped, so a relay disconnect cannot fault the tail.
                if state.detached {
                    return Ok(());
                }
                if state.completed {
                    return Err(ChannelClosed);
                }
                if state.items.len() < self.capacity {
                    state.items.push_back(event);
                    drop(state);
                    self.readable.notify_one();
                    return Ok(());
                }
            }
            room.await;
        }
    }

    pub(super) fn try_read(&self) -> Option<GameEvent> {
        let event = self.lock().items.pop_front()?;
        self.writable.notify_one();
        Some(event)
    }

    /// `None` once the queue is completed and drained.
    pub(super) async fn read(&self) -> Option<GameEvent> {
        loop {
            let mut available = pin!(self.readable.notified());
            available.as_mut().enable();
            {
                let mut state = self.lock();
                if let Some(event) = state.items.pop_front() {
                    drop(state);
                    self.writable.notify_one();
                    return Some(event);
                }
                if state.completed {
                    return None;
                }
            }
            available.await;
        }
    }

    pub(super) fn complete(&self) {
        self.lock().completed = true;
        self.wake_all();
    }

    /// Completes the queue for a subscriber that has left the bus; writes to it then succeed as no-ops.
    pub(super) fn detach(&self) {
        let mut state = self.lock();
        state.completed = true;
        state.detached = true;
        drop(state);
        self.wake_all();
    }

    fn wake_all(&self) {
        self.readable.notify_waiters();
        self.writable.notify_waiters();
    }
}
