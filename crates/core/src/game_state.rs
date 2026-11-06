//! M-05 Game state actor (PRG-05). See 04-data-sources-and-ingestion.md.
//!
//! One authoritative `GameState` per match, built from `ParserEvent`s. This type is deliberately
//! not `Serialize`: the only path from it to the UI in Live Mode is `live_safe` (NFR-FAIR-003, FP-4).

use std::time::SystemTime;

use crate::types::{AccountId, GameSecs, HeroId, ItemId, MatchId, ParserEvent};

/// The user's own state (LS-12). `flex_slots` is shown only when the whitelist enables LS-13.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelfStateData {
    pub souls: u32,
    pub items: Vec<ItemId>,
    pub cooldowns: Vec<(u32, f32)>,
    pub flex_slots: u8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct PlayerState {
    pub account_id: AccountId,
    pub name: String,
    pub team: u8,
    pub hero_id: HeroId,
    pub items: Vec<ItemId>,
    pub kda: (u16, u16, u16),
    pub level: u8,
    pub lane: Option<u8>,
    /// Net worth in souls (LS-08), from `ParserEvent::NetWorth`. `None` until known. Whether it
    /// is shown is `live_safe`'s call.
    pub souls: Option<u32>,
    /// Review only; M-06 never copies it.
    pub position: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub match_id: MatchId,
    pub t_game: GameSecs,
    /// Broadcast delay in seconds, set by [`GameState::set_delay`] (FR-UI-008, DEC-050).
    pub delay_s: u32,
    pub players: Vec<PlayerState>,
    pub objectives: Vec<(u8, String, GameSecs)>,
    pub me: AccountId,
    pub self_state: Option<SelfStateData>,
    /// Kept so `new` keeps its signature. Delay now comes from the Broadcast client (DEC-050).
    #[allow(dead_code)]
    started_wall: SystemTime,
    /// Every event received, for Review. Includes chat, which is never displayed (DEC-025).
    log: Vec<ParserEvent>,
}

impl GameState {
    pub fn new(match_id: MatchId, me: AccountId, started_wall: SystemTime) -> Self {
        Self {
            match_id,
            t_game: 0,
            delay_s: 0,
            players: Vec::new(),
            objectives: Vec::new(),
            me,
            self_state: None,
            started_wall,
            log: Vec::new(),
        }
    }

    /// The full event log, for Review Mode only. It contains chat, which no code may display (DEC-025).
    pub fn review_log(&self) -> &[ParserEvent] {
        &self.log
    }

    /// Account 0 is a bot and several share it, so per-player events never match it.
    fn player_mut(&mut self, id: AccountId) -> Option<&mut PlayerState> {
        if id == 0 {
            return None;
        }
        self.players.iter_mut().find(|p| p.account_id == id)
    }

    /// Set a player's assigned lane (LS-09). No parser event carries lanes yet; app wiring calls this.
    pub fn set_lane(&mut self, account_id: AccountId, lane: Option<u8>) {
        if let Some(p) = self.player_mut(account_id) {
            p.lane = lane;
        }
    }

    /// Set a player's net worth (LS-08). `ParserEvent::NetWorth` calls this.
    pub fn set_souls(&mut self, account_id: AccountId, souls: Option<u32>) {
        if let Some(p) = self.player_mut(account_id) {
            p.souls = souls;
        }
    }

    /// Set the broadcast delay from a `BroadcastKind::Delay` message (FR-UI-008, DEC-050).
    /// Rounded to whole seconds and clamped at 0; NaN gives 0.
    pub fn set_delay(&mut self, secs: f32) {
        self.delay_s = if secs.is_nan() || secs <= 0.0 {
            0
        } else {
            secs.round() as u32 // `as` saturates for huge values
        };
    }

    pub fn apply(&mut self, ev: &ParserEvent, _received_wall: SystemTime) {
        self.log.push(ev.clone());
        match ev {
            ParserEvent::Roster { players } => {
                for r in players {
                    // Bots all have account 0, so they match on account, team and hero.
                    let existing = self.players.iter_mut().find(|p| {
                        p.account_id == r.account_id
                            && (r.account_id != 0 || (p.team == r.team && p.hero_id == r.hero_id))
                    });
                    if let Some(p) = existing {
                        p.name = r.name.clone();
                        p.team = r.team;
                        p.hero_id = r.hero_id;
                    } else {
                        self.players.push(PlayerState {
                            account_id: r.account_id,
                            name: r.name.clone(),
                            team: r.team,
                            hero_id: r.hero_id,
                            items: Vec::new(),
                            kda: (0, 0, 0),
                            level: 0,
                            lane: None,
                            souls: None,
                            position: None,
                        });
                    }
                }
            }
            ParserEvent::Tick { t_game } => {
                self.t_game = *t_game;
            }
            ParserEvent::Inventory { account_id, items } => {
                if let Some(p) = self.player_mut(*account_id) {
                    p.items = items.clone();
                }
                if *account_id != 0 && *account_id == self.me {
                    self.self_state
                        .get_or_insert_with(SelfStateData::default)
                        .items = items.clone();
                }
            }
            ParserEvent::Kda {
                account_id,
                kills,
                deaths,
                assists,
            } => {
                if let Some(p) = self.player_mut(*account_id) {
                    p.kda = (*kills, *deaths, *assists);
                }
            }
            ParserEvent::Level { account_id, level } => {
                if let Some(p) = self.player_mut(*account_id) {
                    p.level = *level;
                }
            }
            ParserEvent::Objective { team, kind, t_game } => {
                self.objectives.push((*team, kind.clone(), *t_game));
            }
            ParserEvent::SelfState {
                souls,
                cooldowns,
                flex_slots,
            } => {
                let s = self.self_state.get_or_insert_with(SelfStateData::default);
                s.souls = *souls;
                s.cooldowns = cooldowns.clone();
                s.flex_slots = *flex_slots;
            }
            ParserEvent::Position {
                account_id,
                x,
                y,
                z,
            } => {
                if let Some(p) = self.player_mut(*account_id) {
                    p.position = Some([*x, *y, *z]);
                }
            }
            // Kept in the log for Review only. Nothing derived from these reaches Live Mode (FR-LIVE-015).
            ParserEvent::Damage { .. } | ParserEvent::Death { .. } | ParserEvent::Chat { .. } => {}
            ParserEvent::End { .. } => {}
            // T-010: added with the new variants so the match stays exhaustive. Whether souls are
            // shown is still `live_safe`'s call.
            ParserEvent::NetWorth { account_id, souls } => {
                self.set_souls(*account_id, Some(*souls));
            }
            ParserEvent::Mode { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::RosterEntry;

    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000)
    }

    fn roster12() -> ParserEvent {
        ParserEvent::Roster {
            players: (1..=12)
                .map(|i| RosterEntry {
                    account_id: i,
                    name: format!("p{i}"),
                    team: (i % 2) as u8,
                    hero_id: i + 100,
                })
                .collect(),
        }
    }

    #[test]
    fn tc_05_1_roster_then_12_inventory_events() {
        let mut gs = GameState::new(7, 1, t0());
        gs.apply(&roster12(), t0());
        for i in 1..=12u32 {
            gs.apply(
                &ParserEvent::Inventory {
                    account_id: i,
                    items: vec![i, i + 50],
                },
                t0(),
            );
        }
        assert_eq!(gs.players.len(), 12);
        assert!(gs.players.iter().all(|p| p.items.len() == 2));
        assert_eq!(gs.players[3].items, vec![4, 54]);
        assert_eq!(gs.self_state.as_ref().unwrap().items, vec![1, 51]);
        assert_eq!(gs.review_log().len(), 13);
    }

    #[test]
    fn tc_05_2_set_delay_rounds_and_clamps() {
        let mut gs = GameState::new(7, 1, t0());
        assert_eq!(gs.delay_s, 0);
        gs.set_delay(75.4);
        assert_eq!(gs.delay_s, 75);
        gs.set_delay(75.6);
        assert_eq!(gs.delay_s, 76);
        gs.set_delay(-3.0);
        assert_eq!(gs.delay_s, 0);
        gs.set_delay(40.0);
        gs.set_delay(f32::NAN);
        assert_eq!(gs.delay_s, 0);
    }

    #[test]
    fn tick_sets_game_time_only() {
        let mut gs = GameState::new(7, 1, t0());
        gs.set_delay(60.0);
        gs.apply(
            &ParserEvent::Tick { t_game: 10 },
            t0() + std::time::Duration::from_secs(500),
        );
        assert_eq!(gs.t_game, 10);
        assert_eq!(gs.delay_s, 60);
    }

    #[test]
    fn net_worth_event_sets_souls() {
        let mut gs = GameState::new(7, 1, t0());
        gs.apply(&roster12(), t0());
        assert_eq!(gs.players[2].souls, None);
        gs.apply(
            &ParserEvent::NetWorth {
                account_id: 3,
                souls: 4200,
            },
            t0(),
        );
        assert_eq!(gs.players[2].souls, Some(4200));
        assert_eq!(gs.players[3].souls, None);
    }

    fn bot(hero: u32, team: u8) -> RosterEntry {
        RosterEntry {
            account_id: 0,
            name: String::new(),
            team,
            hero_id: hero,
        }
    }

    #[test]
    fn roster_keeps_two_bots_and_is_idempotent() {
        let mut gs = GameState::new(7, 1, t0());
        let ev = ParserEvent::Roster {
            players: vec![
                RosterEntry {
                    account_id: 1,
                    name: "me".into(),
                    team: 0,
                    hero_id: 5,
                },
                bot(10, 1),
                bot(11, 1),
            ],
        };
        gs.apply(&ev, t0());
        assert_eq!(gs.players.len(), 3);
        gs.apply(&ev, t0());
        assert_eq!(gs.players.len(), 3);
        // Per-player events for account 0 touch nobody.
        gs.apply(
            &ParserEvent::Inventory {
                account_id: 0,
                items: vec![1],
            },
            t0(),
        );
        gs.apply(
            &ParserEvent::Kda {
                account_id: 0,
                kills: 1,
                deaths: 1,
                assists: 1,
            },
            t0(),
        );
        gs.apply(
            &ParserEvent::Level {
                account_id: 0,
                level: 9,
            },
            t0(),
        );
        gs.apply(
            &ParserEvent::NetWorth {
                account_id: 0,
                souls: 9,
            },
            t0(),
        );
        assert!(gs.players[1..]
            .iter()
            .all(|p| p.items.is_empty() && p.level == 0 && p.souls.is_none()));
    }

    #[test]
    fn self_state_and_objectives_are_recorded() {
        let mut gs = GameState::new(7, 1, t0());
        gs.apply(
            &ParserEvent::SelfState {
                souls: 900,
                cooldowns: vec![(1, 2.5)],
                flex_slots: 1,
            },
            t0(),
        );
        gs.apply(
            &ParserEvent::Objective {
                team: 1,
                kind: "guardian".into(),
                t_game: 300,
            },
            t0(),
        );
        assert_eq!(gs.self_state.as_ref().unwrap().souls, 900);
        assert_eq!(gs.objectives.len(), 1);
    }
}
