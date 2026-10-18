mod common;

use chrono::{DateTime, Utc};
use ro_core::events::CombatEvent;
use ro_core::logs::LogBlock;
use ro_core::parsing::{CombatLogDeduplicator, combat_log_parser};
use ro_core::timespan::add_seconds;

fn dumped_at() -> DateTime<Utc> {
    common::utc(2026, 8, 2, 22, 26, 57, 337)
}

fn row(line: &str) -> CombatEvent {
    combat_log_parser::try_parse_row(line, dumped_at()).expect("row parses")
}

#[test]
fn parses_row_with_empty_info_column() {
    let evt = row("344.98s polarbear you    N/A    N/A  generic 3.6m     55.9   10.9           0    0.00      0.00s  0.00m    0");

    assert_eq!(evt.attacker, "polarbear");
    assert_eq!(evt.target, "you");
    assert_eq!(evt.info, None);
    assert_eq!(evt.old_hp, 55.9);
    assert_eq!(evt.new_hp, 10.9);
    assert_eq!(evt.distance_metres, 3.6);
    assert!(!evt.names_ambiguous);
}

#[test]
fn parses_row_with_populated_info_column() {
    let evt = row("338.82s polarbear you    N/A    N/A  generic 1.0m     8.0    2.0    killed  0    0.00      0.00s  0.00m    0");

    assert_eq!(evt.info.as_deref(), Some("killed"));
    assert!(evt.is_kill());
    assert_eq!(evt.damage(), 6.0);
}

#[test]
fn resolves_relative_offset_against_the_dump_header() {
    let evt = row("344.98s polarbear you    N/A    N/A  generic 3.6m     55.9   10.9           0    0.00      0.00s  0.00m    0");

    assert_eq!(Some(evt.timestamp), add_seconds(dumped_at(), -344.98));
    assert_eq!(evt.timestamp, common::utc(2026, 8, 2, 22, 21, 12, 357));
}

#[test]
fn drops_a_row_whose_offset_cannot_be_a_time() {
    for offset in ["1e20s", "Infinitys", "-Infinitys", "NaNs"] {
        let line = format!("{offset} polarbear you N/A N/A generic 3.6m 55.9 10.9 0 0.00 0.00s 0.00m 0");
        assert!(combat_log_parser::try_parse_row(&line, dumped_at()).is_none(), "{line}");
    }
}

#[test]
fn handles_attacker_names_containing_spaces() {
    let evt = row(
        "1210.11s Big Bad Wolf you assets/prefabs/weapons/ak47u/ak47u.entity.prefab riflebullet head 84.2m 100.0 12.5 killed 1 1.00 0.14s 0.02m 47",
    );

    assert_eq!(evt.attacker, "Big Bad Wolf");
    assert_eq!(evt.target, "you");
    assert_eq!(evt.weapon, "assets/prefabs/weapons/ak47u/ak47u.entity.prefab");
    assert_eq!(evt.area, "head");
    assert_eq!(evt.distance_metres, 84.2);
    assert_eq!(evt.desync, 47);
    assert!(evt.names_ambiguous);
}

#[test]
fn rejects_unity_stack_frames() {
    for line in [
        "UnityEngine.DebugLogHandler:Internal_Log(LogType, LogOption, String, Object)",
        "Client:Update()",
        "%81fccaa25850c1aad153f3495fdba74f2308d41a:%c8d978f80bc4e736f7b1a7cf6c6efaf2ca9745be()",
    ] {
        assert!(combat_log_parser::try_parse_row(line, dumped_at()).is_none(), "{line}");
    }
}

#[test]
fn recognises_headers_of_differing_column_widths() {
    assert!(combat_log_parser::is_header(
        "time    attacker  target weapon ammo area    distance old_hp new_hp info    hits integrity travel mismatch desync"
    ));
    assert!(combat_log_parser::is_header(
        "time     attacker  target weapon ammo area    distance old_hp new_hp info    hits integrity travel mismatch desync"
    ));
    assert!(!combat_log_parser::is_header("Entity: player"));
}

#[test]
fn deduplicates_the_same_hit_across_overlapping_dumps() {
    // The ring buffer replays events under a larger offset against a later header, so the
    // resolved timestamps differ by milliseconds rather than matching exactly.
    let first = block(dumped_at(), "344.98s polarbear you N/A N/A generic 3.6m 55.9 10.9 0 0.00 0.00s 0.00m 0");
    let second = block(
        common::utc(2026, 8, 2, 22, 48, 12, 936),
        "1620.59s polarbear you N/A N/A generic 3.6m 55.9 10.9 0 0.00 0.00s 0.00m 0",
    );

    let mut dedupe = CombatLogDeduplicator::default();
    let first_pass = dedupe.filter(combat_log_parser::parse(&first));
    let second_pass = dedupe.filter(combat_log_parser::parse(&second));

    assert_eq!(first_pass.len(), 1);
    assert!(second_pass.is_empty());

    let reparsed = combat_log_parser::parse(&second);
    assert_eq!(reparsed.len(), 1);
    assert_ne!(first_pass[0].timestamp, reparsed[0].timestamp);
}

fn block(at: DateTime<Utc>, row: &str) -> LogBlock {
    LogBlock::new(
        at,
        "0x6e08",
        "time attacker target weapon ammo area distance old_hp new_hp info hits integrity travel mismatch desync",
        vec![row.to_owned()],
    )
}
