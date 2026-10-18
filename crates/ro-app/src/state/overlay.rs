//! The teammates drawn over the game (`OverlayViewModel`).
//!
//! Ordered by SteamID, deliberately not by name as the room is. Display names arrive only on the
//! periodic snapshot, so a name-ordered panel reshuffles a few seconds after a teammate connects;
//! an arbitrary but fixed order means a row stays where the player learned to look for it.

use std::collections::HashSet;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use ro_core::hosting::OverlaySettingsStore;
use ro_net::relay::{TeamMemberView, TeamRoomState};
use ro_vision::items::LoadoutCounts;
use ro_vision::items::wire::Loadout;

use super::Notify;
use super::inspect::InspectView;
use super::overlay_member::OverlayMemberRow;
use crate::host::LazyCatalog;
use crate::icons::ItemIcons;

#[derive(Default)]
pub struct OverlayState {
    pub members: Vec<OverlayMemberRow>,
    /// The expanded inventory of whichever row is selected.
    pub inspect: InspectView,
    pub panel_visible: bool,
    /// Every row's inspector, built when the room changes so a cycle or select is a swap: built on
    /// the press, a full inventory's first icon decode held the input thread ~65 ms.
    inspects: Vec<InspectView>,
    selected: usize,
    reported: HashSet<String>,
}

pub struct OverlayModel {
    room: Arc<TeamRoomState>,
    catalog: Arc<LazyCatalog>,
    icons: Arc<ItemIcons>,
    settings: Arc<OverlaySettingsStore>,
    /// The room seeds every roster entry including the joiner, so without this the player's own row
    /// is drawn back at them — stale by up to a snapshot interval, since their own updates are never
    /// echoed. Changes when the player signs in as another account.
    own_steam_id: AtomicU64,
    notify: Notify,
    state: Mutex<OverlayState>,
}

impl OverlayModel {
    pub fn new(
        room: Arc<TeamRoomState>,
        catalog: Arc<LazyCatalog>,
        icons: Arc<ItemIcons>,
        settings: Arc<OverlaySettingsStore>,
        own_steam_id: u64,
        notify: Notify,
    ) -> Arc<Self> {
        let own_steam_id = AtomicU64::new(own_steam_id);
        Arc::new(Self { room, catalog, icons, settings, own_steam_id, notify, state: Mutex::default() })
    }

    pub fn set_own_steam_id(&self, steam_id: u64) {
        if self.own_steam_id.swap(steam_id, Ordering::Relaxed) != steam_id {
            self.reconcile();
        }
    }

    pub fn state(&self) -> MutexGuard<'_, OverlayState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Whether there is anything worth drawing; an empty panel should not be on screen.
    pub fn has_members(&self) -> bool {
        !self.state().members.is_empty()
    }

    /// Run on every room change, on the relay's thread.
    pub fn reconcile(&self) {
        let catalog = self.catalog.get();
        let stale_after = self.settings.current().stale_after.to_std().unwrap_or_default();
        let counts = LoadoutCounts::new(catalog.clone(), stale_after);
        let now = SystemTime::now();

        let own = self.own_steam_id.load(Ordering::Relaxed);
        let mut wanted: Vec<TeamMemberView> =
            self.room.members().iter().filter(|m| m.online && m.steam_id != own).cloned().collect();
        wanted.sort_by_key(|m| m.steam_id);

        // Built before locking: rows and inspectors load icons, and the input and UI threads wait on this lock.
        let members: Vec<OverlayMemberRow> =
            wanted.iter().map(|m| OverlayMemberRow::of(m, &counts, &self.icons, now)).collect();
        let inspects: Vec<InspectView> =
            wanted.iter().map(|m| InspectView::of(Some(m), catalog.as_deref(), &self.icons)).collect();
        let unmapped: Vec<Option<String>> = wanted
            .iter()
            .map(|m| counts.unmapped_weapon(m.loadout.clone().map(Into::<Loadout>::into).as_ref()))
            .collect();

        let mut state = self.state();
        state.members = members;
        state.inspects = inspects;
        for weapon in unmapped {
            report(&mut state.reported, weapon);
        }

        // A teammate disconnecting must not leave the selection pointing past the end of the list.
        state.selected = state.selected.min(state.members.len().saturating_sub(1));
        show_selected(&mut state);
        drop(state);

        (self.notify)();
    }

    /// Moves to the next row, wrapping. The selection survives the inspector closing.
    pub fn cycle_selection(&self) {
        let len = self.state().members.len();
        if len > 0 {
            let next = (self.state().selected + 1) % len;
            self.select(next);
        }
    }

    /// Selects a row by position, ignoring an index past the end of a short roster.
    pub fn select(&self, index: usize) {
        let mut state = self.state();
        if index >= state.members.len() || index == state.selected {
            return;
        }

        state.selected = index;
        show_selected(&mut state);
        drop(state);

        (self.notify)();
    }

    /// Whether the panel is wanted on screen at all; see `OverlayInput::show`. True when it changed.
    pub fn set_panel_visible(&self, visible: bool) -> bool {
        let mut state = self.state();
        std::mem::replace(&mut state.panel_visible, visible) != visible
    }

    /// True when that changed what is drawn.
    pub fn set_inspect_visible(&self, visible: bool) -> bool {
        let mut state = self.state();
        let changed = state.inspect.visible != visible;
        state.inspect.visible = visible;
        changed
    }
}

/// Marks the selected row and puts its prebuilt inspector up, keeping whether it is open.
fn show_selected(state: &mut OverlayState) {
    let selected = state.selected;
    for (i, row) in state.members.iter_mut().enumerate() {
        row.is_selected = i == selected;
    }
    let built = state.inspects.get(selected).cloned().unwrap_or_default();
    state.inspect = InspectView { visible: state.inspect.visible, ..built };
}

/// Names a held firearm `WeaponAmmo` has no entry for, once per weapon per run.
///
/// The table is hand-authored against a game that adds guns, and its failure is a blank spare count
/// rather than an error. Without this the panel would just quietly stop showing a number.
fn report(reported: &mut HashSet<String>, weapon: Option<String>) {
    let Some(weapon) = weapon else { return };
    if reported.insert(weapon.clone()) {
        tracing::warn!("No ammunition mapped for held weapon '{weapon}'; its spare count will be blank");
    }
}

#[cfg(test)]
mod tests {
    use ro_core::hosting::OverlayOptions;
    use ro_core::install::{RustInstall, RustInstallLocator};
    use ro_net::proto;

    use super::*;

    struct NoInstall;

    impl RustInstallLocator for NoInstall {
        fn locate(&self) -> Option<RustInstall> {
            None
        }
    }

    fn model(dir: &tempfile::TempDir, own: u64) -> (Arc<TeamRoomState>, Arc<OverlayModel>) {
        let room = Arc::new(TeamRoomState::new());
        let settings = Arc::new(OverlaySettingsStore::new(dir.path(), OverlayOptions::default()));
        let catalog = Arc::new(LazyCatalog::new(Arc::new(NoInstall)));
        let model = OverlayModel::new(room.clone(), catalog, Arc::new(ItemIcons::new()), settings, own, Arc::new(|| {}));
        (room, model)
    }

    fn member(steam_id: u64, online: bool) -> proto::Member {
        proto::Member { steam_id, display_name: format!("m{steam_id}"), online, ..Default::default() }
    }

    fn snapshot(room: &TeamRoomState, members: Vec<proto::Member>) {
        room.apply_snapshot(&proto::TeamSnapshot { taken_at_ms: 0, members });
    }

    #[test]
    fn rows_are_online_teammates_by_steam_id_without_the_player() {
        let dir = tempfile::tempdir().unwrap();
        let (room, model) = model(&dir, 5);
        snapshot(&room, vec![member(30, true), member(5, true), member(10, true), member(20, false)]);

        model.reconcile();

        let ids: Vec<u64> = model.state().members.iter().map(|m| m.steam_id).collect();
        assert_eq!(ids, [10, 30]);
        assert!(model.state().members[0].is_selected);
    }

    #[test]
    fn selection_cycles_wraps_and_survives_a_shrinking_roster() {
        let dir = tempfile::tempdir().unwrap();
        let (room, model) = model(&dir, 0);
        snapshot(&room, vec![member(1, true), member(2, true), member(3, true)]);
        model.reconcile();

        model.select(2);
        model.select(7);
        assert!(model.state().members[2].is_selected);

        model.cycle_selection();
        assert!(model.state().members[0].is_selected);

        model.select(2);
        snapshot(&room, vec![member(1, true)]);
        model.reconcile();
        assert!(model.state().members[0].is_selected);
        assert!(model.has_members());
    }

    #[test]
    fn inspect_visibility_reports_only_changes() {
        let dir = tempfile::tempdir().unwrap();
        let (_room, model) = model(&dir, 0);

        assert!(model.set_inspect_visible(true));
        assert!(!model.set_inspect_visible(true));
        assert!(model.set_inspect_visible(false));
    }
}
