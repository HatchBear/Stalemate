//! Turns a snapshot of the Broadcast's entity state into normalized [`ParserEvent`]s.
//!
//! This part has no haste dependency, so it compiles and is tested without the `broadcast` feature.
//! The haste side (`haste_source`) fills a [`GameSnapshot`] once per fragment; this module decides
//! which events that snapshot yields. De-duplication across snapshots (change-only output, no
//! repeats after a reconnect) is the job of [`super::dedup::Dedup`].
//!
//! Alpha extracts only what the Live-Safe whitelist and the Alpha Review need (doc 03, FP-5):
//! roster, items, KDA, level, net worth, own souls, game time, mode and the end of the match.
//! Positions, health, cooldowns and chat are not extracted. They stay in the raw recording.

use crate::types::{AccountId, GameSecs, HeroId, ItemId, ParserEvent, RosterEntry};

/// Normalize a Source 2 team number to the API's Team0 / Team1.
///
/// Players are on teams 2 and 3. Team 1 is the spectators and observers, which are dropped, as is
/// anything else.
// VERIFY: 2 -> 0 and 3 -> 1 is an assumption. Check it against a match's API-05 roster (T-008
// follow-up) before the Review depends on team numbers.
pub fn normalize_team(raw: i64) -> Option<u8> {
    match raw {
        2 => Some(0),
        3 => Some(1),
        _ => None,
    }
}

/// One player controller's state at the end of a fragment.
#[derive(Debug, Clone, PartialEq)]
pub struct PlayerSnapshot {
    pub account_id: AccountId,
    pub name: String,
    /// Already normalized (0 or 1). Spectators are dropped before this struct is built.
    pub team: u8,
    pub hero_id: HeroId,
    /// `m_vecUpgrades` tokens, empty slots removed.
    pub items: Vec<ItemId>,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub level: u8,
    /// `m_iGoldNetWorth`.
    pub net_worth: u32,
    /// `m_iGold`: unspent souls.
    pub gold: u32,
}

/// The game rules entity plus every player controller, at the end of one fragment.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct GameSnapshot {
    /// `m_pGameRules.m_unMatchID`, when the rules entity exists yet.
    pub match_id: Option<u64>,
    /// `m_eGameMode` and `m_eMatchMode`, raw numbers.
    pub modes: Option<(u32, u32)>,
    /// `m_unExpectedPlayerCount`. The roster is complete when this many players are present.
    pub expected_players: u32,
    pub players: Vec<PlayerSnapshot>,
    /// Game time in whole seconds, `None` before the game has started.
    pub game_time: Option<GameSecs>,
    /// Normalized winning team (0 or 1), `None` while the match is running.
    pub winning_team: Option<u8>,
}

impl GameSnapshot {
    /// Every expected player is present with an Account ID and a hero.
    pub fn roster_complete(&self) -> bool {
        self.expected_players > 0
            && self.players.len() == self.expected_players as usize
            && self
                .players
                .iter()
                .all(|p| p.account_id != 0 && p.hero_id != 0)
    }
}

/// The events one snapshot yields. Complete state each time; the caller de-duplicates.
///
/// Order: `Roster` first (consumers need it to interpret the rest), then `Mode`, per-player state
/// sorted by Account ID, the user's `SelfState`, `Tick`, and `End` last.
///
/// `self_account` is the user's Account ID. `SelfState` carries their unspent souls (`m_iGold`).
/// `cooldowns` stays empty and `flex_slots` is 0 in Alpha: cooldowns are Review-only and there is
/// no simple verified source for flex slots.
pub fn events_from_snapshot(
    snap: &GameSnapshot,
    self_account: Option<AccountId>,
) -> Vec<ParserEvent> {
    let mut out = Vec::new();
    let complete = snap.roster_complete();
    if complete {
        let mut players: Vec<&PlayerSnapshot> = snap.players.iter().collect();
        players.sort_by_key(|p| (p.team, p.account_id));
        out.push(ParserEvent::Roster {
            players: players
                .iter()
                .map(|p| RosterEntry {
                    account_id: p.account_id,
                    name: p.name.clone(),
                    team: p.team,
                    hero_id: p.hero_id,
                })
                .collect(),
        });
    }
    if let Some((game_mode, match_mode)) = snap.modes {
        out.push(ParserEvent::Mode {
            game_mode,
            match_mode,
        });
    }
    if complete {
        let mut players: Vec<&PlayerSnapshot> = snap.players.iter().collect();
        players.sort_by_key(|p| p.account_id);
        for p in players {
            out.push(ParserEvent::Inventory {
                account_id: p.account_id,
                items: p.items.clone(),
            });
            out.push(ParserEvent::Kda {
                account_id: p.account_id,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
            });
            out.push(ParserEvent::Level {
                account_id: p.account_id,
                level: p.level,
            });
            out.push(ParserEvent::NetWorth {
                account_id: p.account_id,
                souls: p.net_worth,
            });
        }
        if let Some(me) = self_account.and_then(|a| snap.players.iter().find(|p| p.account_id == a))
        {
            out.push(ParserEvent::SelfState {
                souls: me.gold,
                cooldowns: Vec::new(),
                flex_slots: 0,
            });
        }
    }
    if let Some(t_game) = snap.game_time {
        out.push(ParserEvent::Tick { t_game });
    }
    if let Some(winning_team) = snap.winning_team {
        out.push(ParserEvent::End { winning_team });
    }
    out
}

/// `(net_tick - paused_ticks) / tps - game_start_time`, in whole seconds. `None` before the game
/// has started (no start time yet) or when the result is negative.
pub fn game_time_secs(
    net_tick: i32,
    paused_ticks: i32,
    tps: i32,
    game_start_time: f32,
) -> Option<GameSecs> {
    if tps <= 0 || game_start_time <= 0.0 {
        return None;
    }
    let t = f64::from(net_tick - paused_ticks) / f64::from(tps) - f64::from(game_start_time);
    (t >= 0.0).then_some(t as GameSecs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn player(acc: u32, team: u8) -> PlayerSnapshot {
        PlayerSnapshot {
            account_id: acc,
            name: format!("p{acc}"),
            team,
            hero_id: 7,
            items: vec![11, 22],
            kills: 1,
            deaths: 2,
            assists: 3,
            level: 4,
            net_worth: 5000,
            gold: 120,
        }
    }

    fn snap(n: u32) -> GameSnapshot {
        GameSnapshot {
            match_id: Some(9),
            modes: Some((1, 1)),
            expected_players: n,
            players: (1..=n).map(|i| player(i, (i % 2) as u8)).collect(),
            game_time: Some(61),
            winning_team: None,
        }
    }

    #[test]
    fn teams_two_and_three_become_zero_and_one_and_the_rest_are_dropped() {
        assert_eq!(normalize_team(2), Some(0));
        assert_eq!(normalize_team(3), Some(1));
        for other in [0, 1, 4, -1] {
            assert_eq!(normalize_team(other), None, "{other}");
        }
    }

    #[test]
    fn incomplete_roster_yields_no_roster_and_no_player_events() {
        let mut s = snap(12);
        s.players.pop();
        assert!(!s.roster_complete());
        let ev = events_from_snapshot(&s, None);
        assert!(!ev
            .iter()
            .any(|e| matches!(e, ParserEvent::Roster { .. } | ParserEvent::Kda { .. })));
        assert!(ev.iter().any(|e| matches!(e, ParserEvent::Tick { .. })));
    }

    #[test]
    fn a_player_without_a_hero_keeps_the_roster_incomplete() {
        let mut s = snap(12);
        s.players[3].hero_id = 0;
        assert!(!s.roster_complete());
    }

    #[test]
    fn complete_roster_yields_roster_first_then_per_player_state() {
        let ev = events_from_snapshot(&snap(12), Some(3));
        assert!(matches!(&ev[0], ParserEvent::Roster { players } if players.len() == 12));
        assert!(matches!(ev[1], ParserEvent::Mode { .. }));
        let count = |f: fn(&ParserEvent) -> bool| ev.iter().filter(|e| f(e)).count();
        assert_eq!(count(|e| matches!(e, ParserEvent::Inventory { .. })), 12);
        assert_eq!(count(|e| matches!(e, ParserEvent::Kda { .. })), 12);
        assert_eq!(count(|e| matches!(e, ParserEvent::Level { .. })), 12);
        assert_eq!(count(|e| matches!(e, ParserEvent::NetWorth { .. })), 12);
        assert!(ev.contains(&ParserEvent::SelfState {
            souls: 120,
            cooldowns: vec![],
            flex_slots: 0
        }));
        assert_eq!(ev.last(), Some(&ParserEvent::Tick { t_game: 61 }));
    }

    #[test]
    fn roster_is_sorted_by_team_then_account() {
        let ev = events_from_snapshot(&snap(12), None);
        let ParserEvent::Roster { players } = &ev[0] else {
            panic!()
        };
        let keys: Vec<_> = players.iter().map(|p| (p.team, p.account_id)).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted);
    }

    #[test]
    fn no_self_state_when_the_user_is_not_in_the_roster() {
        let ev = events_from_snapshot(&snap(12), Some(999));
        assert!(!ev
            .iter()
            .any(|e| matches!(e, ParserEvent::SelfState { .. })));
    }

    #[test]
    fn end_comes_last_and_carries_the_winner() {
        let mut s = snap(12);
        s.winning_team = Some(1);
        let ev = events_from_snapshot(&s, None);
        assert_eq!(ev.last(), Some(&ParserEvent::End { winning_team: 1 }));
    }

    #[test]
    fn game_time_follows_the_documented_formula() {
        // (119090 - 4546) / 64 - 45.421875 = 1744.0 (to the whole second)
        assert_eq!(game_time_secs(119_090, 4546, 64, 45.421_875), Some(1744));
        assert_eq!(game_time_secs(100, 0, 64, 0.0), None, "not started");
        assert_eq!(game_time_secs(100, 0, 0, 5.0), None, "tps from the sync");
        assert_eq!(game_time_secs(64, 0, 64, 45.0), None, "negative");
    }
}
