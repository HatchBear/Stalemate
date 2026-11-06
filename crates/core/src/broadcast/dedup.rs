//! Change-only output (FR-DATA-003, TC-04-2).
//!
//! A new connection re-reads the whole entity state, so the parser re-sends the roster and every
//! player's values. [`Dedup`] remembers what was already sent and lets through only what changed, so
//! consumers see no duplicates after a reconnect and no per-fragment spam.

use std::collections::HashMap;

use crate::types::{AccountId, GameSecs, ItemId, ParserEvent, RosterEntry};

#[derive(Debug, Default)]
pub struct Dedup {
    roster: Option<Vec<RosterEntry>>,
    modes: Option<(u32, u32)>,
    inventory: HashMap<AccountId, Vec<ItemId>>,
    kda: HashMap<AccountId, (u16, u16, u16)>,
    level: HashMap<AccountId, u8>,
    net_worth: HashMap<AccountId, u32>,
    souls: Option<u32>,
    last_tick: Option<GameSecs>,
    ended: bool,
}

fn changed<K: std::hash::Hash + Eq, V: PartialEq>(m: &mut HashMap<K, V>, k: K, v: V) -> bool {
    if m.get(&k) == Some(&v) {
        false
    } else {
        m.insert(k, v);
        true
    }
}

impl Dedup {
    /// True when the event is new information and should be sent.
    ///
    /// - `Tick` passes only when game time moved forward, so at most one per game second.
    /// - `End` passes once.
    /// - Events the Alpha parser never produces (positions, damage, chat, ...) pass unchanged.
    pub fn admit(&mut self, ev: &ParserEvent) -> bool {
        match ev {
            ParserEvent::Roster { players } => {
                if self.roster.as_ref() == Some(players) {
                    false
                } else {
                    self.roster = Some(players.clone());
                    true
                }
            }
            ParserEvent::Mode {
                game_mode,
                match_mode,
            } => {
                let v = (*game_mode, *match_mode);
                if self.modes == Some(v) {
                    false
                } else {
                    self.modes = Some(v);
                    true
                }
            }
            ParserEvent::Inventory { account_id, items } => {
                changed(&mut self.inventory, *account_id, items.clone())
            }
            ParserEvent::Kda {
                account_id,
                kills,
                deaths,
                assists,
            } => changed(&mut self.kda, *account_id, (*kills, *deaths, *assists)),
            ParserEvent::Level { account_id, level } => {
                changed(&mut self.level, *account_id, *level)
            }
            ParserEvent::NetWorth { account_id, souls } => {
                changed(&mut self.net_worth, *account_id, *souls)
            }
            ParserEvent::SelfState { souls, .. } => {
                if self.souls == Some(*souls) {
                    false
                } else {
                    self.souls = Some(*souls);
                    true
                }
            }
            ParserEvent::Tick { t_game } => {
                if self.last_tick.is_some_and(|t| *t_game <= t) {
                    false
                } else {
                    self.last_tick = Some(*t_game);
                    true
                }
            }
            ParserEvent::End { .. } => {
                let first = !self.ended;
                self.ended = true;
                first
            }
            _ => true,
        }
    }

    /// The last game second sent, for the caller's own bookkeeping.
    pub fn last_tick(&self) -> Option<GameSecs> {
        self.last_tick
    }

    pub fn has_ended(&self) -> bool {
        self.ended
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kda(a: u32, k: u16) -> ParserEvent {
        ParserEvent::Kda {
            account_id: a,
            kills: k,
            deaths: 0,
            assists: 0,
        }
    }

    #[test]
    fn unchanged_values_are_dropped_and_changes_pass() {
        let mut d = Dedup::default();
        assert!(d.admit(&kda(1, 0)));
        assert!(!d.admit(&kda(1, 0)));
        assert!(d.admit(&kda(1, 1)));
        assert!(d.admit(&kda(2, 1)), "per player");
        assert!(d.admit(&kda(1, 0)), "a change back is still a change");
    }

    #[test]
    fn tick_passes_only_when_game_time_advances() {
        let mut d = Dedup::default();
        assert!(d.admit(&ParserEvent::Tick { t_game: 5 }));
        assert!(!d.admit(&ParserEvent::Tick { t_game: 5 }));
        assert!(!d.admit(&ParserEvent::Tick { t_game: 4 }));
        assert!(d.admit(&ParserEvent::Tick { t_game: 6 }));
        assert_eq!(d.last_tick(), Some(6));
    }

    #[test]
    fn end_passes_once() {
        let mut d = Dedup::default();
        assert!(d.admit(&ParserEvent::End { winning_team: 0 }));
        assert!(!d.admit(&ParserEvent::End { winning_team: 0 }));
        assert!(d.has_ended());
    }

    #[test]
    fn roster_and_inventory_compare_by_value() {
        let mut d = Dedup::default();
        let r = ParserEvent::Roster {
            players: vec![RosterEntry {
                account_id: 1,
                name: "a".into(),
                team: 0,
                hero_id: 2,
            }],
        };
        assert!(d.admit(&r));
        assert!(!d.admit(&r.clone()));
        let inv = |items: Vec<u32>| ParserEvent::Inventory {
            account_id: 1,
            items,
        };
        assert!(d.admit(&inv(vec![1, 2])));
        assert!(!d.admit(&inv(vec![1, 2])));
        assert!(d.admit(&inv(vec![1])));
    }

    #[test]
    fn roster_that_gains_a_bot_is_sent_once() {
        let mut d = Dedup::default();
        let human = RosterEntry {
            account_id: 1,
            name: "a".into(),
            team: 0,
            hero_id: 2,
        };
        let bot = RosterEntry {
            account_id: 0,
            name: "bot".into(),
            team: 1,
            hero_id: 3,
        };
        assert!(d.admit(&ParserEvent::Roster {
            players: vec![human.clone()]
        }));
        let with_bot = ParserEvent::Roster {
            players: vec![human, bot],
        };
        assert!(d.admit(&with_bot));
        assert!(!d.admit(&with_bot.clone()));
    }
}
