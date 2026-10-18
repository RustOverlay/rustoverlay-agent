//! The multi-frame filter over the belt. Readings are handed in directly rather than matched out of
//! a painted frame — what is under test is which of them survive, not how they were produced.

mod common;

use std::cell::Cell;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use common::item;
use ro_vision::hud::{FrameBuffer, InventoryGeometry, InventoryReading, Rgb, SlotCount, SlotHistory, SlotReading};
use ro_vision::items::IconMatch;

/// A frame whose every pixel moves with `backdrop`, standing in for terrain.
#[derive(Default)]
struct Scene {
    backdrop: Cell<u8>,
}

impl FrameBuffer for Scene {
    fn width(&self) -> i32 {
        1920
    }

    fn height(&self) -> i32 {
        1080
    }

    fn pixel(&self, x: i32, y: i32) -> Rgb {
        let b = self.backdrop.get();
        Rgb::new(b.wrapping_add((x % 7) as u8), b.wrapping_add((y % 5) as u8), b)
    }
}

/// Moves the scene on, so the next reading counts as independent evidence.
fn move_on(scene: &Scene) {
    scene.backdrop.set(scene.backdrop.get().wrapping_add(40));
}

static GEOMETRY: LazyLock<InventoryGeometry> = LazyLock::new(|| InventoryGeometry::new(1920, 1080, 1.0));

/// 2026-01-01T00:00:00Z.
fn start() -> SystemTime {
    UNIX_EPOCH + Duration::from_secs(1_767_225_600)
}

fn matched(short_name: &str, alike: &[&str]) -> IconMatch {
    IconMatch { item: item(short_name), score: 0.9, indistinguishable: alike.iter().map(|a| item(a)).collect() }
}

fn reading(seen: Option<IconMatch>, count: Option<SlotCount>) -> InventoryReading {
    InventoryReading { belt: vec![SlotReading { slot: GEOMETRY.belt()[0], item: seen, count }], ..InventoryReading::default() }
}

/// Pushes one frame and returns what the first belt slot would be reported as.
fn push(history: &mut SlotHistory, scene: &Scene, seen: Option<IconMatch>, second: u64, open: bool) -> Option<IconMatch> {
    let read_at = start() + Duration::from_secs(second);
    history.confirm(scene, &reading(seen, None), open, read_at).belt[0].item.clone()
}

fn named(m: Option<IconMatch>) -> Option<String> {
    m.map(|m| m.item.short_name.clone())
}

fn rifle() -> Option<IconMatch> {
    Some(matched("rifle.ak", &[]))
}

#[test]
fn one_reading_is_not_enough_to_report_an_item() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    assert!(push(&mut history, &scene, rifle(), 0, false).is_none());
}

#[test]
fn two_agreeing_readings_over_different_backdrops_confirm_it() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, rifle(), 0, false);
    move_on(&scene);

    assert_eq!(named(push(&mut history, &scene, rifle(), 1, false)).as_deref(), Some("rifle.ak"));
}

#[test]
fn the_same_pixels_twice_are_one_observation_not_two() {
    // InventoryReader serves an unchanged slot from cache, so a stationary player would otherwise
    // confirm whatever the terrain invented on the first frame.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());
    let web = || Some(matched("spiderweb", &[]));

    push(&mut history, &scene, web(), 0, false);

    assert!(push(&mut history, &scene, web(), 1, false).is_none());
    assert!(push(&mut history, &scene, web(), 2, false).is_none());
}

#[test]
fn a_confirmed_item_is_held_while_the_backdrop_stays_put() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, rifle(), 0, false);
    move_on(&scene);
    push(&mut history, &scene, rifle(), 1, false);

    assert_eq!(named(push(&mut history, &scene, rifle(), 2, false)).as_deref(), Some("rifle.ak"));
}

#[test]
fn inventions_that_never_repeat_are_never_reported() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    for (i, name) in ["spiderweb", "cannonball", "horsedung", "water", "skull.human"].into_iter().enumerate() {
        assert!(push(&mut history, &scene, Some(matched(name, &[])), i as u64, false).is_none());
        move_on(&scene);
    }
}

#[test]
fn one_contradicting_reading_does_not_withdraw_a_confirmed_item() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, rifle(), 0, false);
    move_on(&scene);
    push(&mut history, &scene, rifle(), 1, false);
    move_on(&scene);

    assert_eq!(named(push(&mut history, &scene, None, 2, false)).as_deref(), Some("rifle.ak"));
}

#[test]
fn two_contradicting_readings_withdraw_it() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, rifle(), 0, false);
    move_on(&scene);
    push(&mut history, &scene, rifle(), 1, false);
    move_on(&scene);
    push(&mut history, &scene, None, 2, false);
    move_on(&scene);

    assert!(push(&mut history, &scene, None, 3, false).is_none());
}

#[test]
fn look_alikes_confirm_on_what_the_readings_share() {
    // Ammo variants cannot be split by the icon, so requiring the same leader twice would abstain
    // on them forever. The reported set is the intersection, narrower than either.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, Some(matched("ammo.rifle", &["ammo.rifle.hv", "ammo.rifle.he"])), 0, false);
    move_on(&scene);

    let held = push(&mut history, &scene, Some(matched("ammo.rifle.hv", &["ammo.rifle"])), 1, false).expect("confirmed");
    let mut names: Vec<&str> = std::iter::once(&held.item).chain(&held.indistinguishable).map(|i| i.short_name.as_str()).collect();
    names.sort_unstable();

    assert_eq!(names, ["ammo.rifle", "ammo.rifle.hv"]);
}

#[test]
fn readings_either_side_of_a_long_gap_are_not_adjacent() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push(&mut history, &scene, rifle(), 0, false);
    move_on(&scene);

    assert!(push(&mut history, &scene, rifle(), 30, false).is_none());
}

/// Confirms a rifle in the first slot over two independent frames, ending at second 1.
fn confirmed_rifle(history: &mut SlotHistory, scene: &Scene) {
    push(history, scene, rifle(), 0, false);
    move_on(scene);
    assert!(push(history, scene, rifle(), 1, false).is_some());
    move_on(scene);
}

#[test]
fn a_belt_still_held_after_a_gap_is_back_on_the_first_frame() {
    // Alt-tab is a gap; forgetting outright made every slot go blank for two frames on return.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());
    confirmed_rifle(&mut history, &scene);

    assert_eq!(named(push(&mut history, &scene, rifle(), 30, false)).as_deref(), Some("rifle.ak"));
}

#[test]
fn nothing_confirmed_is_reported_after_a_gap_until_a_frame_agrees() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());
    confirmed_rifle(&mut history, &scene);

    assert!(push(&mut history, &scene, None, 30, false).is_none());
}

#[test]
fn a_slot_that_changed_during_a_gap_still_needs_the_usual_agreements() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());
    confirmed_rifle(&mut history, &scene);
    let knife = || Some(matched("knife.bone", &[]));

    assert!(push(&mut history, &scene, knife(), 30, false).is_none());
    move_on(&scene);
    assert_eq!(named(push(&mut history, &scene, knife(), 31, false)).as_deref(), Some("knife.bone"));
}

#[test]
fn a_panel_backed_reading_is_trusted_at_once() {
    // Shutting the inventory must not cost two silent frames re-proving what was just read.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    assert_eq!(named(push(&mut history, &scene, rifle(), 0, true)).as_deref(), Some("rifle.ak"));

    move_on(&scene);

    assert_eq!(named(push(&mut history, &scene, rifle(), 1, false)).as_deref(), Some("rifle.ak"));
}

/// Pushes one frame and returns what the first belt slot's count would be reported as.
fn push_count(history: &mut SlotHistory, scene: &Scene, seen: Option<SlotCount>, second: u64, open: bool) -> Option<SlotCount> {
    let read_at = start() + Duration::from_secs(second);
    history.confirm(scene, &reading(Some(matched("cloth", &[])), seen), open, read_at).belt[0].count
}

fn stack(value: i32) -> Option<SlotCount> {
    Some(SlotCount::new(value, true))
}

#[test]
fn one_reading_is_not_enough_to_report_a_count() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    assert_eq!(push_count(&mut history, &scene, stack(30), 0, false), None);
}

#[test]
fn two_agreeing_readings_confirm_a_count() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push_count(&mut history, &scene, stack(30), 0, false);
    move_on(&scene);

    assert_eq!(push_count(&mut history, &scene, stack(30), 1, false), stack(30));
}

#[test]
fn a_single_frame_misread_never_reaches_a_teammate() {
    // The live failure: a confirmed x30 read as x9 for one frame, the digit walk having stopped early.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push_count(&mut history, &scene, stack(30), 0, false);
    move_on(&scene);
    push_count(&mut history, &scene, stack(30), 1, false);
    move_on(&scene);

    assert_eq!(push_count(&mut history, &scene, stack(9), 2, false), stack(30));
    assert_eq!(history.last_count_rejections().len(), 1);
}

#[test]
fn a_count_that_really_changed_is_adopted() {
    // Gathering moves a stack every few seconds, so the filter must not pin it to the old value.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push_count(&mut history, &scene, stack(50), 0, false);
    move_on(&scene);
    push_count(&mut history, &scene, stack(50), 1, false);
    move_on(&scene);
    push_count(&mut history, &scene, stack(100), 2, false);
    move_on(&scene);

    assert_eq!(push_count(&mut history, &scene, stack(100), 3, false), stack(100));
}

#[test]
fn an_unreadable_count_is_reported_as_unknown_rather_than_stale() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push_count(&mut history, &scene, stack(30), 0, false);
    move_on(&scene);
    push_count(&mut history, &scene, stack(30), 1, false);
    move_on(&scene);
    push_count(&mut history, &scene, None, 2, false);
    move_on(&scene);

    assert_eq!(push_count(&mut history, &scene, None, 3, false), None);
}

#[test]
fn a_panel_backed_count_is_trusted_at_once() {
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    assert_eq!(push_count(&mut history, &scene, stack(30), 0, true), stack(30));
}

#[test]
fn a_magazine_count_is_not_the_same_reading_as_a_stack() {
    // x24 and 24 mean different things — a stack of rounds versus a loaded magazine.
    let (mut history, scene) = (SlotHistory::new(), Scene::default());

    push_count(&mut history, &scene, stack(24), 0, false);
    move_on(&scene);

    assert_eq!(push_count(&mut history, &scene, Some(SlotCount::new(24, false)), 1, false), None);
}
