//! Parses the identity line and the `Entity: player` state dump.

use std::collections::HashMap;

use crate::events::{IdentityEvent, PlayerStateEvent};
use crate::invariant::{parse_bool, parse_u64};
use crate::logs::LogBlock;

const STEAM_ID_PREFIX: &str = "SteamID: ";
const ENTITY_PLAYER: &str = "Entity: player";

/// Matches `SteamID: 76561190000000001 (testplayer)`, name optional.
pub fn try_parse_identity(block: &LogBlock) -> Option<IdentityEvent> {
    let rest = block.message.strip_prefix(STEAM_ID_PREFIX)?.trim();
    let space = rest.find(' ');
    let id_text = space.map_or(rest, |s| &rest[..s]);
    let steam_id = parse_u64(id_text)?;

    let name = space
        .filter(|&s| s > 0)
        .map(|s| rest[s + 1..].trim().trim_matches(['(', ')']))
        .filter(|name| !name.is_empty())
        .map(str::to_owned);

    Some(IdentityEvent { timestamp: block.timestamp, steam_id, name })
}

pub fn try_parse_player_state(block: &LogBlock) -> Option<PlayerStateEvent> {
    if !block.message.starts_with(ENTITY_PLAYER) {
        return None;
    }

    let mut name = None;
    let mut steam_id = 0;
    let mut flags = HashMap::new();

    for line in &block.body {
        let Some(separator) = line.find(':').filter(|&s| s > 0) else { continue };

        let key = line[..separator].trim();
        let value = line[separator + 1..].trim();

        match key {
            "Name" => name = Some(value.to_owned()),
            // A failed parse zeroes the id, as `ulong.TryParse`'s out parameter did.
            "SteamID" => steam_id = parse_u64(value).unwrap_or(0),
            _ => {
                if let Some(flag) = parse_bool(value) {
                    flags.insert(key.to_owned(), flag);
                }
            }
        }
    }

    let name = name.filter(|_| steam_id != 0)?;

    Some(PlayerStateEvent { timestamp: block.timestamp, steam_id, name, flags })
}
