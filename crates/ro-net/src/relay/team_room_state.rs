use std::cmp::Ordering;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use indexmap::IndexMap;

use crate::proto;

#[derive(Debug, Clone, PartialEq)]
pub struct TeamMemberView {
    pub steam_id: u64,
    pub display_name: String,
    pub online: bool,
    pub vitals: Option<proto::Vitals>,
    pub loadout: Option<proto::Loadout>,
}

type Listener = Arc<dyn Fn(&TeamRoomState) + Send + Sync>;

#[derive(Default)]
struct Inner {
    connected: bool,
    region: Option<String>,
    /// The last refusal's code, kept across the reconnect wait and cleared by the next join.
    refused: Option<String>,
    // Insertion-ordered, as Dictionary enumerated, so the stable sort breaks ties the same way.
    members: IndexMap<u64, TeamMemberView>,
    sorted: Arc<[TeamMemberView]>,
}

/// The room as this agent currently believes it to be.
///
/// State, not events, for the same reason as `LoadoutState`: only the latest matters. Listeners run
/// on the relay read loop, so UI subscribers must marshal.
///
/// A `MemberUpdate` carries only the fields that changed, so an absent vitals or loadout means
/// "unchanged", never "cleared". The relay's periodic snapshot is what repairs a dropped update.
#[derive(Default)]
pub struct TeamRoomState {
    inner: Mutex<Inner>,
    listeners: Mutex<Vec<Listener>>,
}

impl TeamRoomState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connected(&self) -> bool {
        self.lock().connected
    }

    pub fn region(&self) -> Option<String> {
        self.lock().region.clone()
    }

    /// Online first, then by display name.
    pub fn members(&self) -> Arc<[TeamMemberView]> {
        self.lock().sorted.clone()
    }

    pub fn subscribe(&self, listener: impl Fn(&TeamRoomState) + Send + Sync + 'static) {
        self.listeners.lock().unwrap_or_else(PoisonError::into_inner).push(Arc::new(listener));
    }

    /// See [`super::refusal`] for the codes.
    pub fn refused(&self) -> Option<String> {
        self.lock().refused.clone()
    }

    pub fn set_refused(&self, code: Option<String>) {
        self.lock().refused = code;
        self.notify();
    }

    pub fn set_connected(&self, connected: bool, region: Option<String>) {
        {
            let mut inner = self.lock();
            inner.connected = connected;
            inner.region = region;
            if connected {
                inner.refused = None;
            } else {
                inner.members.clear();
                publish(&mut inner);
            }
        }
        self.notify();
    }

    pub fn apply_snapshot(&self, snapshot: &proto::TeamSnapshot) {
        {
            let mut inner = self.lock();
            inner.members.clear();
            for member in &snapshot.members {
                inner.members.insert(
                    member.steam_id,
                    TeamMemberView {
                        steam_id: member.steam_id,
                        display_name: member.display_name.clone(),
                        online: member.online,
                        vitals: member.vitals.clone(),
                        loadout: member.loadout.clone(),
                    },
                );
            }
            publish(&mut inner);
        }
        self.notify();
    }

    pub fn apply_update(&self, update: &proto::MemberUpdate) {
        {
            let mut inner = self.lock();
            // Ahead of the snapshot that would name them; the next correction fills in the name.
            let member = inner.members.entry(update.steam_id).or_insert_with(|| TeamMemberView {
                steam_id: update.steam_id,
                display_name: String::new(),
                online: false,
                vitals: None,
                loadout: None,
            });

            member.online = update.online;
            if let Some(vitals) = &update.vitals {
                member.vitals = Some(vitals.clone());
            }
            if let Some(loadout) = &update.loadout {
                member.loadout = Some(loadout.clone());
            }
            publish(&mut inner);
        }
        self.notify();
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn notify(&self) {
        let listeners = self.listeners.lock().unwrap_or_else(PoisonError::into_inner).clone();
        for listener in &listeners {
            listener(self);
        }
    }
}

fn publish(inner: &mut Inner) {
    let mut members: Vec<TeamMemberView> = inner.members.values().cloned().collect();
    members.sort_by(|a, b| b.online.cmp(&a.online).then_with(|| culture_compare(&a.display_name, &b.display_name)));
    inner.sorted = members.into();
}

/// Approximates `string.Compare` under an English culture: case-insensitive first, lowercase
/// before uppercase on a tie.
// TODO(port): .NET used the current culture's ICU collation; accents and punctuation may order differently.
fn culture_compare(a: &str, b: &str) -> Ordering {
    let folded = a.to_lowercase().cmp(&b.to_lowercase());
    folded.then_with(|| {
        let flip = |s: &str| s.chars().map(|c| if c.is_uppercase() { 1u8 } else { 0 }).collect::<Vec<_>>();
        flip(a).cmp(&flip(b))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn member(steam_id: u64, name: &str, online: bool) -> proto::Member {
        proto::Member { steam_id, display_name: name.into(), online, ..Default::default() }
    }

    #[test]
    fn a_snapshot_orders_online_members_first_then_by_name() {
        let room = TeamRoomState::new();
        room.apply_snapshot(&proto::TeamSnapshot {
            taken_at_ms: 0,
            members: vec![member(1, "zed", true), member(2, "Bob", false), member(3, "alice", true)],
        });

        let names: Vec<_> = room.members().iter().map(|m| m.display_name.clone()).collect();
        assert_eq!(names, ["alice", "zed", "Bob"]);
    }

    #[test]
    fn an_update_without_vitals_keeps_the_previous_ones() {
        let room = TeamRoomState::new();
        let vitals = proto::Vitals { health: 80.0, ..Default::default() };
        room.apply_snapshot(&proto::TeamSnapshot {
            taken_at_ms: 0,
            members: vec![proto::Member { vitals: Some(vitals.clone()), ..member(1, "a", true) }],
        });

        room.apply_update(&proto::MemberUpdate { steam_id: 1, online: false, ..Default::default() });

        let only = &room.members()[0];
        assert!(!only.online);
        assert_eq!(only.vitals, Some(vitals));
    }

    #[test]
    fn an_update_ahead_of_the_snapshot_adds_a_nameless_member() {
        let room = TeamRoomState::new();
        room.apply_update(&proto::MemberUpdate { steam_id: 9, online: true, ..Default::default() });

        assert_eq!(room.members()[0].display_name, "");
    }

    #[test]
    fn disconnecting_forgets_the_room() {
        let room = TeamRoomState::new();
        let changes = Arc::new(Mutex::new(0));
        let counter = changes.clone();
        room.subscribe(move |_| *counter.lock().unwrap() += 1);

        room.set_connected(true, Some("eu".into()));
        room.apply_update(&proto::MemberUpdate { steam_id: 9, online: true, ..Default::default() });
        room.set_connected(false, None);

        assert!(room.members().is_empty());
        assert_eq!(room.region(), None);
        assert_eq!(*changes.lock().unwrap(), 3);
    }
}
