mod common;

use std::time::Duration;

use ro_core::events::{GameEvent, TransformTriple};
use ro_core::hosting::{DeliveryMode, GameEventBus};

fn triple(value: f64) -> GameEvent {
    GameEvent::Triple(TransformTriple {
        timestamp: common::utc(2026, 8, 2, 21, 41, 41, 0),
        a: value,
        b: value,
        c: value,
        label: None,
    })
}

fn a_of(event: Option<GameEvent>) -> f64 {
    match event {
        Some(GameEvent::Triple(t)) => t.a,
        other => panic!("expected a triple, got {other:?}"),
    }
}

#[tokio::test]
async fn every_subscriber_sees_every_event() {
    let bus = GameEventBus::new();
    let first = bus.subscribe("first", 8, DeliveryMode::Lossy);
    let second = bus.subscribe("second", 8, DeliveryMode::Lossy);

    bus.publish(&triple(1.0)).await.unwrap();

    assert_eq!(a_of(first.recv().await), 1.0);
    assert_eq!(a_of(second.recv().await), 1.0);
}

#[tokio::test]
async fn a_lossy_subscriber_drops_the_oldest_rather_than_blocking_the_publisher() {
    let bus = GameEventBus::new();
    let subscription = bus.subscribe("slow", 2, DeliveryMode::Lossy);

    for i in 1..=5 {
        bus.publish(&triple(f64::from(i))).await.unwrap();
    }

    // Capacity 2, so only the last two survive — and none of the publishes blocked.
    assert_eq!(a_of(subscription.recv().await), 4.0);
    assert_eq!(a_of(subscription.recv().await), 5.0);
}

#[tokio::test]
async fn a_full_lossless_subscriber_applies_backpressure() {
    let bus = GameEventBus::new();
    let subscription = bus.subscribe("relay", 1, DeliveryMode::Lossless);

    bus.publish(&triple(1.0)).await.unwrap();

    let publisher = bus.clone();
    let blocked = tokio::spawn(async move { publisher.publish(&triple(2.0)).await });
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(20)).await;

    assert!(!blocked.is_finished());

    subscription.recv().await;
    tokio::time::timeout(Duration::from_secs(1), blocked).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn disposing_a_subscription_stops_delivery() {
    let bus = GameEventBus::new();
    let subscription = bus.subscribe("gone", 8, DeliveryMode::Lossless);
    subscription.dispose();

    // Would deadlock against a full lossless queue if the subscriber were still attached.
    tokio::time::timeout(Duration::from_secs(1), bus.publish(&triple(1.0))).await.unwrap().unwrap();

    assert!(subscription.try_recv().is_none());
}

#[tokio::test]
async fn a_subscriber_dropped_while_the_publisher_waits_on_it_is_skipped_not_an_error() {
    let bus = GameEventBus::new();
    let relay = bus.subscribe("relay", 1, DeliveryMode::Lossless);
    let ui = bus.subscribe("ui", 8, DeliveryMode::Lossy);
    bus.publish(&triple(1.0)).await.unwrap();

    let publisher = bus.clone();
    let blocked = tokio::spawn(async move { publisher.publish(&triple(2.0)).await });
    tokio::task::yield_now().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert!(!blocked.is_finished());

    drop(relay);
    tokio::time::timeout(Duration::from_secs(1), blocked).await.unwrap().unwrap().unwrap();
    bus.publish(&triple(3.0)).await.unwrap();

    assert_eq!(a_of(ui.recv().await), 1.0);
    assert_eq!(a_of(ui.recv().await), 2.0);
    assert_eq!(a_of(ui.recv().await), 3.0);
}

#[tokio::test]
async fn completing_the_bus_ends_every_subscription_after_it_drains() {
    let bus = GameEventBus::new();
    let subscription = bus.subscribe("tail", 4, DeliveryMode::Lossless);

    bus.publish(&triple(1.0)).await.unwrap();
    bus.complete();

    assert_eq!(a_of(subscription.recv().await), 1.0);
    assert!(subscription.recv().await.is_none());
    assert!(bus.publish(&triple(2.0)).await.is_err());
}
