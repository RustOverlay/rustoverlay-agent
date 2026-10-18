//! Who this install is signed in as, and who Rust says is playing: the Team tab's account strip.

use ro_core::events::IdentityEvent;
use ro_net::enrollment::InstallToken;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Player {
    pub name: String,
    pub steam_id: u64,
}

impl Player {
    pub fn from_token(token: &InstallToken) -> Self {
        Self { name: token.display_name.clone(), steam_id: token.steam_id as u64 }
    }

    pub fn from_identity(identity: &IdentityEvent) -> Self {
        Self { name: identity.name.clone().unwrap_or_default(), steam_id: identity.steam_id }
    }

    /// The API has no Steam Web API key in dev, so a persona name can fall back to the SteamID.
    fn shown_name(&self) -> String {
        if self.name.is_empty() { self.steam_id.to_string() } else { self.name.clone() }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct AccountLine {
    pub name: String,
    pub detail: Option<String>,
    /// Rust is logged in to a different Steam account than the one signed in here.
    pub mismatch: bool,
}

pub fn account_line(signed_in: &Player, in_game: Option<&Player>) -> AccountLine {
    let name = signed_in.shown_name();
    let (detail, mismatch) = match in_game {
        Some(game) if game.steam_id != signed_in.steam_id => {
            (Some(format!("Rust is logged in to a different Steam account ({})", game.shown_name())), true)
        }
        Some(game) if !game.name.is_empty() && game.name != signed_in.name => {
            (Some(format!("in game as {}", game.name)), false)
        }
        _ => (None, false),
    };
    AccountLine { name, detail, mismatch }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(name: &str, steam_id: u64) -> Player {
        Player { name: name.into(), steam_id }
    }

    #[test]
    fn the_line_names_the_account_and_only_mentions_the_game_when_it_differs() {
        let me = player("Hexay", 7);

        assert_eq!(account_line(&me, None), AccountLine { name: "Hexay".into(), detail: None, mismatch: false });
        assert_eq!(account_line(&me, Some(&player("Hexay", 7))).detail, None);
        assert_eq!(account_line(&me, Some(&player("Hex", 7))).detail.as_deref(), Some("in game as Hex"));

        let other = account_line(&me, Some(&player("", 9)));
        assert!(other.mismatch);
        assert_eq!(other.detail.as_deref(), Some("Rust is logged in to a different Steam account (9)"));
        assert_eq!(account_line(&player("", 7), None).name, "7");
    }
}
