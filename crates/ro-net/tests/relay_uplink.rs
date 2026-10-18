//! The uplink against a loopback relay: join, room state, combat forwarding and the refusal path.

mod common;

use std::time::SystemTime;

use chrono::{DateTime, TimeDelta, Utc};
use common::relay::{Harness, PATIENCE, next, server_frame, until};
use futures_util::SinkExt;
use ro_core::events::{CombatEvent, GameEvent};
use ro_net::proto::{self, client_frame, server_frame};
use ro_net::relay::RelayOptions;
use ro_vision::hud::InventoryReading;
use tokio_tungstenite::tungstenite::handshake::server::Request;

fn combat() -> CombatEvent {
    CombatEvent {
        timestamp: DateTime::<Utc>::from(SystemTime::now()),
        attacker: "you".into(),
        target: "them".into(),
        weapon: "rifle.ak".into(),
        ammo: "ammo.rifle".into(),
        area: "head".into(),
        distance_metres: 40.0,
        old_hp: 100.0,
        new_hp: 0.0,
        info: Some("killed".into()),
        hits: 1,
        integrity: 1.0,
        travel_seconds: 0.1,
        mismatch_metres: 0.0,
        desync: 0,
        names_ambiguous: false,
    }
}

#[tokio::test(flavor = "multi_thread")]
#[allow(clippy::result_large_err)] // tungstenite's handshake Callback fixes the closure's error type.
async fn joins_the_active_team_and_mirrors_the_room() {
    let options = RelayOptions {
        heartbeat_interval: TimeDelta::milliseconds(50),
        min_loadout_interval: TimeDelta::milliseconds(20),
        reconnect_delay: TimeDelta::seconds(60),
        ..Default::default()
    };
    let relay = Harness::start(options).await;
    let (loadout, room, bus) = (&relay.loadout, &relay.room, &relay.bus);

    let (tcp, _) = tokio::time::timeout(PATIENCE, relay.listener.accept()).await.unwrap().unwrap();
    let mut socket = tokio_tungstenite::accept_hdr_async(tcp, |request: &Request, response| {
        assert_eq!(request.uri().path(), "/v1/room");
        Ok(response)
    })
    .await
    .unwrap();

    let Some(client_frame::Payload::Join(join)) = next(&mut socket).await.payload else { panic!("join first") };
    assert_eq!((join.room.as_str(), join.token.as_str()), ("7926ZG", "tok_live"));
    assert_eq!(join.steam_id, 76561198000000001);
    assert_eq!(join.agent_version.split('.').count(), 4);

    loadout.lock().unwrap().set(InventoryReading::default(), Some(2), None, false, SystemTime::now());

    let (mut saw_heartbeat, mut saw_loadout, mut saw_combat) = (false, false, false);
    while !(saw_heartbeat && saw_loadout && saw_combat) {
        match next(&mut socket).await.payload {
            Some(client_frame::Payload::Heartbeat(_)) => {
                saw_heartbeat = true;
                // The combat pump subscribes after the join is sent, so an earlier event is missed.
                if !saw_combat {
                    bus.publish(&GameEvent::Combat(combat())).await.unwrap();
                }
            }
            Some(client_frame::Payload::Loadout(l)) => saw_loadout = l.held_slot == Some(2),
            Some(client_frame::Payload::Combat(c)) => saw_combat = c.info == "killed",
            _ => {}
        }
    }

    let joined = proto::JoinAccepted { room: "7926ZG".into(), region: "eu".into(), server_time_ms: 0 };
    socket.send(server_frame(server_frame::Payload::Joined(joined))).await.unwrap();
    let update = proto::MemberUpdate { steam_id: 7, online: true, ..Default::default() };
    socket.send(server_frame(server_frame::Payload::MemberUpdate(update))).await.unwrap();
    until(|| room.connected() && room.members().len() == 1).await;
    assert_eq!(room.region().as_deref(), Some("eu"));

    let error = proto::Error { code: "bad_token".into(), message: "no".into() };
    socket.send(server_frame(server_frame::Payload::Error(error))).await.unwrap();
    until(|| !room.connected() && room.members().is_empty()).await;
}
