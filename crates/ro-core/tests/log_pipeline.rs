mod common;

use std::time::Duration;

use ro_core::LogPipeline;
use ro_core::events::GameEvent;
use ro_core::invariant::read_all_lines;
use ro_core::logs::LogTailer;

fn run_fixture() -> Vec<GameEvent> {
    let lines = read_all_lines(&common::fixture("output_log_sample.txt")).unwrap();
    LogPipeline::default().run_all(lines)
}

#[test]
fn extracts_identity() {
    let identity = run_fixture()
        .into_iter()
        .find_map(|e| match e {
            GameEvent::Identity(i) => Some(i),
            _ => None,
        })
        .unwrap();

    assert_eq!(identity.steam_id, 76561190000000001);
    assert_eq!(identity.name.as_deref(), Some("testplayer"));
}

#[test]
fn extracts_player_state_flags() {
    let states: Vec<_> = run_fixture()
        .into_iter()
        .filter_map(|e| match e {
            GameEvent::PlayerState(s) => Some(s),
            _ => None,
        })
        .collect();

    let [state] = states.as_slice() else { panic!("expected one player state, got {}", states.len()) };
    assert_eq!(state.name, "testplayer");
    assert!(state.wounded());
    assert!(!state.sleeping());
}

#[test]
fn correlates_a_triple_pair_into_one_camera_transform() {
    let events = run_fixture();

    assert_eq!(events.iter().filter(|e| matches!(e, GameEvent::Triple(_))).count(), 2);

    let transform = single_transform(&events);
    assert_eq!(transform.x, -1234.56);
    assert_eq!(transform.y, 12.34);
    assert_eq!(transform.z, 987.65);
    assert_eq!(transform.raw_pitch, 351.98);
    assert_eq!(transform.yaw, 274.51);
}

#[test]
fn positive_raw_pitch_above_180_reads_as_looking_up() {
    let events = run_fixture();

    common::assert_close(8.02, single_transform(&events).pitch_degrees(), 2);
}

#[test]
fn second_dump_contributes_only_its_new_row() {
    let combat: Vec<_> = run_fixture()
        .into_iter()
        .filter_map(|e| match e {
            GameEvent::Combat(c) => Some(c),
            _ => None,
        })
        .collect();

    // Three bear hits from the first dump, replayed in the second, plus one new AK kill.
    assert_eq!(combat.len(), 4);
    assert_eq!(combat.iter().filter(|e| e.attacker == "Big Bad Wolf").count(), 1);
}

#[tokio::test]
async fn tailer_reads_the_whole_file_from_start() {
    let path = common::fixture("output_log_sample.txt");
    let expected = read_all_lines(&path).unwrap().len();
    let mut tailer = LogTailer::new(&path, Some(Duration::from_millis(10)), true);
    let mut lines = Vec::new();

    let read = async {
        while lines.len() < expected {
            lines.extend(tailer.next_lines().await.unwrap());
        }
    };
    tokio::time::timeout(Duration::from_secs(5), read).await.unwrap();

    assert!(lines.iter().any(|l| l.contains("SteamID: 76561190000000001")));
}

fn single_transform(events: &[GameEvent]) -> &ro_core::events::CameraTransform {
    let transforms: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            GameEvent::Transform(t) => Some(t),
            _ => None,
        })
        .collect();

    let [transform] = transforms.as_slice() else { panic!("expected one transform, got {}", transforms.len()) };
    transform
}
