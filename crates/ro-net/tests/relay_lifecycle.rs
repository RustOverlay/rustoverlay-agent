//! When the uplink ends a session and how soon it starts the next.

mod common;

use std::time::Duration;

use chrono::TimeDelta;
use common::hexay;
use common::relay::{Harness, PATIENCE, closed, next, server_frame, squad, until};
use futures_util::SinkExt;
use ro_net::enrollment::TokenStore;
use ro_net::proto::{self, client_frame, server_frame};
use ro_net::relay::RelayOptions;
use tokio::net::TcpStream;
use tokio_tungstenite::WebSocketStream;

/// A reconnect delay far beyond the test's patience, so any prompt reconnect is the change signal.
fn slow_reconnect() -> RelayOptions {
    RelayOptions {
        heartbeat_interval: TimeDelta::milliseconds(50),
        reconnect_delay: TimeDelta::seconds(60),
        ..Default::default()
    }
}

async fn joined_room(socket: &mut WebSocketStream<TcpStream>) -> String {
    let Some(client_frame::Payload::Join(join)) = next(socket).await.payload else { panic!("join first") };
    join.room
}

async fn refuse(socket: &mut WebSocketStream<TcpStream>, code: &str) {
    let error = proto::Error { code: code.into(), message: String::new() };
    socket.send(server_frame(server_frame::Payload::Error(error))).await.unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn disconnecting_hangs_up_and_leaves_the_roster_empty() {
    let relay = Harness::start(slow_reconnect()).await;
    let mut socket = relay.accept().await;
    joined_room(&mut socket).await;

    let member = proto::Member { steam_id: 7, display_name: "Mate".into(), online: true, ..Default::default() };
    let snapshot = proto::TeamSnapshot { taken_at_ms: 0, members: vec![member] };
    socket.send(server_frame(server_frame::Payload::Snapshot(snapshot))).await.unwrap();
    until(|| relay.room.members().len() == 1).await;

    relay.teams.save(None).unwrap();

    closed(&mut socket).await;
    until(|| relay.room.members().is_empty()).await;
    assert!(!relay.connects_within(Duration::from_millis(300)).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn switching_teams_rejoins_the_new_room_at_once() {
    let relay = Harness::start(slow_reconnect()).await;
    let mut first = relay.accept().await;
    assert_eq!(joined_room(&mut first).await, "7926ZG");

    relay.teams.save(Some(&squad("B2B2B2"))).unwrap();

    closed(&mut first).await;
    let mut second = relay.accept().await;
    assert_eq!(joined_room(&mut second).await, "B2B2B2");
}

#[tokio::test(flavor = "multi_thread")]
async fn signing_out_hangs_up_and_stays_silent() {
    let relay = Harness::start(slow_reconnect()).await;
    let mut socket = relay.accept().await;
    joined_room(&mut socket).await;

    relay.tokens.clear().unwrap();

    closed(&mut socket).await;
    assert!(!relay.connects_within(Duration::from_millis(300)).await);
}

/// Two installs on one account would otherwise evict each other every reconnect delay.
#[tokio::test(flavor = "multi_thread")]
async fn a_replaced_session_waits_for_the_team_or_token_to_change() {
    let relay = Harness::start(RelayOptions { reconnect_delay: TimeDelta::milliseconds(20), ..slow_reconnect() }).await;
    let mut socket = relay.accept().await;
    joined_room(&mut socket).await;

    refuse(&mut socket, "replaced").await;

    closed(&mut socket).await;
    assert!(!relay.connects_within(Duration::from_millis(300)).await);
    relay.tokens.save(&hexay()).unwrap();
    let mut again = relay.accept().await;
    joined_room(&mut again).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn repeated_refusals_back_off() {
    let relay = Harness::start(RelayOptions { reconnect_delay: TimeDelta::milliseconds(100), ..slow_reconnect() }).await;

    // First refusal waits 2x the delay, the second 4x.
    for wait_ms in [200, 400] {
        let mut socket = relay.accept().await;
        joined_room(&mut socket).await;
        refuse(&mut socket, "bad_token").await;
        closed(&mut socket).await;
        assert!(!relay.connects_within(Duration::from_millis(wait_ms - 80)).await, "{wait_ms}");
    }
    relay.accept().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn a_silent_relay_is_dropped_and_redialled() {
    let options = RelayOptions {
        idle_timeout: TimeDelta::milliseconds(200),
        reconnect_delay: TimeDelta::milliseconds(20),
        ..slow_reconnect()
    };
    let relay = Harness::start(options).await;
    let mut socket = relay.accept().await;
    joined_room(&mut socket).await;

    tokio::time::timeout(PATIENCE, closed(&mut socket)).await.unwrap();

    let mut again = relay.accept().await;
    joined_room(&mut again).await;
}
