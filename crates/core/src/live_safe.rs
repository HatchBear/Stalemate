//! M-06 Live-Safe filter (PRG-06). See 03-fair-play-and-compliance.md and 06-in-match-recommendations.md.
//!
//! The only path from `GameState` to the UI in Live Mode (NFR-FAIR-003, FP-4). It whitelists by field ID.
//! The output types below have no field for positions, enemy health, enemy cooldowns, ally souls,
//! chat or selected builds, so those can't be sent by mistake. Nothing derived from `Damage`
//! or `Death` events is represented either (FR-LIVE-015).

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::game_state::GameState;
use crate::types::{AccountId, GameSecs, HeroId, ItemId, MatchId};

/// Whitelist field IDs that needed in-game verification. The author verified all of them (T-007,
/// 2026-11-05), but each stays switchable. LS-01, LS-03, LS-04, LS-10, LS-12 and LS-14 are always on.
/// Allies' souls are not on the whitelist at all, so no variant exists for them.
#[derive(Hash, Eq, PartialEq, Ord, PartialOrd, Clone, Copy, Debug, Deserialize)]
pub enum Ls {
    Ls02Name,
    Ls05AllyItems,
    Ls06Kda,
    Ls07Level,
    Ls08EnemySouls,
    Ls09Lane,
    Ls11Objectives,
    Ls13Flex,
}

impl Ls {
    pub const ALL: [Ls; 8] = [
        Ls::Ls02Name,
        Ls::Ls05AllyItems,
        Ls::Ls06Kda,
        Ls::Ls07Level,
        Ls::Ls08EnemySouls,
        Ls::Ls09Lane,
        Ls::Ls11Objectives,
        Ls::Ls13Flex,
    ];
}

/// DATA-003: only fields verified in-game. The default is empty, the safest whitelist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Whitelist(pub HashSet<Ls>);

#[derive(Debug, thiserror::Error)]
pub enum WhitelistError {
    #[error("cannot read live_safe.toml: {0}")]
    Io(#[from] std::io::Error),
    #[error("invalid live_safe.toml (unknown field name or bad syntax): {0}")]
    Parse(#[from] toml::de::Error),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WhitelistFile {
    #[serde(default)]
    enabled: Vec<Ls>,
}

impl Whitelist {
    pub fn has(&self, f: Ls) -> bool {
        self.0.contains(&f)
    }

    /// Parse DATA-003. Unknown field names are rejected.
    pub fn from_toml_str(s: &str) -> Result<Self, WhitelistError> {
        let f: WhitelistFile = toml::from_str(s)?;
        Ok(Whitelist(f.enabled.into_iter().collect()))
    }

    pub fn load(path: &Path) -> Result<Self, WhitelistError> {
        Self::from_toml_str(&std::fs::read_to_string(path)?)
    }
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct PlayerLs {
    pub account_id: AccountId,
    pub team: u8,
    pub hero_id: HeroId, // LS-01, LS-03
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Vec<ItemId>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub kda: Option<(u16, u16, u16)>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub level: Option<u8>,
    /// LS-08: enemies only, never allies or the user (own souls are in `self`, LS-12).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub souls: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lane: Option<u8>,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct DestroyedLs {
    pub team: u8,
    pub kind: String,
    pub t_game_s: GameSecs,
}

#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct ObjectivesLs {
    pub destroyed: Vec<DestroyedLs>,
}

/// The user's own state (LS-12). Always present; `flex_slots` only with `Ls13Flex`.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct SelfLs {
    pub souls: u32,
    pub items: Vec<ItemId>,
    /// Ability id to seconds left. Own abilities only.
    pub cooldowns: BTreeMap<u32, f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub flex_slots: Option<u8>,
}

/// Output of M-06 (doc 11). Fields not listed here can't be represented.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct LiveSafeState {
    pub match_id: MatchId,
    pub t_game_s: GameSecs, // LS-10
    pub delay_s: u32,
    pub players: Vec<PlayerLs>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub objectives: Option<ObjectivesLs>,
    #[serde(rename = "self")]
    pub self_state: SelfLs,
}

pub fn project(gs: &GameState, wl: &Whitelist) -> Vec<PlayerLs> {
    let my_team = gs
        .players
        .iter()
        .find(|p| p.account_id == gs.me)
        .map(|p| p.team);
    gs.players
        .iter()
        .map(|p| {
            // Fail closed: if the user's team is unknown, nobody counts as an enemy.
            let is_enemy = my_team.is_some_and(|t| t != p.team);
            let is_me = p.account_id == gs.me;
            PlayerLs {
                account_id: p.account_id,
                team: p.team,
                hero_id: p.hero_id,
                name: wl.has(Ls::Ls02Name).then(|| p.name.clone()),
                // LS-04: enemy items always. LS-12: own items always. LS-05: other allies only if verified.
                items: (is_enemy || is_me || wl.has(Ls::Ls05AllyItems)).then(|| p.items.clone()),
                kda: wl.has(Ls::Ls06Kda).then_some(p.kda),
                level: wl.has(Ls::Ls07Level).then_some(p.level),
                souls: if is_enemy && wl.has(Ls::Ls08EnemySouls) {
                    p.souls
                } else {
                    None
                },
                // LS-09: verified in-game for every player, not only lane opponents (T-007).
                lane: if wl.has(Ls::Ls09Lane) { p.lane } else { None },
            }
        })
        .collect()
}

pub fn project_state(gs: &GameState, wl: &Whitelist) -> LiveSafeState {
    let own = gs.self_state.clone().unwrap_or_default();
    // Own items come from the user's roster entry when the self state has none yet.
    let own_items = if own.items.is_empty() {
        gs.players
            .iter()
            .find(|p| p.account_id == gs.me)
            .map(|p| p.items.clone())
            .unwrap_or_default()
    } else {
        own.items
    };
    LiveSafeState {
        match_id: gs.match_id,
        t_game_s: gs.t_game,
        delay_s: gs.delay_s,
        players: project(gs, wl),
        objectives: wl.has(Ls::Ls11Objectives).then(|| ObjectivesLs {
            destroyed: gs
                .objectives
                .iter()
                .map(|(team, kind, t)| DestroyedLs {
                    team: *team,
                    kind: kind.clone(),
                    t_game_s: *t,
                })
                .collect(),
        }),
        self_state: SelfLs {
            souls: own.souls,
            items: own_items,
            cooldowns: own.cooldowns.iter().copied().collect(),
            flex_slots: wl.has(Ls::Ls13Flex).then_some(own.flex_slots),
        },
    }
}

/// Every JSON key path that may appear in a serialized `LiveSafeState` under this whitelist.
/// Paths are dot-joined; arrays add no segment. `self.cooldowns` is a map keyed by ability id, so
/// it is treated as one opaque path.
pub fn allowed_keys(wl: &Whitelist) -> BTreeSet<String> {
    let mut k: BTreeSet<String> = [
        "match_id",
        "t_game_s",
        "delay_s",
        "players",
        "players.account_id",
        "players.team",
        "players.hero_id",
        "players.items", // LS-04 enemies, LS-12 self; allies gated in `project`
        "self",
        "self.souls",
        "self.items",
        "self.cooldowns",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let mut add = |on: bool, keys: &[&str]| {
        if on {
            k.extend(keys.iter().map(|s| s.to_string()));
        }
    };
    add(wl.has(Ls::Ls02Name), &["players.name"]);
    add(wl.has(Ls::Ls06Kda), &["players.kda"]);
    add(wl.has(Ls::Ls07Level), &["players.level"]);
    add(wl.has(Ls::Ls08EnemySouls), &["players.souls"]);
    add(wl.has(Ls::Ls09Lane), &["players.lane"]);
    add(wl.has(Ls::Ls13Flex), &["self.flex_slots"]);
    add(
        wl.has(Ls::Ls11Objectives),
        &[
            "objectives",
            "objectives.destroyed",
            "objectives.destroyed.team",
            "objectives.destroyed.kind",
            "objectives.destroyed.t_game_s",
        ],
    );
    k
}

fn walk(v: &serde_json::Value, prefix: &str, out: &mut BTreeSet<String>) {
    match v {
        serde_json::Value::Object(m) => {
            for (key, child) in m {
                let path = if prefix.is_empty() {
                    key.clone()
                } else {
                    format!("{prefix}.{key}")
                };
                out.insert(path.clone());
                if path != "self.cooldowns" {
                    walk(child, &path, out);
                }
            }
        }
        serde_json::Value::Array(a) => a.iter().for_each(|c| walk(c, prefix, out)),
        _ => {}
    }
}

/// Every key path present in a payload (same path rules as `allowed_keys`).
pub fn key_paths(payload: &serde_json::Value) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    walk(payload, "", &mut out);
    out
}

/// TST-030 check: lists every key in the payload that the whitelist doesn't allow.
pub fn check_payload(payload: &serde_json::Value, wl: &Whitelist) -> Result<(), Vec<String>> {
    let allowed = allowed_keys(wl);
    let mut bad: Vec<String> = key_paths(payload).difference(&allowed).cloned().collect();
    // Cooldowns must be a flat map of numbers. Nothing else may hide in there.
    if let Some(cd) = payload.get("self").and_then(|s| s.get("cooldowns")) {
        let ok = cd
            .as_object()
            .is_some_and(|m| m.values().all(|x| x.is_number()));
        if !ok {
            bad.push("self.cooldowns (not a map of numbers)".into());
        }
    }
    if !payload.is_object() {
        bad.push("(payload is not an object)".into());
    }
    if bad.is_empty() {
        Ok(())
    } else {
        Err(bad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{ParserEvent, RosterEntry};
    use proptest::prelude::*;
    use std::time::{Duration, SystemTime};

    fn t0() -> SystemTime {
        SystemTime::UNIX_EPOCH
    }

    /// Me = account 1 on team 0. Allies 2, 4, 6 (team 0). Enemies 3, 5, 7 (team 1).
    fn state() -> GameState {
        let mut gs = GameState::new(9, 1, t0());
        gs.apply(
            &ParserEvent::Roster {
                players: (1..=7)
                    .map(|i| RosterEntry {
                        account_id: i,
                        name: format!("n{i}"),
                        team: (i % 2 == 1 && i != 1) as u8,
                        hero_id: i,
                    })
                    .collect(),
            },
            t0(),
        );
        // team: 1 -> 0, 2 -> 0, 3 -> 1, 4 -> 0, 5 -> 1, 6 -> 0, 7 -> 1
        for i in 1..=7u32 {
            gs.apply(
                &ParserEvent::Inventory {
                    account_id: i,
                    items: vec![i * 10],
                },
                t0(),
            );
            gs.apply(
                &ParserEvent::Position {
                    account_id: i,
                    x: 1.0,
                    y: 2.0,
                    z: 3.0,
                },
                t0(),
            );
        }
        gs
    }

    fn json(gs: &GameState, wl: &Whitelist) -> serde_json::Value {
        serde_json::to_value(project_state(gs, wl)).unwrap()
    }

    #[test]
    fn tc_06_1_empty_whitelist_minimal_players() {
        let gs = state();
        let v = json(&gs, &Whitelist::default());
        for p in v["players"].as_array().unwrap() {
            let keys: BTreeSet<_> = p.as_object().unwrap().keys().cloned().collect();
            let team = p["team"].as_u64().unwrap();
            let me_or_enemy = team == 1 || p["account_id"] == 1;
            let mut want: BTreeSet<String> = ["account_id", "team", "hero_id"]
                .iter()
                .map(|s| s.to_string())
                .collect();
            if me_or_enemy {
                want.insert("items".into());
            }
            assert_eq!(keys, want, "player {p}");
        }
        assert!(v.get("objectives").is_none());
        assert!(v["self"].get("flex_slots").is_none());
        check_payload(&v, &Whitelist::default()).unwrap();
    }

    #[test]
    fn own_items_shown_but_other_allies_hidden_without_ls05() {
        let gs = state();
        let players = project(&gs, &Whitelist::default());
        let get = |id| players.iter().find(|p| p.account_id == id).unwrap();
        assert!(get(1).items.is_some());
        assert!(get(2).items.is_none());
        assert!(get(3).items.is_some());
        let wl = Whitelist([Ls::Ls05AllyItems].into());
        assert!(project(&gs, &wl).iter().all(|p| p.items.is_some()));
    }

    #[test]
    fn unknown_team_fails_closed() {
        let mut gs = state();
        gs.me = 999;
        assert!(project(&gs, &Whitelist::default())
            .iter()
            .all(|p| p.items.is_none()));
    }

    #[test]
    fn tc_06_2_no_position_anywhere() {
        let gs = state();
        assert!(gs.players.iter().all(|p| p.position.is_some()));
        let all = Whitelist(Ls::ALL.iter().copied().collect());
        let s = serde_json::to_string(&json(&gs, &all)).unwrap();
        for bad in [
            "position", "\"x\"", "\"y\"", "\"z\"", "chat", "damage", "health",
        ] {
            assert!(!s.contains(bad), "{bad} in {s}");
        }
    }

    #[test]
    fn tc_06_3_kda_present_for_all() {
        let mut gs = state();
        gs.apply(
            &ParserEvent::Kda {
                account_id: 3,
                kills: 1,
                deaths: 2,
                assists: 3,
            },
            t0(),
        );
        let wl = Whitelist([Ls::Ls06Kda].into());
        let v = json(&gs, &wl);
        let ps = v["players"].as_array().unwrap();
        assert!(ps.iter().all(|p| p.get("kda").is_some()));
        assert_eq!(ps[2]["kda"], serde_json::json!([1, 2, 3]));
        check_payload(&v, &wl).unwrap();
    }

    #[test]
    fn objectives_only_with_ls11_and_flex_only_with_ls13() {
        let mut gs = state();
        gs.apply(
            &ParserEvent::Objective {
                team: 1,
                kind: "guardian".into(),
                t_game: 400,
            },
            t0(),
        );
        gs.apply(
            &ParserEvent::SelfState {
                souls: 5,
                cooldowns: vec![(3, 1.5)],
                flex_slots: 2,
            },
            t0(),
        );
        let v = json(&gs, &Whitelist::default());
        assert!(v.get("objectives").is_none());
        assert!(v["self"].get("flex_slots").is_none());
        assert_eq!(v["self"]["souls"], 5);
        assert_eq!(v["self"]["cooldowns"]["3"], 1.5);
        let wl = Whitelist([Ls::Ls11Objectives, Ls::Ls13Flex].into());
        let v = json(&gs, &wl);
        assert_eq!(v["objectives"]["destroyed"][0]["kind"], "guardian");
        assert_eq!(v["self"]["flex_slots"], 2);
        check_payload(&v, &wl).unwrap();
    }

    #[test]
    fn self_items_present_even_with_empty_self_state() {
        let gs = state();
        let v = json(&gs, &Whitelist::default());
        assert_eq!(v["self"]["items"], serde_json::json!([10]));
    }

    #[test]
    fn fight_events_leave_no_trace() {
        let mut a = state();
        let b = state();
        a.apply(
            &ParserEvent::Damage {
                attacker: Some(1),
                victim: 3,
                amount: 100,
                source: "gun".into(),
            },
            t0(),
        );
        a.apply(
            &ParserEvent::Death {
                account_id: 3,
                t_game: 5,
            },
            t0(),
        );
        a.apply(
            &ParserEvent::Chat {
                account_id: 3,
                team_only: true,
                text: "secret".into(),
            },
            t0(),
        );
        let all = Whitelist(Ls::ALL.iter().copied().collect());
        assert_eq!(json(&a, &all), json(&b, &all));
    }

    #[test]
    fn check_payload_rejects_forbidden_keys() {
        let wl = Whitelist::default();
        let v = serde_json::json!({
            "match_id": 1, "t_game_s": 0, "delay_s": 0,
            "players": [{"account_id": 1, "team": 0, "hero_id": 1, "position": [1,2,3], "souls": 5}],
            "self": {"souls": 0, "items": [], "cooldowns": {}},
            "chat": []
        });
        let bad = check_payload(&v, &wl).unwrap_err();
        assert!(bad.contains(&"players.position".to_string()));
        assert!(bad.contains(&"players.souls".to_string()));
        assert!(bad.contains(&"chat".to_string()));
        // A gated key is rejected when the whitelist doesn't enable it.
        let v = serde_json::json!({"players": [{"kda": [1,2,3]}]});
        assert!(check_payload(&v, &wl).is_err());
        assert!(check_payload(&v, &Whitelist([Ls::Ls06Kda].into())).is_ok());
    }

    #[test]
    fn shipped_config_loads_and_matches_doc_14() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/live_safe.toml");
        let wl = Whitelist::load(&path).unwrap();
        assert_eq!(wl, Whitelist(Ls::ALL.iter().copied().collect()));
        assert_eq!(wl.0.len(), 8);
    }

    #[test]
    fn ls08_enemy_souls_only_for_enemies() {
        let mut gs = state();
        for i in 1..=7u32 {
            gs.set_souls(i, Some(i * 1000));
        }
        let on = Whitelist([Ls::Ls08EnemySouls].into());
        let players = project(&gs, &on);
        for p in &players {
            if p.team == 1 {
                assert_eq!(p.souls, Some(p.account_id * 1000), "enemy {}", p.account_id);
            } else {
                assert!(p.souls.is_none(), "ally or self {}", p.account_id);
            }
        }
        let v = json(&gs, &on);
        check_payload(&v, &on).unwrap();
        assert_eq!(v["self"]["souls"], 0);
        // Disabled: nobody carries souls, and the payload check rejects the key.
        let off = Whitelist::default();
        assert!(project(&gs, &off).iter().all(|p| p.souls.is_none()));
        let v = json(&gs, &off);
        assert!(!serde_json::to_string(&v["players"])
            .unwrap()
            .contains("souls"));
        let bad = serde_json::json!({"players": [{"souls": 5}]});
        assert!(check_payload(&bad, &off).is_err());
        assert!(check_payload(&bad, &on).is_ok());
    }

    #[test]
    fn ls08_unknown_team_fails_closed() {
        let mut gs = state();
        gs.me = 999;
        for i in 1..=7u32 {
            gs.set_souls(i, Some(1));
        }
        let on = Whitelist([Ls::Ls08EnemySouls].into());
        assert!(project(&gs, &on).iter().all(|p| p.souls.is_none()));
    }

    #[test]
    fn loader_rejects_unknown_names() {
        assert!(Whitelist::from_toml_str("enabled = [\"Ls08AllySouls\"]").is_err());
        assert!(Whitelist::from_toml_str("enabled = [\"Ls08EnemySouls\"]").is_ok());
        assert!(Whitelist::from_toml_str("enabled = [\"Ls02Name\"]\nextra = 1").is_err());
        assert!(Whitelist::from_toml_str("").unwrap().0.is_empty());
        assert!(Whitelist::from_toml_str("enabled = [\"Ls06Kda\", \"Ls13Flex\"]").is_ok());
    }

    fn arb_event() -> impl Strategy<Value = ParserEvent> {
        let id = 0u32..8;
        prop_oneof![
            (id.clone(), proptest::collection::vec(0u32..500, 0..6))
                .prop_map(|(account_id, items)| ParserEvent::Inventory { account_id, items }),
            (id.clone(), any::<u16>(), any::<u16>(), any::<u16>()).prop_map(
                |(account_id, kills, deaths, assists)| ParserEvent::Kda {
                    account_id,
                    kills,
                    deaths,
                    assists
                }
            ),
            (id.clone(), any::<u8>())
                .prop_map(|(account_id, level)| ParserEvent::Level { account_id, level }),
            (0u8..2, "[a-z]{1,8}", 0u32..3000)
                .prop_map(|(team, kind, t_game)| { ParserEvent::Objective { team, kind, t_game } }),
            (
                any::<u32>(),
                proptest::collection::vec((0u32..20, 0.0f32..100.0), 0..4),
                any::<u8>()
            )
                .prop_map(|(souls, cooldowns, flex_slots)| ParserEvent::SelfState {
                    souls,
                    cooldowns,
                    flex_slots
                }),
            (id.clone(), any::<f32>(), any::<f32>(), any::<f32>()).prop_map(
                |(account_id, x, y, z)| ParserEvent::Position {
                    account_id,
                    x,
                    y,
                    z
                }
            ),
            (id.clone(), id.clone(), any::<u32>()).prop_map(|(a, victim, amount)| {
                ParserEvent::Damage {
                    attacker: Some(a),
                    victim,
                    amount,
                    source: "s".into(),
                }
            }),
            (id.clone(), 0u32..3000)
                .prop_map(|(account_id, t_game)| ParserEvent::Death { account_id, t_game }),
            (id, any::<bool>(), "[a-z ]{0,20}").prop_map(|(account_id, team_only, text)| {
                ParserEvent::Chat {
                    account_id,
                    team_only,
                    text,
                }
            }),
            (0u32..3000).prop_map(|t_game| ParserEvent::Tick { t_game }),
        ]
    }

    proptest! {
        // TC-06-4, NFR-FAIR-003, FP-4, FR-LIVE-015
        #[test]
        fn tc_06_4_keys_always_subset_of_allowed(
            events in proptest::collection::vec(arb_event(), 0..60),
            enabled in proptest::collection::vec(any::<bool>(), Ls::ALL.len()),
            me in 0u32..9,
            wall in 0u64..500,
        ) {
            let wl = Whitelist(
                Ls::ALL.iter().zip(&enabled).filter(|(_, e)| **e).map(|(l, _)| *l).collect(),
            );
            let mut gs = GameState::new(1, me, t0());
            gs.apply(
                &ParserEvent::Roster {
                    players: (0..8).map(|i| RosterEntry {
                        account_id: i, name: format!("p{i}"), team: (i % 2) as u8, hero_id: i,
                    }).collect(),
                },
                t0(),
            );
            for e in &events {
                gs.apply(e, t0() + Duration::from_secs(wall));
            }
            gs.set_lane(2, Some(1));
            for i in 0..8 {
                gs.set_souls(i, Some(i * 100));
            }
            let v = serde_json::to_value(project_state(&gs, &wl)).unwrap();
            let allowed = allowed_keys(&wl);
            let present = key_paths(&v);
            prop_assert!(present.is_subset(&allowed), "extra: {:?}", present.difference(&allowed).collect::<Vec<_>>());
            prop_assert!(check_payload(&v, &wl).is_ok());
            // LS-08: souls only ever on enemies, never on allies or the user's own entry.
            let my_team = gs.players.iter().find(|p| p.account_id == me).map(|p| p.team);
            for p in project(&gs, &wl) {
                if p.souls.is_some() {
                    prop_assert!(wl.has(Ls::Ls08EnemySouls));
                    prop_assert!(p.account_id != me);
                    prop_assert!(my_team.is_some_and(|t| t != p.team));
                }
            }
            if let Some(arr) = v["players"].as_array() {
                for pj in arr {
                    if pj.get("souls").is_some() {
                        prop_assert!(pj["account_id"] != me && pj["team"] != my_team.unwrap_or(99));
                    }
                }
            }
            // Without the ally-items field, only the user and enemies carry items.
            if !wl.has(Ls::Ls05AllyItems) {
                let my_team = gs.players.iter().find(|p| p.account_id == me).map(|p| p.team);
                for p in project(&gs, &wl) {
                    if p.items.is_some() {
                        prop_assert!(p.account_id == me || my_team.is_some_and(|t| t != p.team));
                    }
                }
            }
        }
    }
}
