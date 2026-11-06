//! Pseudonymization of fixtures (DEC-060) and chat stripping (DEC-056).
//!
//! Every player except the developer gets a name `Player N` and a synthetic Account ID. The same
//! [`Pseudonyms`] value is applied to the FX-BC events and to the FX-API files of one match, so a
//! player has one identity across all of a match's fixtures.
//!
//! How the mapping is built (deterministic, nothing stored anywhere):
//! 1. Take every roster entry seen in the match, as `(team, account_id, name)`.
//! 2. Drop the developer (`me`, who keeps their real ID and name) and Account ID 0 (bots: the ID
//!    stays 0 and the name becomes `Bot`).
//! 3. Sort by `(team, account_id)`, number from 1: `Player N` gets ID `SYNTHETIC_BASE + N`.
//! 4. IDs found in other files but not in the roster are numbered after those, in ascending order.
//!
//! Because the roster is the input, `normalize` and `api` build the same mapping from the same raw
//! relay. The mapping is never written to disk.
//!
//! The walk over JSON is by key name (`account_id`, `*_account_id`, `attacker`, `victim`, and
//! `name`-like keys), so a new `ParserEvent` variant is handled without code here. The safety net
//! is [`Pseudonyms::find_leaks`]: a final scan of the output text for any original ID or name. The
//! tool writes nothing if it finds one (fail closed).

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Result};
use serde_json::Value;

/// Synthetic IDs are `SYNTHETIC_BASE + N`. Above any real Account ID so far (those are below
/// 2^31 in practice), and still a valid `u32` for up to ~290 million players.
pub const SYNTHETIC_BASE: u32 = 4_000_000_000;

/// Added to a 32-bit Account ID to get the SteamID64. Only used by the leak scan.
const STEAM_ID64_BASE: u64 = 76_561_197_960_265_728;

/// One roster entry as the mapping needs it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Seen {
    pub team: u8,
    pub account_id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct Pseudonyms {
    me: u32,
    me_name: Option<String>,
    ids: BTreeMap<u32, u32>,
    names: BTreeMap<u32, String>,
    /// Original names of everyone replaced, for the leak scan.
    original_names: BTreeSet<String>,
    next: u32,
}

/// What a leak scan found. The value itself is never put in the message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Leak {
    AccountId,
    SteamId64,
    Name,
}

fn is_player_n(s: &str) -> bool {
    s.strip_prefix("Player ")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

impl Pseudonyms {
    /// Build from the roster entries seen in a match. `me` keeps their real ID and name.
    pub fn from_roster(me: u32, seen: &[Seen]) -> Self {
        let mut p = Self {
            me,
            me_name: None,
            ids: BTreeMap::new(),
            names: BTreeMap::new(),
            original_names: BTreeSet::new(),
            next: 1,
        };
        let mut others: BTreeMap<(u8, u32), &Seen> = BTreeMap::new();
        for s in seen {
            if s.account_id == me {
                p.me_name = Some(s.name.clone());
            } else if s.account_id == 0 {
                p.note_name(&s.name);
            } else {
                others.entry((s.team, s.account_id)).or_insert(s);
            }
        }
        // A player seen on two teams (a swap) keeps the number of the first (team, id) in order.
        for ((_, id), s) in others {
            if !p.ids.contains_key(&id) {
                p.assign(id);
            }
            p.note_name(&s.name);
        }
        // Every name a player has had is recorded for the leak scan, not only the first.
        for s in seen {
            if s.account_id != me {
                p.note_name(&s.name);
            }
        }
        p
    }

    fn note_name(&mut self, name: &str) {
        if !name.is_empty() && !is_player_n(name) && Some(name) != self.me_name.as_deref() {
            self.original_names.insert(name.to_string());
        }
    }

    fn assign(&mut self, id: u32) {
        let n = self.next;
        self.next += 1;
        self.ids.insert(id, SYNTHETIC_BASE + n);
        self.names.insert(id, format!("Player {n}"));
    }

    /// Number IDs found outside the roster (ascending), after the roster's players.
    pub fn add_unknown(&mut self, found: &BTreeSet<u32>) {
        for &id in found {
            if id != self.me && id != 0 && !self.ids.contains_key(&id) {
                self.assign(id);
            }
        }
    }

    pub fn me(&self) -> u32 {
        self.me
    }

    /// The pseudonym of an Account ID. `None` if it is not known yet (see [`Self::add_unknown`]).
    pub fn id(&self, original: u32) -> Option<u32> {
        if original == self.me || original == 0 {
            Some(original)
        } else {
            self.ids.get(&original).copied()
        }
    }

    /// Account IDs of the human players (the developer included), real values. For API calls.
    pub fn human_ids(&self) -> Vec<u32> {
        let mut v: Vec<u32> = self.ids.keys().copied().collect();
        v.push(self.me);
        v.sort_unstable();
        v
    }

    /// Whether an ID belongs in a pseudonymized file: the developer, a bot, or synthetic.
    pub fn is_pseudonymous(&self, id: u32) -> bool {
        id == self.me || id == 0 || self.ids.values().any(|v| *v == id)
    }

    fn name_for(&self, original_id: Option<u32>) -> String {
        match original_id {
            Some(id) if Some(&id) == Some(&self.me) => self.me_name.clone().unwrap_or_default(),
            Some(0) => "Bot".to_string(),
            Some(id) => self
                .names
                .get(&id)
                .cloned()
                .unwrap_or_else(|| "Player".to_string()),
            None => "Player".to_string(),
        }
    }

    // -----------------------------------------------------------------------------------------
    // JSON
    // -----------------------------------------------------------------------------------------

    /// Collect the Account IDs the walk would replace.
    pub fn collect_ids(v: &Value, out: &mut BTreeSet<u32>) {
        Self::collect(v, false, out);
    }

    fn collect(v: &Value, keys_are_ids: bool, out: &mut BTreeSet<u32>) {
        match v {
            Value::Object(m) => {
                for (k, x) in m {
                    if is_id_key(k) {
                        ids_in(x, out);
                    }
                    if keys_are_ids {
                        if let Ok(id) = k.parse::<u32>() {
                            out.insert(id);
                        }
                    }
                    Self::collect(x, is_id_map_key(k), out);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| Self::collect(x, keys_are_ids, out)),
            _ => {}
        }
    }

    /// Replace IDs and names in `v`. Fails if an ID is not in the mapping.
    pub fn apply(&self, v: &Value) -> Result<Value> {
        self.apply_in(v, false)
    }

    fn apply_in(&self, v: &Value, keys_are_ids: bool) -> Result<Value> {
        Ok(match v {
            Value::Object(m) => {
                let sibling = m
                    .iter()
                    .find(|(k, _)| is_id_key(k) && !matches!(k.as_str(), "attacker" | "victim"))
                    .and_then(|(_, x)| first_id(x));
                let mut out = serde_json::Map::new();
                for (k, x) in m {
                    let new_key = match k.parse::<u32>() {
                        Ok(id) if keys_are_ids => self.require(id)?.to_string(),
                        _ => k.clone(),
                    };
                    let new_val = if is_id_key(k) {
                        self.map_ids(x)?
                    } else if is_name_key(k) && x.is_string() {
                        Value::String(self.name_for(sibling))
                    } else {
                        self.apply_in(x, is_id_map_key(k))?
                    };
                    out.insert(new_key, new_val);
                }
                Value::Object(out)
            }
            Value::Array(a) => Value::Array(
                a.iter()
                    .map(|x| self.apply_in(x, keys_are_ids))
                    .collect::<Result<_>>()?,
            ),
            other => other.clone(),
        })
    }

    fn require(&self, id: u32) -> Result<u32> {
        match self.id(id) {
            Some(p) => Ok(p),
            None => bail!("an Account ID is missing from the mapping (call add_unknown first)"),
        }
    }

    fn map_ids(&self, x: &Value) -> Result<Value> {
        Ok(match x {
            Value::Number(n) => match n.as_u64().and_then(|u| u32::try_from(u).ok()) {
                Some(id) => Value::from(self.require(id)?),
                None => x.clone(),
            },
            Value::String(s) => Value::String(
                s.split(',')
                    .map(|part| match part.trim().parse::<u32>() {
                        Ok(id) => self.require(id).map(|p| p.to_string()),
                        Err(_) => Ok(part.to_string()),
                    })
                    .collect::<Result<Vec<_>>>()?
                    .join(","),
            ),
            Value::Array(a) => {
                Value::Array(a.iter().map(|y| self.map_ids(y)).collect::<Result<_>>()?)
            }
            Value::Object(_) => self.apply(x)?,
            Value::Null | Value::Bool(_) => x.clone(),
        })
    }

    /// Scan finished output text for anything that identifies a replaced player. Empty means clean.
    pub fn find_leaks(&self, text: &str) -> Vec<Leak> {
        let mut leaks = Vec::new();
        let originals: Vec<u32> = self.ids.keys().copied().collect();
        if originals.iter().any(|id| has_token(text, &id.to_string())) {
            leaks.push(Leak::AccountId);
        }
        if originals
            .iter()
            .any(|id| has_token(text, &(STEAM_ID64_BASE + u64::from(*id)).to_string()))
        {
            leaks.push(Leak::SteamId64);
        }
        let quoted = |n: &str| serde_json::to_string(n).unwrap_or_default();
        if self.original_names.iter().any(|n| {
            // Short names would match inside other words; only whole JSON strings count for them.
            text.contains(&quoted(n)) || (n.chars().count() >= 4 && text.contains(n.as_str()))
        }) {
            leaks.push(Leak::Name);
        }
        leaks
    }
}

/// Objects whose keys are Account IDs (API-05 single).
fn is_id_map_key(k: &str) -> bool {
    matches!(k, "hero_build_ids" | "pregame_hero_ids")
}

fn is_id_key(k: &str) -> bool {
    matches!(k, "account_id" | "account_ids" | "attacker" | "victim")
        || k.ends_with("_account_id")
        || k.ends_with("_account_ids")
}

fn is_name_key(k: &str) -> bool {
    matches!(
        k,
        "name" | "player_name" | "personaname" | "persona_name" | "display_name" | "realname"
    )
}

fn ids_in(x: &Value, out: &mut BTreeSet<u32>) {
    match x {
        Value::Number(n) => {
            if let Some(id) = n.as_u64().and_then(|u| u32::try_from(u).ok()) {
                if id != 0 {
                    out.insert(id);
                }
            }
        }
        Value::String(s) => {
            for part in s.split(',') {
                if let Ok(id) = part.trim().parse::<u32>() {
                    if id != 0 {
                        out.insert(id);
                    }
                }
            }
        }
        Value::Array(a) => a.iter().for_each(|y| ids_in(y, out)),
        _ => {}
    }
}

fn first_id(x: &Value) -> Option<u32> {
    let mut s = BTreeSet::new();
    ids_in(x, &mut s);
    // 0 (a bot) is not collected above; a plain 0 number is a bot.
    s.into_iter().next().or(match x {
        Value::Number(n) if n.as_u64() == Some(0) => Some(0),
        _ => None,
    })
}

/// Whether `needle` appears in `text` with no digit on either side.
fn has_token(text: &str, needle: &str) -> bool {
    let b = text.as_bytes();
    let mut from = 0;
    while let Some(i) = text[from..].find(needle) {
        let s = from + i;
        let e = s + needle.len();
        let before = s == 0 || !b[s - 1].is_ascii_digit();
        let after = e >= b.len() || !b[e].is_ascii_digit();
        if before && after {
            return true;
        }
        from = s + 1;
    }
    false
}

/// Whether a parsed event line is chat. Chat never reaches a fixture (DEC-056).
pub fn is_chat(event: &Value) -> bool {
    event.get("kind").and_then(Value::as_str) == Some("Chat")
}
