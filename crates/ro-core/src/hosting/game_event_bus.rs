use std::sync::{Arc, Mutex, PoisonError};

use super::event_queue::EventQueue;
use crate::events::GameEvent;

/// Whether a subscriber may lose events when it falls behind. See [`GameEventBus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryMode {
    /// Oldest events are discarded when the queue is full.
    Lossy,
    /// The publisher waits for room. Applies backpressure to every other subscriber.
    Lossless,
}

/// A lossless write hit a subscriber that was already completed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChannelClosed;

impl std::fmt::Display for ChannelClosed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the subscriber's channel has been completed")
    }
}

impl std::error::Error for ChannelClosed {}

type Subscribers = Arc<[(Arc<EventQueue>, u64)]>;

#[derive(Default)]
struct Inner {
    subscribers: Mutex<(Subscribers, u64)>,
}

/// Fans one producer out to many consumers, each with its own queue and its own loss policy.
///
/// Every subscriber gets a dedicated queue and the publisher writes to each in subscription order.
/// The policy is per-subscriber because it differs by event: a stale position is worth dropping, a
/// kill is not. The cost is that one [`DeliveryMode::Lossless`] subscriber can stall the whole
/// bus. That is the intended trade — the log file on disk is the real buffer, so a slow consumer
/// makes the tailer fall behind rather than lose anything.
#[derive(Clone, Default)]
pub struct GameEventBus {
    inner: Arc<Inner>,
}

impl GameEventBus {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn subscribe(&self, name: impl Into<String>, capacity: usize, mode: DeliveryMode) -> Subscription {
        let queue = Arc::new(EventQueue::new(capacity, mode));
        let mut guard = self.inner.subscribers.lock().unwrap_or_else(PoisonError::into_inner);
        let id = guard.1;
        guard.1 += 1;
        guard.0 = guard.0.iter().cloned().chain([(queue.clone(), id)]).collect();

        Subscription { bus: self.inner.clone(), queue, id, name: name.into() }
    }

    /// Delivers to every subscriber; drop the future to cancel. Fails only when a lossless
    /// subscriber was completed by [`complete`](Self::complete), as a write to a completed channel
    /// threw in C#; a subscriber that detached is skipped instead.
    pub async fn publish(&self, event: &GameEvent) -> Result<(), ChannelClosed> {
        // Snapshot, then release: publishing awaits, and a lock held across an await is unwanted.
        let subscribers = self.snapshot();

        for (queue, _) in subscribers.iter() {
            queue.write(event.clone()).await?;
        }
        Ok(())
    }

    /// Signals every subscriber that no further events are coming.
    pub fn complete(&self) {
        for (queue, _) in self.snapshot().iter() {
            queue.complete();
        }
    }

    fn snapshot(&self) -> Subscribers {
        self.inner.subscribers.lock().unwrap_or_else(PoisonError::into_inner).0.clone()
    }
}

/// One subscriber's end of the bus. Dropping it (or [`dispose`](Self::dispose)) detaches it.
pub struct Subscription {
    bus: Arc<Inner>,
    queue: Arc<EventQueue>,
    id: u64,
    name: String,
}

impl Subscription {
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The next event, or `None` once the bus is completed and this queue drained.
    pub async fn recv(&self) -> Option<GameEvent> {
        self.queue.read().await
    }

    pub fn try_recv(&self) -> Option<GameEvent> {
        self.queue.try_read()
    }

    /// Detaches from the bus and completes the queue. Idempotent.
    pub fn dispose(&self) {
        let mut guard = self.bus.subscribers.lock().unwrap_or_else(PoisonError::into_inner);
        guard.0 = guard.0.iter().filter(|(_, id)| *id != self.id).cloned().collect();
        drop(guard);

        self.queue.detach();
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        self.dispose();
    }
}
