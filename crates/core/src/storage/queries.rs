//! Typed queries for the Alpha tables: players, matches, match_players, purchase_events.
//! Builds, reviews and recommendation history get their queries with their own tasks.

use super::{Result, Storage, StorageError};
use crate::purchase_log::{Action, PurchaseEvent};
use crate::types::{AccountId, HeroId, MatchId};
use rusqlite::{params, OptionalExtension, Row};

macro_rules! text_enum {
    ($(#[$m:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$m])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub enum $name { $($variant),+ }
        impl $name {
            pub fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $text),+ }
            }
            fn parse(s: &str) -> Result<Self> {
                match s {
                    $($text => Ok(Self::$variant),)+
                    other => Err(StorageError::BadValue(format!(
                        "{}: {other:?}", stringify!($name)
                    ))),
                }
            }
        }
    };
}

/// The `matches.game_mode` column. The two known modes have variants; any other mode the API
/// reports is kept as `Other` with its lowercase snake_case name, so a new mode never fails a read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GameMode {
    Normal,
    StreetBrawl,
    Other(String),
}

impl GameMode {
    pub fn as_str(&self) -> &str {
        match self {
            Self::Normal => "normal",
            Self::StreetBrawl => "street_brawl",
            Self::Other(s) => s,
        }
    }

    /// `normal` and `street_brawl` give their variants; any other non-empty `[a-z0-9_]` name gives
    /// `Other`. Anything else (empty, upper case, spaces) is a bad value.
    pub fn parse(s: &str) -> Result<Self> {
        match s {
            "normal" => Ok(Self::Normal),
            "street_brawl" => Ok(Self::StreetBrawl),
            other
                if !other.is_empty()
                    && other
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_') =>
            {
                Ok(Self::Other(other.to_string()))
            }
            other => Err(StorageError::BadValue(format!("GameMode: {other:?}"))),
        }
    }
}

text_enum!(MatchResult { Win => "win", Loss => "loss", Abandoned => "abandoned" });
text_enum!(DataSource { Broadcast => "broadcast", Replay => "replay", Api => "api" });

fn action_str(a: Action) -> &'static str {
    match a {
        Action::Buy => "buy",
        Action::Sell => "sell",
        Action::Upgrade => "upgrade",
    }
}

fn parse_action(s: &str) -> Result<Action> {
    match s {
        "buy" => Ok(Action::Buy),
        "sell" => Ok(Action::Sell),
        "upgrade" => Ok(Action::Upgrade),
        other => Err(StorageError::BadValue(format!("action: {other:?}"))),
    }
}

fn to_i64(v: u64, what: &'static str) -> Result<i64> {
    i64::try_from(v).map_err(|_| StorageError::OutOfRange(what))
}

fn to_u64(v: i64, what: &'static str) -> Result<u64> {
    u64::try_from(v).map_err(|_| StorageError::BadValue(format!("{what} is negative")))
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlayerRow {
    pub account_id: AccountId,
    pub display_name: Option<String>,
    /// RFC 3339.
    pub last_seen_at: String,
}

/// A row of `matches`: the user's own matches.
#[derive(Debug, Clone, PartialEq)]
pub struct MatchRow {
    pub match_id: MatchId,
    /// RFC 3339.
    pub started_at: String,
    pub duration_s: Option<u32>,
    pub game_mode: GameMode,
    /// 'ranked' | 'unranked' | ... (free text, doc 11).
    pub match_mode: Option<String>,
    pub patch: String,
    pub user_team: u8,
    pub user_hero_id: HeroId,
    pub result: Option<MatchResult>,
    pub data_source: DataSource,
    pub recording_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchPlayerRow {
    pub account_id: AccountId,
    pub team: u8,
    pub hero_id: HeroId,
    pub assigned_lane: Option<u8>,
    pub party_id: Option<u32>,
}

const MATCH_COLS: &str = "match_id, started_at, duration_s, game_mode, match_mode, patch, \
     user_team, user_hero_id, result, data_source, recording_path";

fn match_from_row(r: &Row<'_>) -> Result<MatchRow> {
    Ok(MatchRow {
        match_id: to_u64(r.get(0)?, "match_id")?,
        started_at: r.get(1)?,
        duration_s: r.get(2)?,
        game_mode: GameMode::parse(&r.get::<_, String>(3)?)?,
        match_mode: r.get(4)?,
        patch: r.get(5)?,
        user_team: r.get(6)?,
        user_hero_id: r.get(7)?,
        result: r
            .get::<_, Option<String>>(8)?
            .map(|s| MatchResult::parse(&s))
            .transpose()?,
        data_source: DataSource::parse(&r.get::<_, String>(9)?)?,
        recording_path: r.get(10)?,
    })
}

impl Storage {
    /// Insert a player, or update the last-seen time. A `None` name keeps the stored name.
    pub fn upsert_player(
        &self,
        account_id: AccountId,
        display_name: Option<&str>,
        last_seen_at: &str,
    ) -> Result<()> {
        self.lock().execute(
            "INSERT INTO players (account_id, display_name, last_seen_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(account_id) DO UPDATE SET
               display_name = COALESCE(excluded.display_name, display_name),
               last_seen_at = excluded.last_seen_at",
            params![account_id, display_name, last_seen_at],
        )?;
        Ok(())
    }

    pub fn get_player(&self, account_id: AccountId) -> Result<Option<PlayerRow>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT account_id, display_name, last_seen_at FROM players WHERE account_id = ?1",
                [account_id],
                |r| {
                    Ok(PlayerRow {
                        account_id: r.get(0)?,
                        display_name: r.get(1)?,
                        last_seen_at: r.get(2)?,
                    })
                },
            )
            .optional()?)
    }

    /// Insert a match and its players in one transaction. If the match exists, its row is updated
    /// in place (so purchases and reviews stay) and its player list is replaced.
    pub fn insert_match(&self, m: &MatchRow, players: &[MatchPlayerRow]) -> Result<()> {
        let id = to_i64(m.match_id, "match_id")?;
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO matches (match_id, started_at, duration_s, game_mode, match_mode, patch,
               user_team, user_hero_id, result, data_source, recording_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(match_id) DO UPDATE SET
               started_at = excluded.started_at, duration_s = excluded.duration_s,
               game_mode = excluded.game_mode, match_mode = excluded.match_mode,
               patch = excluded.patch, user_team = excluded.user_team,
               user_hero_id = excluded.user_hero_id, result = excluded.result,
               data_source = excluded.data_source, recording_path = excluded.recording_path",
            params![
                id,
                m.started_at,
                m.duration_s,
                m.game_mode.as_str(),
                m.match_mode,
                m.patch,
                m.user_team,
                m.user_hero_id,
                m.result.map(MatchResult::as_str),
                m.data_source.as_str(),
                m.recording_path,
            ],
        )?;
        tx.execute("DELETE FROM match_players WHERE match_id = ?1", [id])?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO match_players (match_id, account_id, team, hero_id, assigned_lane, party_id)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for p in players {
                ins.execute(params![
                    id,
                    p.account_id,
                    p.team,
                    p.hero_id,
                    p.assigned_lane,
                    p.party_id
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    pub fn get_match(&self, match_id: MatchId) -> Result<Option<MatchRow>> {
        let id = to_i64(match_id, "match_id")?;
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MATCH_COLS} FROM matches WHERE match_id = ?1"
        ))?;
        let mut rows = stmt.query([id])?;
        match rows.next()? {
            Some(r) => Ok(Some(match_from_row(r)?)),
            None => Ok(None),
        }
    }

    pub fn get_match_players(&self, match_id: MatchId) -> Result<Vec<MatchPlayerRow>> {
        let id = to_i64(match_id, "match_id")?;
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT account_id, team, hero_id, assigned_lane, party_id
             FROM match_players WHERE match_id = ?1 ORDER BY team, account_id",
        )?;
        let rows = stmt.query_map([id], |r| {
            Ok(MatchPlayerRow {
                account_id: r.get(0)?,
                team: r.get(1)?,
                hero_id: r.get(2)?,
                assigned_lane: r.get(3)?,
                party_id: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// The user's matches, newest first. `limit` of `None` returns all.
    pub fn list_matches(&self, limit: Option<u32>) -> Result<Vec<MatchRow>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {MATCH_COLS} FROM matches ORDER BY started_at DESC, match_id DESC LIMIT ?1"
        ))?;
        let limit = limit.map(i64::from).unwrap_or(-1); // SQLite: negative = no limit
        let mut rows = stmt.query([limit])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(match_from_row(r)?);
        }
        Ok(out)
    }

    /// Delete a match. Players, purchases, recommendations and the Review go with it (cascade).
    /// Returns whether a match was deleted.
    pub fn delete_match(&self, match_id: MatchId) -> Result<bool> {
        let id = to_i64(match_id, "match_id")?;
        Ok(self
            .lock()
            .execute("DELETE FROM matches WHERE match_id = ?1", [id])?
            > 0)
    }

    /// Insert purchase events in one transaction. The matches must exist.
    pub fn insert_purchase_events(&self, events: &[PurchaseEvent]) -> Result<()> {
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        {
            let mut ins = tx.prepare(
                "INSERT INTO purchase_events (match_id, account_id, t_game_s, item_id, action, cost)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            )?;
            for e in events {
                ins.execute(params![
                    to_i64(e.match_id, "match_id")?,
                    e.account_id,
                    e.t_game_s,
                    e.item_id,
                    action_str(e.action),
                    e.cost
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Purchase events for a match in game-time order (ties keep insert order).
    pub fn list_purchase_events(&self, match_id: MatchId) -> Result<Vec<PurchaseEvent>> {
        let id = to_i64(match_id, "match_id")?;
        let conn = self.lock();
        let mut stmt = conn.prepare(
            "SELECT match_id, account_id, t_game_s, item_id, action, cost
             FROM purchase_events WHERE match_id = ?1 ORDER BY t_game_s, id",
        )?;
        let mut rows = stmt.query([id])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(PurchaseEvent {
                match_id: to_u64(r.get(0)?, "match_id")?,
                account_id: r.get(1)?,
                t_game_s: r.get(2)?,
                item_id: r.get(3)?,
                action: parse_action(&r.get::<_, String>(4)?)?,
                cost: r.get(5)?,
            });
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn game_mode_round_trips_any_snake_case_mode() {
        for (text, mode) in [
            ("normal", GameMode::Normal),
            ("street_brawl", GameMode::StreetBrawl),
            ("hero_labs", GameMode::Other("hero_labs".into())),
        ] {
            assert_eq!(GameMode::parse(text).unwrap(), mode);
            assert_eq!(mode.as_str(), text);
        }
        for bad in ["", "Street Brawl", "NORMAL", "a-b"] {
            assert!(GameMode::parse(bad).is_err(), "{bad:?}");
        }
    }

    fn m(id: u64, started: &str) -> MatchRow {
        MatchRow {
            match_id: id,
            started_at: started.into(),
            duration_s: Some(1800),
            game_mode: GameMode::Normal,
            match_mode: Some("unranked".into()),
            patch: "1.0".into(),
            user_team: 1,
            user_hero_id: 15,
            result: Some(MatchResult::Win),
            data_source: DataSource::Broadcast,
            recording_path: None,
        }
    }

    fn mp(acc: u32, team: u8) -> MatchPlayerRow {
        MatchPlayerRow {
            account_id: acc,
            team,
            hero_id: 1,
            assigned_lane: None,
            party_id: Some(3),
        }
    }

    fn buy(match_id: u64, t: u32, item: u32) -> PurchaseEvent {
        PurchaseEvent {
            match_id,
            account_id: 5,
            t_game_s: t,
            item_id: item,
            action: Action::Buy,
            cost: 500,
        }
    }

    #[test]
    fn player_upsert_keeps_name_when_none() {
        let s = Storage::open_in_memory().unwrap();
        s.upsert_player(1, Some("Ann"), "2026-01-01T00:00:00Z")
            .unwrap();
        s.upsert_player(1, None, "2026-02-01T00:00:00Z").unwrap();
        let p = s.get_player(1).unwrap().unwrap();
        assert_eq!(p.display_name.as_deref(), Some("Ann"));
        assert_eq!(p.last_seen_at, "2026-02-01T00:00:00Z");
        s.upsert_player(1, Some("Anna"), "2026-03-01T00:00:00Z")
            .unwrap();
        assert_eq!(
            s.get_player(1).unwrap().unwrap().display_name.as_deref(),
            Some("Anna")
        );
        assert!(s.get_player(2).unwrap().is_none());
    }

    #[test]
    fn match_roundtrip_and_update_keeps_children() {
        let s = Storage::open_in_memory().unwrap();
        let mut a = m(100, "2026-01-01T10:00:00Z");
        s.insert_match(&a, &[mp(1, 0), mp(2, 1)]).unwrap();
        assert_eq!(s.get_match(100).unwrap().unwrap(), a);
        assert_eq!(s.get_match_players(100).unwrap(), vec![mp(1, 0), mp(2, 1)]);
        s.insert_purchase_events(&[buy(100, 10, 7)]).unwrap();
        a.result = Some(MatchResult::Loss);
        a.duration_s = Some(2000);
        s.insert_match(&a, &[mp(1, 0)]).unwrap();
        assert_eq!(s.get_match(100).unwrap().unwrap(), a);
        assert_eq!(s.get_match_players(100).unwrap().len(), 1);
        assert_eq!(
            s.list_purchase_events(100).unwrap().len(),
            1,
            "update must not cascade-delete"
        );
        assert!(s.get_match(101).unwrap().is_none());
    }

    #[test]
    fn other_game_mode_is_stored_and_read_back() {
        let s = Storage::open_in_memory().unwrap();
        let mut row = m(5, "2026-01-01T00:00:00Z");
        row.game_mode = GameMode::Other("hero_labs".into());
        s.insert_match(&row, &[]).unwrap();
        assert_eq!(s.get_match(5).unwrap().unwrap(), row);
    }

    #[test]
    fn matches_list_newest_first() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_match(&m(1, "2026-01-01T10:00:00Z"), &[]).unwrap();
        s.insert_match(&m(3, "2026-01-03T10:00:00Z"), &[]).unwrap();
        s.insert_match(&m(2, "2026-01-02T10:00:00Z"), &[]).unwrap();
        let ids: Vec<_> = s
            .list_matches(None)
            .unwrap()
            .iter()
            .map(|r| r.match_id)
            .collect();
        assert_eq!(ids, vec![3, 2, 1]);
        assert_eq!(s.list_matches(Some(2)).unwrap().len(), 2);
    }

    #[test]
    fn purchases_come_back_in_time_order() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_match(&m(1, "2026-01-01T10:00:00Z"), &[]).unwrap();
        s.insert_purchase_events(&[buy(1, 300, 3), buy(1, 100, 1), buy(1, 100, 2)])
            .unwrap();
        let items: Vec<_> = s
            .list_purchase_events(1)
            .unwrap()
            .iter()
            .map(|e| e.item_id)
            .collect();
        assert_eq!(items, vec![1, 2, 3]);
    }

    #[test]
    fn purchase_for_unknown_match_is_rejected() {
        let s = Storage::open_in_memory().unwrap();
        assert!(s.insert_purchase_events(&[buy(9, 1, 1)]).is_err());
    }

    #[test]
    fn deleting_a_match_cascades() {
        let s = Storage::open_in_memory().unwrap();
        s.insert_match(&m(1, "2026-01-01T10:00:00Z"), &[mp(1, 0)])
            .unwrap();
        s.insert_match(&m(2, "2026-01-02T10:00:00Z"), &[mp(1, 0)])
            .unwrap();
        s.insert_purchase_events(&[buy(1, 1, 1), buy(2, 1, 1)])
            .unwrap();
        s.with_conn(|c| {
            c.execute(
                "INSERT INTO recommendation_history (match_id, t_game_s, phase, items_json, drivers_json)
                 VALUES (1, 5, 'lane', '[]', '[]')",
                [],
            )?;
            c.execute(
                "INSERT INTO reviews (match_id, facts_json, report_json, llm_provider, created_at)
                 VALUES (1, '{}', '{}', 'off', 'x')",
                [],
            )?;
            c.execute(
                "INSERT INTO review_decisions (match_id, code, t_game_s, epl, delta_p, tier, facts_json)
                 VALUES (1, 'D-DEATH', 5, 0.5, -0.1, 'T3', '{}')",
                [],
            )
        })
        .unwrap();
        assert!(s.delete_match(1).unwrap());
        assert!(!s.delete_match(1).unwrap());
        assert!(s.get_match_players(1).unwrap().is_empty());
        assert!(s.list_purchase_events(1).unwrap().is_empty());
        let left: i64 = s
            .with_conn(|c| {
                c.query_row(
                    "SELECT (SELECT COUNT(*) FROM recommendation_history)
                          + (SELECT COUNT(*) FROM reviews)
                          + (SELECT COUNT(*) FROM review_decisions)",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap();
        assert_eq!(left, 0);
        assert_eq!(s.list_purchase_events(2).unwrap().len(), 1);
        assert_eq!(s.get_match_players(2).unwrap().len(), 1);
    }
}
