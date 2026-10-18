//! What a freshly listed set of teams does to the team this install broadcasts to.

use ro_net::relay::ActiveTeam;
use ro_net::teams::TeamDto;

#[derive(Debug, PartialEq)]
pub enum Refreshed<'a> {
    Keep,
    /// The active team was deleted or left elsewhere, and there is no single team to fall back to.
    Clear,
    Adopt(&'a TeamDto),
}

/// Membership without a broadcast is never what joining meant, so the only team you are in is
/// connected to on its own. Several teams are left to the player, and an explicit Disconnect opts
/// out for the session.
pub fn on_refresh<'a>(active: Option<&ActiveTeam>, mine: &'a [TeamDto], disconnected: bool) -> Refreshed<'a> {
    let orphaned = active.is_some_and(|a| mine.iter().all(|t| t.id.to_string() != a.team_id));
    if active.is_some() && !orphaned {
        return Refreshed::Keep;
    }

    match mine {
        [only] if !disconnected => Refreshed::Adopt(only),
        _ if orphaned => Refreshed::Clear,
        _ => Refreshed::Keep,
    }
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;

    use super::*;

    fn team(n: u128) -> TeamDto {
        TeamDto { id: Uuid::from_u128(n), name: format!("t{n}"), ..TeamDto::default() }
    }

    fn active(team: &TeamDto) -> ActiveTeam {
        ActiveTeam { team_id: team.id.to_string(), join_code: team.join_code.clone(), name: team.name.clone() }
    }

    #[test]
    fn the_only_team_is_connected_to_when_nothing_is_active() {
        let mine = [team(1)];
        assert_eq!(on_refresh(None, &mine, false), Refreshed::Adopt(&mine[0]));
    }

    #[test]
    fn an_explicit_disconnect_is_not_undone() {
        assert_eq!(on_refresh(None, &[team(1)], true), Refreshed::Keep);
    }

    #[test]
    fn several_teams_are_left_to_the_player() {
        assert_eq!(on_refresh(None, &[team(1), team(2)], false), Refreshed::Keep);
    }

    #[test]
    fn a_live_active_team_is_kept_even_with_others_listed() {
        let mine = [team(1), team(2)];
        assert_eq!(on_refresh(Some(&active(&mine[1])), &mine, false), Refreshed::Keep);
    }

    #[test]
    fn an_orphaned_team_falls_back_to_the_only_remaining_one() {
        let mine = [team(2)];
        assert_eq!(on_refresh(Some(&active(&team(1))), &mine, false), Refreshed::Adopt(&mine[0]));
    }

    #[test]
    fn an_orphaned_team_is_cleared_when_there_is_no_single_fallback() {
        assert_eq!(on_refresh(Some(&active(&team(1))), &[], false), Refreshed::Clear);
        assert_eq!(on_refresh(Some(&active(&team(1))), &[team(2), team(3)], false), Refreshed::Clear);
        assert_eq!(on_refresh(Some(&active(&team(1))), &[team(2)], true), Refreshed::Clear);
    }
}
