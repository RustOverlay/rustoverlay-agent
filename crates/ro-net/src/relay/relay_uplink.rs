use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::{Duration, SystemTime};

use chrono::TimeDelta;
use ro_core::events::GameEvent;
use ro_core::hosting::{DeliveryMode, GameEventBus};
use ro_vision::hosting::LoadoutState;
use tokio::io::{AsyncRead, AsyncWrite};

use super::proxy_tunnel::connect_with_system_proxy;
use super::session_changes::SessionChanges;
use super::{ActiveTeam, ActiveTeamStore, RelayConnection, RelayError, RelayOptions, TeamRoomState};
use super::{combat_to_proto, loadout_to_proto, vitals_to_proto};
use crate::enrollment::{InstallToken, TokenStore};
use crate::proto::{ClientFrame, Heartbeat, JoinRoom, client_frame::Payload, server_frame};
use crate::unix_millis;

const COMBAT_QUEUE_CAPACITY: usize = 512;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const CLOSE_TIMEOUT: Duration = Duration::from_secs(2);
const MAX_REFUSAL_BACKOFF: Duration = Duration::from_secs(300);
use refusal::REPLACED;

/// The relay's `Error.code`s.
pub mod refusal {
    /// The token is unknown, or its player is not a member of that room; the relay cannot say which.
    pub const BAD_TOKEN: &str = "bad_token";
    pub const ROOM_FULL: &str = "room_full";
    pub const VERSION_UNSUPPORTED: &str = "version_unsupported";
    /// Another session for this player joined the room.
    pub const REPLACED: &str = "replaced";
}

/// Why a session ended, which decides how long the uplink waits before the next.
enum Ended {
    /// No active team or no token, so nothing was sent.
    Inactive,
    /// The active team or the token was written while the session ran.
    Changed,
    /// The socket closed or failed.
    Closed,
    /// The relay sent an `Error` frame (bad_token, room_full, version_unsupported, replaced).
    Refused(String),
}

/// Keeps one relay session alive for the active team and pushes this player's state into it.
///
/// Nothing leaves the machine unless a team is active: with no active team this holds a closed
/// socket and sends nothing at all.
pub struct RelayUplink {
    teams: ActiveTeamStore,
    tokens: Arc<dyn TokenStore>,
    loadout: Arc<Mutex<LoadoutState>>,
    room: Arc<TeamRoomState>,
    bus: GameEventBus,
    options: RelayOptions,
    /// Set on the capture thread, read on the send loop. A missed flip costs one second.
    loadout_dirty: Arc<AtomicBool>,
}

/// Marks the room disconnected however `run` ends, including by its future being dropped.
struct Disconnect<'a>(&'a TeamRoomState);

impl Drop for Disconnect<'_> {
    fn drop(&mut self) {
        self.0.set_connected(false, None);
    }
}

impl RelayUplink {
    /// Subscribes to `loadout` for change notifications for the lifetime of the state.
    pub fn new(
        teams: ActiveTeamStore,
        tokens: Arc<dyn TokenStore>,
        loadout: Arc<Mutex<LoadoutState>>,
        room: Arc<TeamRoomState>,
        bus: GameEventBus,
        options: RelayOptions,
    ) -> Self {
        let loadout_dirty = Arc::new(AtomicBool::new(false));
        let dirty = loadout_dirty.clone();
        loadout
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .subscribe(move |_| dirty.store(true, Ordering::Relaxed));

        Self { teams, tokens, loadout, room, bus, options, loadout_dirty }
    }

    /// Runs sessions back to back until the future is dropped, which is how the app stops it. A
    /// write to the active team or the token ends the live session at once and starts the next.
    pub async fn run(&self) {
        let _disconnect = Disconnect(&self.room);
        let mut changes = SessionChanges::new(self.teams.changes(), self.tokens.changes());
        let mut refusals = 0;

        loop {
            changes.mark_seen();
            let ended = self.session(&mut changes).await.unwrap_or_else(|e| {
                tracing::warn!(error = %e, "Relay session ended");
                Ended::Closed
            });

            // Also wipes any snapshot the closing session applied after the UI cleared the roster.
            self.room.set_connected(false, None);

            let wait = match ended {
                Ended::Changed => {
                    refusals = 0;
                    continue;
                }
                // Another install of this account took the seat; retrying would evict it in turn.
                Ended::Refused(code) if code == REPLACED => None,
                Ended::Refused(_) => {
                    refusals += 1;
                    Some(refusal_backoff(duration(self.options.reconnect_delay), refusals))
                }
                Ended::Inactive | Ended::Closed => {
                    refusals = 0;
                    Some(duration(self.options.reconnect_delay))
                }
            };

            if changes.wait(wait).await {
                refusals = 0;
            }
        }
    }

    async fn session(&self, changes: &mut SessionChanges) -> Result<Ended, RelayError> {
        let (Some(team), Some(token)) = (self.teams.load(), self.tokens.load()) else {
            return Ok(Ended::Inactive);
        };

        let url = self.options.base_url.join("v1/room").map_err(|e| RelayError::Protocol(e.to_string()))?;
        let connect = tokio::time::timeout(CONNECT_TIMEOUT, connect_with_system_proxy(&url));
        let socket = tokio::select! {
            biased;
            () = changes.next() => return Ok(Ended::Changed),
            connected = connect => connected.map_err(|_| RelayError::Protocol("timed out connecting".into()))??,
        };

        let connection = RelayConnection::new(socket);
        let ended = tokio::select! {
            biased;
            () = changes.next() => Ok(Ended::Changed),
            ended = self.exchange(&connection, &team, &token) => ended,
        };

        let _ = tokio::time::timeout(CLOSE_TIMEOUT, connection.close()).await;
        ended
    }

    async fn exchange<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        connection: &RelayConnection<S>,
        team: &ActiveTeam,
        token: &InstallToken,
    ) -> Result<Ended, RelayError> {
        connection.send(&join(team, token)).await?;

        let pumps = async {
            tokio::try_join!(
                self.send_loadout(connection),
                self.send_combat(connection),
                self.send_heartbeats(connection),
            )?;
            // Pumps only finish early when the bus completes; the session lasts until the read ends.
            std::future::pending::<Result<Ended, RelayError>>().await
        };

        tokio::select! {
            read = self.read(connection) => read,
            pumped = pumps => pumped,
        }
    }

    async fn read<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        connection: &RelayConnection<S>,
    ) -> Result<Ended, RelayError> {
        let idle = Some(duration(self.options.idle_timeout)).filter(|idle| !idle.is_zero());
        while let Some(frame) = connection.receive(idle).await? {
            match frame.payload {
                Some(server_frame::Payload::Joined(joined)) => self.room.set_connected(true, Some(joined.region)),
                Some(server_frame::Payload::Snapshot(snapshot)) => self.room.apply_snapshot(&snapshot),
                Some(server_frame::Payload::MemberUpdate(update)) => self.room.apply_update(&update),
                Some(server_frame::Payload::Error(error)) => {
                    tracing::warn!(code = %error.code, message = %error.message, "Relay refused the session");
                    self.room.set_refused(Some(error.code.clone()));
                    return Ok(Ended::Refused(error.code));
                }
                _ => {}
            }
        }
        Ok(Ended::Closed)
    }

    async fn send_loadout<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        connection: &RelayConnection<S>,
    ) -> Result<(), RelayError> {
        let mut last_sent: Option<SystemTime> = None;

        loop {
            tokio::time::sleep(duration(self.options.min_loadout_interval)).await;

            let frames = {
                let state = self.loadout.lock().unwrap_or_else(PoisonError::into_inner);
                let due = self.loadout_dirty.load(Ordering::Relaxed)
                    || refresh_due(last_sent, self.options.loadout_refresh_interval);
                if !due || state.read_at().is_none() {
                    continue;
                }

                self.loadout_dirty.store(false, Ordering::Relaxed);
                last_sent = Some(SystemTime::now());

                let loadout = frame(Payload::Loadout(loadout_to_proto(&state)));
                let vitals = state.vitals().map(|v| frame(Payload::Vitals(vitals_to_proto(v, state.read_at()))));
                (loadout, vitals)
            };

            connection.send(&frames.0).await?;
            if let Some(vitals) = frames.1 {
                connection.send(&vitals).await?;
            }
        }
    }

    async fn send_combat<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        connection: &RelayConnection<S>,
    ) -> Result<(), RelayError> {
        // Lossless: a stale position is worth dropping, a kill is not.
        let subscription = self.bus.subscribe("relay", COMBAT_QUEUE_CAPACITY, DeliveryMode::Lossless);

        while let Some(event) = subscription.recv().await {
            if let GameEvent::Combat(combat) = event {
                connection.send(&frame(Payload::Combat(combat_to_proto(&combat)))).await?;
            }
        }
        Ok(())
    }

    async fn send_heartbeats<S: AsyncRead + AsyncWrite + Unpin>(
        &self,
        connection: &RelayConnection<S>,
    ) -> Result<(), RelayError> {
        loop {
            tokio::time::sleep(duration(self.options.heartbeat_interval)).await;
            let heartbeat = Heartbeat { sent_at_ms: unix_millis(SystemTime::now()) };
            connection.send(&frame(Payload::Heartbeat(heartbeat))).await?;
        }
    }
}

fn join(team: &ActiveTeam, token: &InstallToken) -> ClientFrame {
    frame(Payload::Join(JoinRoom {
        room: team.join_code.clone(),
        token: token.token.clone(),
        steam_id: token.steam_id as u64,
        display_name: token.display_name.clone(),
        agent_version: crate::AGENT_VERSION.into(),
        ..Default::default()
    }))
}

fn frame(payload: Payload) -> ClientFrame {
    ClientFrame { seq: 0, payload: Some(payload) }
}

/// `UtcNow - lastSent >= refresh`, with "never sent" as DateTimeOffset.MinValue. A clock that went
/// backwards gives a negative gap, which is never due.
fn refresh_due(last_sent: Option<SystemTime>, refresh: TimeDelta) -> bool {
    let Some(last) = last_sent else { return true };
    match SystemTime::now().duration_since(last) {
        Ok(gap) => TimeDelta::from_std(gap).is_ok_and(|gap| gap >= refresh),
        Err(_) => refresh <= TimeDelta::zero(),
    }
}

fn duration(span: TimeDelta) -> Duration {
    span.to_std().unwrap_or(Duration::ZERO)
}

/// `base` doubled per consecutive refusal, capped; each retry costs the API a join verification.
fn refusal_backoff(base: Duration, refusals: u32) -> Duration {
    base.saturating_mul(1 << refusals.min(16)).min(MAX_REFUSAL_BACKOFF)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refusal_backoff_doubles_up_to_five_minutes() {
        let base = Duration::from_secs(5);
        let waits: Vec<u64> = (1..=8).map(|n| refusal_backoff(base, n).as_secs()).collect();

        assert_eq!(waits, [10, 20, 40, 80, 160, 300, 300, 300]);
        assert_eq!(refusal_backoff(base, u32::MAX), MAX_REFUSAL_BACKOFF);
    }
}
