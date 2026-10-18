//! Parses `combatlog` dumps. See docs/log-format.md for the column layout and the three traps: the
//! optional `info` column, column widths that shift between dumps, and player names that may
//! contain spaces.

use chrono::{DateTime, Utc};

use crate::events::CombatEvent;
use crate::invariant::{parse_double, parse_i32};
use crate::logs::LogBlock;
use crate::timespan::add_seconds;

const HEADER_START: &str = "time";
const TRAILING_FIELD_COUNT: usize = 5; // hits integrity travel mismatch desync

pub fn is_header(message: &str) -> bool {
    message.starts_with(HEADER_START) && message.contains("attacker") && message.contains("desync")
}

/// Extracts every row from a dump block. Rows carry a time *relative* to the block header, so
/// absolute time is `block.timestamp - offset`.
pub fn parse(block: &LogBlock) -> Vec<CombatEvent> {
    if !is_header(&block.message) {
        return Vec::new();
    }

    block.body.iter().filter_map(|line| try_parse_row(line, block.timestamp)).collect()
}

pub fn try_parse_row(line: &str, dumped_at: DateTime<Utc>) -> Option<CombatEvent> {
    let t: Vec<&str> = line.split_whitespace().collect();

    // Shortest legal row is time + attacker + target + 11 fields.
    if t.len() < 14 {
        return None;
    }
    let offset_seconds = parse_suffixed(t[0], 's')?;

    let n = t.len();

    // The info column is blank on most rows, so token count alone is ambiguous once a player
    // name contains a space. Disambiguate on content instead: the slot before the five trailing
    // fields is new_hp when info is absent, and new_hp is always numeric.
    let has_info = parse_double(t[n - TRAILING_FIELD_COUNT - 1]).is_none();
    let info_offset = usize::from(has_info);

    let new_hp_idx = n - TRAILING_FIELD_COUNT - 1 - info_offset;
    let old_hp_idx = new_hp_idx - 1;
    let distance_idx = old_hp_idx - 1;
    let area_idx = distance_idx - 1;
    let ammo_idx = area_idx - 1;
    let weapon_idx = ammo_idx - 1;
    if weapon_idx <= 1 {
        return None;
    }

    let new_hp = parse_double(t[new_hp_idx])?;
    let old_hp = parse_double(t[old_hp_idx])?;
    let distance = parse_suffixed(t[distance_idx], 'm')?;
    let integrity = parse_double(t[n - 4])?;
    let travel = parse_suffixed(t[n - 3], 's')?;
    let mismatch = parse_suffixed(t[n - 2], 'm')?;
    let hits = parse_i32(t[n - 5])?;
    let desync = parse_i32(t[n - 1])?;

    let (attacker, target, ambiguous) = split_names(&t, 1, weapon_idx);

    Some(CombatEvent {
        timestamp: add_seconds(dumped_at, -offset_seconds)?,
        attacker,
        target,
        weapon: t[weapon_idx].to_owned(),
        ammo: t[ammo_idx].to_owned(),
        area: t[area_idx].to_owned(),
        distance_metres: distance,
        old_hp,
        new_hp,
        info: has_info.then(|| t[n - TRAILING_FIELD_COUNT - 1].to_owned()),
        hits,
        integrity,
        travel_seconds: travel,
        mismatch_metres: mismatch,
        desync,
        names_ambiguous: ambiguous,
    })
}

/// Names occupy everything between the time column and the weapon column. Two tokens is the
/// normal case; more means at least one name contains spaces, which the format does not escape.
/// "you" is the reliable anchor because one side of a local combatlog row is always the local
/// player.
fn split_names(t: &[&str], start: usize, end: usize) -> (String, String, bool) {
    if end - start == 2 {
        return (t[start].to_owned(), t[start + 1].to_owned(), false);
    }

    if let Some(i) = (start..end).find(|&i| t[i] == "you") {
        return if i == start {
            (t[start].to_owned(), t[start + 1..end].join(" "), true)
        } else {
            (t[start..i].join(" "), t[i..end].join(" "), true)
        };
    }

    (t[start..end - 1].join(" "), t[end - 1].to_owned(), true)
}

fn parse_suffixed(token: &str, suffix: char) -> Option<f64> {
    let number = token.strip_suffix(suffix)?;
    if number.is_empty() {
        return None;
    }
    parse_double(number)
}
