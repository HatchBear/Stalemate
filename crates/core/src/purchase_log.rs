//! M-07 Purchase Log (PRG-07). See 06-in-match-recommendations.md.
//!
//! Turns inventory changes into `PurchaseEvent`s (FR-LIVE-002). `PurchaseLog` reads only
//! `LiveSafeState`, so only items the whitelist lets through can produce events (FP-4).

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::live_safe::LiveSafeState;
use crate::types::{AccountId, GameSecs, ItemId, MatchId};

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Action {
    Buy,
    Sell,
    Upgrade,
}

#[derive(Debug, PartialEq, Clone, Serialize, Deserialize)]
pub struct PurchaseEvent {
    pub match_id: MatchId,
    pub account_id: AccountId,
    pub t_game_s: GameSecs,
    pub item_id: ItemId,
    pub action: Action,
    pub cost: u32,
}

pub trait ItemTree {
    fn is_component_of(&self, component: ItemId, item: ItemId) -> bool;
    fn cost(&self, item: ItemId) -> u32;
}

/// Compare two item lists for one player. `match_id` is 0 in the result; `PurchaseLog` stamps it.
pub fn diff(
    prev: &[ItemId],
    next: &[ItemId],
    tree: &impl ItemTree,
    t: GameSecs,
    acct: AccountId,
) -> Vec<PurchaseEvent> {
    let removed: Vec<ItemId> = prev.iter().copied().filter(|i| !next.contains(i)).collect();
    let added: Vec<ItemId> = next.iter().copied().filter(|i| !prev.contains(i)).collect();
    let mut consumed = HashSet::new();
    let mut out = Vec::new();
    for a in added {
        let comps: Vec<ItemId> = removed
            .iter()
            .copied()
            .filter(|r| tree.is_component_of(*r, a))
            .collect();
        let action = if comps.is_empty() {
            Action::Buy
        } else {
            consumed.extend(comps);
            Action::Upgrade
        };
        out.push(PurchaseEvent {
            match_id: 0,
            account_id: acct,
            t_game_s: t,
            item_id: a,
            action,
            cost: tree.cost(a),
        });
    }
    for r in removed.into_iter().filter(|r| !consumed.contains(r)) {
        out.push(PurchaseEvent {
            match_id: 0,
            account_id: acct,
            t_game_s: t,
            item_id: r,
            action: Action::Sell,
            cost: tree.cost(r),
        });
    }
    out
}

/// Keeps the previous inventory per player and the full event list for the match. Persistence to
/// `purchase_events` is wired elsewhere.
#[derive(Debug, Default)]
pub struct PurchaseLog {
    match_id: MatchId,
    last: HashMap<AccountId, Vec<ItemId>>,
    events: Vec<PurchaseEvent>,
}

impl PurchaseLog {
    pub fn new(match_id: MatchId) -> Self {
        Self {
            match_id,
            ..Self::default()
        }
    }

    /// Feed the next Live-Safe state. Returns the new events. A player whose items the whitelist
    /// hides (`items` is `None`) produces nothing and keeps their last known inventory. A player
    /// seen for the first time is compared against an empty inventory.
    pub fn update(&mut self, state: &LiveSafeState, tree: &impl ItemTree) -> Vec<PurchaseEvent> {
        let mut new = Vec::new();
        for p in &state.players {
            let Some(items) = &p.items else { continue };
            let prev = self
                .last
                .get(&p.account_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let mut evs = diff(prev, items, tree, state.t_game_s, p.account_id);
            for e in &mut evs {
                e.match_id = self.match_id;
            }
            new.extend(evs);
            self.last.insert(p.account_id, items.clone());
        }
        self.events.extend(new.iter().cloned());
        new
    }

    /// Every event so far this match, in order.
    pub fn events(&self) -> &[PurchaseEvent] {
        &self.events
    }

    pub fn events_for(&self, account_id: AccountId) -> impl Iterator<Item = &PurchaseEvent> {
        self.events
            .iter()
            .filter(move |e| e.account_id == account_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::live_safe::{PlayerLs, SelfLs};
    use std::collections::BTreeMap;

    const A: ItemId = 1;
    const B: ItemId = 2;
    const C: ItemId = 3;

    struct Tree;
    impl ItemTree for Tree {
        fn is_component_of(&self, component: ItemId, item: ItemId) -> bool {
            component == A && item == B
        }
        fn cost(&self, item: ItemId) -> u32 {
            item * 100
        }
    }

    fn ev(item_id: ItemId, action: Action) -> PurchaseEvent {
        PurchaseEvent {
            match_id: 0,
            account_id: 5,
            t_game_s: 60,
            item_id,
            action,
            cost: item_id * 100,
        }
    }

    #[test]
    fn tc_07_1_buy() {
        assert_eq!(diff(&[], &[A], &Tree, 60, 5), vec![ev(A, Action::Buy)]);
    }

    #[test]
    fn tc_07_2_upgrade_only() {
        assert_eq!(diff(&[A], &[B], &Tree, 60, 5), vec![ev(B, Action::Upgrade)]);
    }

    #[test]
    fn tc_07_3_sell() {
        assert_eq!(diff(&[A], &[], &Tree, 60, 5), vec![ev(A, Action::Sell)]);
    }

    #[test]
    fn tc_07_4_upgrade_and_sell() {
        assert_eq!(
            diff(&[A, C], &[B], &Tree, 60, 5),
            vec![ev(B, Action::Upgrade), ev(C, Action::Sell)]
        );
    }

    #[test]
    fn serializes_per_doc_11() {
        let mut e = ev(B, Action::Upgrade);
        e.match_id = 9;
        let v = serde_json::to_value(&e).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"match_id": 9, "account_id": 5, "t_game_s": 60,
                "item_id": 2, "action": "upgrade", "cost": 200})
        );
        assert_eq!(serde_json::to_value(Action::Buy).unwrap(), "buy");
        assert_eq!(serde_json::to_value(Action::Sell).unwrap(), "sell");
    }

    fn player(id: AccountId, items: Option<Vec<ItemId>>) -> PlayerLs {
        PlayerLs {
            account_id: id,
            team: 0,
            hero_id: 1,
            name: None,
            items,
            kda: None,
            level: None,
            souls: None,
            lane: None,
        }
    }

    fn state(t: GameSecs, players: Vec<PlayerLs>) -> LiveSafeState {
        LiveSafeState {
            match_id: 9,
            t_game_s: t,
            delay_s: 0,
            players,
            objectives: None,
            self_state: SelfLs {
                souls: 0,
                items: vec![],
                cooldowns: BTreeMap::new(),
                flex_slots: None,
            },
        }
    }

    #[test]
    fn log_tracks_changes_across_states_and_stamps_match_id() {
        let mut log = PurchaseLog::new(9);
        let e1 = log.update(&state(10, vec![player(5, Some(vec![A]))]), &Tree);
        assert_eq!(e1.len(), 1);
        assert_eq!(e1[0].match_id, 9);
        assert!(log
            .update(&state(20, vec![player(5, Some(vec![A]))]), &Tree)
            .is_empty());
        let e3 = log.update(&state(30, vec![player(5, Some(vec![B]))]), &Tree);
        assert_eq!(e3[0].action, Action::Upgrade);
        assert_eq!(e3[0].t_game_s, 30);
        assert_eq!(log.events().len(), 2);
        assert_eq!(log.events_for(5).count(), 2);
    }

    #[test]
    fn hidden_ally_items_produce_no_events() {
        use crate::game_state::GameState;
        use crate::live_safe::{project_state, Whitelist};
        use crate::types::{ParserEvent, RosterEntry};
        let now = std::time::SystemTime::UNIX_EPOCH;
        let mut gs = GameState::new(9, 1, now);
        let roster = |id, team| RosterEntry {
            account_id: id,
            name: "n".into(),
            team,
            hero_id: id,
        };
        gs.apply(
            &ParserEvent::Roster {
                players: vec![roster(1, 0), roster(2, 0), roster(3, 1)],
            },
            now,
        );
        for id in 1..=3 {
            gs.apply(
                &ParserEvent::Inventory {
                    account_id: id,
                    items: vec![A],
                },
                now,
            );
        }
        let mut log = PurchaseLog::new(9);
        let evs = log.update(&project_state(&gs, &Whitelist::default()), &Tree);
        let ids: HashSet<_> = evs.iter().map(|e| e.account_id).collect();
        assert_eq!(ids, HashSet::from([1, 3]), "ally 2 must be hidden");
    }
}
