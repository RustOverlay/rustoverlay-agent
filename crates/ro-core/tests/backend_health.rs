mod common;

use chrono::{DateTime, TimeDelta, Utc};
use ro_core::capture::BackendHealth;

fn now() -> DateTime<Utc> {
    common::utc(2026, 1, 1, 12, 0, 0, 0)
}

const COOL_DOWN: TimeDelta = TimeDelta::minutes(1);

fn health() -> BackendHealth {
    BackendHealth::new(3, COOL_DOWN)
}

#[test]
fn prefers_the_preferred_backend_to_begin_with() {
    assert!(health().use_preferred(now()));
}

#[test]
fn tolerates_failures_below_the_threshold() {
    let mut health = health();

    assert!(!health.record_failure(now()));
    assert!(!health.record_failure(now()));
    assert!(health.use_preferred(now()));
}

#[test]
fn demotes_on_the_threshold_failure() {
    let mut health = health();

    health.record_failure(now());
    health.record_failure(now());

    assert!(health.record_failure(now()));
    assert!(!health.use_preferred(now()));
}

#[test]
fn reports_the_demotion_only_once() {
    let mut health = health();

    for _ in 0..3 {
        health.record_failure(now());
    }

    assert!(!health.record_failure(now()));
}

#[test]
fn retries_the_preferred_backend_once_the_cool_down_expires() {
    let mut health = health();

    for _ in 0..3 {
        health.record_failure(now());
    }

    assert!(!health.use_preferred(now() + COOL_DOWN - TimeDelta::seconds(1)));
    assert!(health.use_preferred(now() + COOL_DOWN));
}

#[test]
fn success_resets_the_run_so_intermittent_failures_never_accumulate() {
    let mut health = health();

    for _ in 0..10 {
        health.record_failure(now());
        health.record_failure(now());
        health.record_success();
    }

    assert!(health.use_preferred(now()));
}

#[test]
fn can_demote_again_after_recovering() {
    let mut health = health();
    let later = now() + COOL_DOWN;

    for _ in 0..3 {
        health.record_failure(now());
    }
    for _ in 0..2 {
        assert!(!health.record_failure(later));
    }

    assert!(health.record_failure(later));
    assert!(!health.use_preferred(later));
    assert!(health.use_preferred(later + COOL_DOWN));
}
