//! M-05 Game state actor (PRG-05). See 04-data-sources-and-ingestion.md.
//!
//! One authoritative `GameState` per match, built from `ParserEvent`s. This type is deliberately
//! not `Serialize`: the only path from it to the UI in Live Mode is `live_safe` (NFR-FAIR-003, FP-4).

use std::time::{Duration, SystemTime};

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
    /// Net worth in souls (LS-08). `None` until known. Only enemies' values may be projected.
    pub souls: Option<u32>,
    /// Review only; M-06 never copies it.
    pub position: Option<[f32; 3]>,
}

#[derive(Clone, Debug)]
pub struct GameState {
    pub match_id: MatchId,
    pub t_game: GameSecs,
    /// Provisional until the T-008 and T-010 delay measurement: `received_wall - (started_wall + t_game)`,
    /// clamped at 0. It is refreshed on every `Tick`.
    pub delay_s: u32,
    pub players: Vec<PlayerState>,
    pub objectives: Vec<(u8, String, GameSecs)>,
    pub me: AccountId,
    pub self_state: Option<SelfStateData>,
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

    fn player_mut(&mut self, id: AccountId) -> Option<&mut PlayerState> {
        self.players.iter_mut().find(|p| p.account_id == id)
    }

    /// Set a player's assigned lane (LS-09). No parser event carries lanes yet; app wiring calls this.
    pub fn set_lane(&mut self, account_id: AccountId, lane: Option<u8>) {
        if let Some(p) = self.player_mut(account_id) {
            p.lane = lane;
        }
    }

    /// Set a player's souls (LS-08). T-010 wiring: no `ParserEvent` carries per-player souls yet.
    pub fn set_souls(&mut self, account_id: AccountId, souls: Option<u32>) {
        if let Some(p) = self.player_mut(account_id) {
            p.souls = souls;
        }
    }

    pub fn apply(&mut self, ev: &ParserEvent, received_wall: SystemTime) {
        self.log.push(ev.clone());
        match ev {
            ParserEvent::Roster { players } => {
                for r in players {
                    if let Some(p) = self.player_mut(r.account_id) {
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
                let expected = self.started_wall + Duration::from_secs(u64::from(*t_game));
                self.delay_s = received_wall
                    .duration_since(expected)
                    .map(|d| d.as_secs_f64().round() as u32)
                    .unwrap_or(0);
            }
            ParserEvent::Inventory { account_id, items } => {
                if let Some(p) = self.player_mut(*account_id) {
                    p.items = items.clone();
                }
                if *account_id == self.me {
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
        SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000)
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
    fn tc_05_2_delay_tracks_wall_clock_gap() {
        let mut gs = GameState::new(7, 1, t0());
        gs.apply(&ParserEvent::Tick { t_game: 0 }, t0());
        assert_eq!(gs.delay_s, 0);
        gs.apply(
            &ParserEvent::Tick { t_game: 10 },
            t0() + Duration::from_secs(85),
        );
        assert!((73..=77).contains(&gs.delay_s), "{}", gs.delay_s);
        assert_eq!(gs.t_game, 10);
    }

    #[test]
    fn delay_clamps_at_zero() {
        let mut gs = GameState::new(7, 1, t0());
        gs.apply(&ParserEvent::Tick { t_game: 100 }, t0());
        assert_eq!(gs.delay_s, 0);
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
