use std::sync::{Arc, Mutex, PoisonError};

use ro_core::hosting::{DeliveryMode, GameEventBus, Subscription};

use crate::state::Notify;
use crate::state::feed::EventFeed;

const QUEUE_CAPACITY: usize = 4096;
const MAX_BATCH: usize = 256;

/// Lossy: the feed is a debug view, and dropping the oldest rows beats stalling the tailer.
/// Subscribed before anything publishes, so the first events are not missed.
pub fn subscribe(bus: &GameEventBus) -> Subscription {
    bus.subscribe("ui", QUEUE_CAPACITY, DeliveryMode::Lossy)
}

/// Drains the event bus into the feed (`UiEventPumpService`). Batches, because a replay emits
/// thousands of events and one repaint per event would lock the window up.
pub async fn run(subscription: Subscription, feed: Arc<Mutex<EventFeed>>, notify: Notify) {
    while let Some(first) = subscription.recv().await {
        let mut batch = Vec::with_capacity(MAX_BATCH);
        batch.push(first);
        while batch.len() < MAX_BATCH {
            match subscription.try_recv() {
                Some(event) => batch.push(event),
                None => break,
            }
        }

        tracing::debug!("Dispatching {} events to the feed", batch.len());
        feed.lock().unwrap_or_else(PoisonError::into_inner).append(&batch);
        notify();
    }
}
