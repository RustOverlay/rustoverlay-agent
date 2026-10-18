//! The Team tab: pick a team, or see the one you are broadcasting to (`TeamViewModel`).

use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use ro_net::enrollment::AuthState;
use ro_net::relay::{ActiveTeam, ActiveTeamStore, TeamRoomState, refusal};
use ro_net::teams::{TeamDto, TeamsApiError, TeamsClient};
use tokio::runtime::Handle;
use uuid::Uuid;

use super::Notify;
use super::team_member::TeamMemberCard;
use super::team_refresh::{Refreshed, on_refresh};
use crate::host::LazyCatalog;

/// What the tab draws. The text fields are edited in place by the view.
#[derive(Default)]
pub struct TeamState {
    pub teams: Vec<TeamDto>,
    pub active: Option<ActiveTeam>,
    pub members: Vec<TeamMemberCard>,
    pub new_team_name: String,
    pub join_code_entry: String,
    pub busy: bool,
    pub error: String,
}

pub struct TeamModel {
    client: Arc<TeamsClient>,
    store: ActiveTeamStore,
    room: Arc<TeamRoomState>,
    auth: Arc<AuthState>,
    catalog: Arc<LazyCatalog>,
    runtime: Handle,
    notify: Notify,
    state: Mutex<TeamState>,
    /// Set by Disconnect, so a refresh does not reconnect what the player just stopped.
    disconnected: AtomicBool,
    /// Whether the current `bad_token` refusal has been put to the API yet; see `on_room_changed`.
    refusal_checked: AtomicBool,
}

impl TeamModel {
    pub fn new(
        client: Arc<TeamsClient>,
        store: ActiveTeamStore,
        room: Arc<TeamRoomState>,
        auth: Arc<AuthState>,
        catalog: Arc<LazyCatalog>,
        runtime: Handle,
        notify: Notify,
    ) -> Arc<Self> {
        let active = store.load();
        Arc::new(Self {
            client,
            store,
            room,
            auth,
            catalog,
            runtime,
            notify,
            state: Mutex::new(TeamState { active, ..TeamState::default() }),
            disconnected: AtomicBool::new(false),
            refusal_checked: AtomicBool::new(false),
        })
    }

    pub fn state(&self) -> MutexGuard<'_, TeamState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Signed out is checked first because the uplink stops opening sockets the moment the token is
    /// cleared, so the honest "Connecting…" would otherwise sit there forever.
    pub fn connection(&self) -> String {
        if self.auth.is_signed_out() {
            "Signed out".into()
        } else if self.state().active.is_none() {
            "Not broadcasting".into()
        } else if self.room.connected() {
            format!("Connected — {}", self.room.region().unwrap_or_default())
        } else if let Some(code) = self.room.refused() {
            refusal_text(&code)
        } else {
            "Connecting…".into()
        }
    }

    /// Run on every room change, on the relay's thread.
    ///
    /// A `bad_token` refusal is put to the API once, as a team refresh: a 401 there is the proof
    /// that signs out, and a 200 without this team clears it. Waiting for the player to open the
    /// tab left them on "Connecting…" for minutes behind a dead token.
    pub fn on_room_changed(self: &Arc<Self>) {
        match self.room.refused().as_deref() {
            Some(refusal::BAD_TOKEN) => {
                if !self.refusal_checked.swap(true, Ordering::Relaxed) {
                    self.refresh();
                }
            }
            None => self.refusal_checked.store(false, Ordering::Relaxed),
            Some(_) => {}
        }

        let catalog = self.catalog.get();
        let members = self.room.members().iter().map(|m| TeamMemberCard::of(m, catalog.as_deref())).collect();
        self.state().members = members;
        (self.notify)();
    }

    pub fn refresh(self: &Arc<Self>) {
        let this = self.clone();
        self.runtime.spawn(async move { this.refresh_now().await });
    }

    pub fn create(self: &Arc<Self>) {
        let name = self.state().new_team_name.trim().to_owned();
        if name.is_empty() {
            return;
        }

        let this = self.clone();
        self.runtime.spawn(async move {
            this.run(async {
                let team = this.client.create(&name).await?;
                this.activate(&team);
                this.state().new_team_name.clear();
                Ok(())
            })
            .await;
            this.refresh_now().await;
        });
    }

    pub fn join(self: &Arc<Self>) {
        let code = upper_invariant(self.state().join_code_entry.trim());
        if code.is_empty() {
            return;
        }

        let this = self.clone();
        self.runtime.spawn(async move {
            this.run(async {
                let team = this.client.join(&code).await?;
                this.activate(&team);
                this.state().join_code_entry.clear();
                Ok(())
            })
            .await;
            this.refresh_now().await;
        });
    }

    /// Leave a team from the lobby list, which is the only place leaving is offered — it is
    /// membership, not a broadcast, and belongs beside the team it destroys rather than inside the
    /// session view where it reads as the twin of Disconnect.
    pub fn leave(self: &Arc<Self>, team: TeamDto) {
        let this = self.clone();
        self.runtime.spawn(async move {
            this.run(async {
                this.client.leave(team.id).await?;
                // The list only renders while disconnected, so this is defensive: a team activated in
                // another window must not leave the uplink retrying a room the API removed it from.
                if this.state().active.as_ref().is_some_and(|a| a.team_id == team.id.to_string()) {
                    this.apply(None);
                }
                Ok(())
            })
            .await;
            this.refresh_now().await;
        });
    }

    pub fn rotate_code(self: &Arc<Self>) {
        let Some(active) = self.state().active.clone() else { return };

        let this = self.clone();
        self.runtime.spawn(async move {
            this.run(async {
                // TODO(port): C# Guid.Parse threw on a corrupt active-team.json; this reports it instead.
                let id = Uuid::parse_str(&active.team_id).map_err(|e| TeamsApiError {
                    message: format!("The saved team id is invalid: {e}"),
                    status: None,
                    code: None,
                    source: None,
                })?;
                let team = this.client.rotate_join_code(id).await?;
                this.activate(&team);
                Ok(())
            })
            .await;
        });
    }

    pub fn connect(&self, team: &TeamDto) {
        self.activate(team);
    }

    /// Stop broadcasting, without leaving the team. Back to the list.
    pub fn disconnect(&self) {
        self.disconnected.store(true, Ordering::Relaxed);
        self.apply(None);
    }

    async fn refresh_now(&self) {
        self.run(async {
            let mine = self.client.list().await?;
            let active = self.state().active.clone();
            match on_refresh(active.as_ref(), &mine, self.disconnected.load(Ordering::Relaxed)) {
                Refreshed::Adopt(team) => self.activate(team),
                Refreshed::Clear => self.apply(None),
                Refreshed::Keep => {}
            }
            self.state().teams = mine;
            Ok(())
        })
        .await;
    }

    fn activate(&self, team: &TeamDto) {
        self.disconnected.store(false, Ordering::Relaxed);
        self.apply(Some(ActiveTeam {
            team_id: team.id.to_string(),
            join_code: team.join_code.clone(),
            name: team.name.clone(),
        }));
    }

    fn apply(&self, team: Option<ActiveTeam>) {
        if let Err(e) = self.store.save(team.as_ref()) {
            // TODO(port): C# let a failed write escape to the dispatcher's unhandled-exception box.
            tracing::error!(error = %e, "Could not save the active team");
        }

        let mut state = self.state();
        if team.is_none() {
            state.members.clear();
        }
        state.active = team;
        drop(state);

        // The refusal was for the team just replaced.
        self.room.set_refused(None);
        (self.notify)();
    }

    async fn run(&self, action: impl Future<Output = Result<(), TeamsApiError>>) {
        {
            let mut state = self.state();
            state.busy = true;
            state.error.clear();
        }
        (self.notify)();

        let outcome = action.await;

        let mut state = self.state();
        if let Err(e) = outcome {
            state.error = e.message;
        }
        state.busy = false;
        drop(state);
        (self.notify)();
    }
}

fn refusal_text(code: &str) -> String {
    match code {
        refusal::BAD_TOKEN => "The relay refused this team — it may have been deleted, or you are no longer a member".into(),
        refusal::ROOM_FULL => "This team's room is full".into(),
        refusal::VERSION_UNSUPPORTED => "This version of RustOverlay is too old for the relay — update it".into(),
        refusal::REPLACED => "Broadcasting from another install of your account".into(),
        other => format!("The relay refused this team ({other})"),
    }
}

/// The join-code box upper-cases as it is typed and holds at most twelve characters.
pub fn normalize_join_code_entry(entry: &mut String) {
    let normalized: String = upper_invariant(entry).chars().take(12).collect();
    if *entry != normalized {
        *entry = normalized;
    }
}

/// `ToUpperInvariant`: one-to-one simple case mapping, so "ß" stays one character rather than "SS".
fn upper_invariant(text: &str) -> String {
    text.chars()
        .map(|c| {
            let mut upper = c.to_uppercase();
            match (upper.next(), upper.next()) {
                (Some(single), None) => single,
                _ => c,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_codes_are_upper_cased_and_capped() {
        let mut entry = "abcdefghijklmnop".to_owned();
        normalize_join_code_entry(&mut entry);
        assert_eq!(entry, "ABCDEFGHIJKL");
    }

    #[test]
    fn upper_casing_never_changes_the_length_like_dotnet_invariant() {
        assert_eq!(upper_invariant("straße-é1"), "STRAßE-É1");
    }
}
