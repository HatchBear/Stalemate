//! Storage of the stats-only Review (FR-REV-001). Re-running the same match replaces its rows.

use super::stats::{find_user, parse_team, ReviewResult, StatsReview};
use crate::api_client::MatchMetadata;
use crate::storage::{
    DataSource, GameMode, MatchPlayerRow, MatchResult, MatchRow, Storage, StorageError,
};
use crate::types::AccountId;
use rusqlite::params;

#[derive(Debug, thiserror::Error)]
pub enum ReviewError {
    #[error("the user is not a player in this match")]
    UserNotInMatch,
    #[error("unknown team value: {0:?}")]
    BadTeam(String),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("could not serialize the review: {0}")]
    Json(#[from] serde_json::Error),
}

/// API-05 `"YYYY-MM-DD HH:MM:SS"` (UTC) as RFC 3339. Other text is kept as it came.
fn started_at(s: &str) -> String {
    match s.split_once(' ') {
        Some((d, t)) if !s.contains('T') => format!("{d}T{t}Z"),
        _ => s.to_string(),
    }
}

/// `StreetBrawl` -> `street_brawl`, `Normal` -> `normal`, `SomeNewMode` -> `some_new_mode`.
fn snake_case_mode(s: &str) -> String {
    let mut out = String::new();
    for (i, ch) in s.trim().chars().enumerate() {
        if ch.is_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.extend(ch.to_lowercase());
        } else if ch == ' ' || ch == '-' {
            if !out.ends_with('_') {
                out.push('_');
            }
        } else {
            out.push(ch);
        }
    }
    out
}

/// Write the `matches` and `match_players` rows and, for Standard matches, the `reviews` row and
/// the R-02 finding. Safe to call again for the same match: nothing is duplicated.
/// `patch` is the game patch label the caller knows for this match.
pub fn persist_stats_review(
    storage: &Storage,
    meta: &MatchMetadata,
    me: AccountId,
    review: &StatsReview,
    patch: &str,
) -> Result<(), ReviewError> {
    let user = find_user(meta, me).ok_or(ReviewError::UserNotInMatch)?;
    let team = |s: &str| parse_team(s).ok_or_else(|| ReviewError::BadTeam(s.to_string()));
    let players = meta
        .players
        .iter()
        .map(|p| {
            Ok(MatchPlayerRow {
                account_id: p.account_id,
                team: team(&p.team)?,
                hero_id: p.hero_id,
                assigned_lane: p.assigned_lane.and_then(|l| u8::try_from(l).ok()),
                party_id: None, // API-05 has no party field
            })
        })
        .collect::<Result<Vec<_>, ReviewError>>()?;
    let row = MatchRow {
        match_id: meta.match_id,
        started_at: started_at(&meta.start_time),
        duration_s: Some(meta.duration_s),
        // Placeholder for the typed column; the API's real mode is written right after the insert.
        game_mode: GameMode::Normal,
        match_mode: Some(meta.match_mode.to_lowercase()).filter(|s| !s.is_empty()),
        patch: patch.to_string(),
        user_team: team(&user.team)?,
        user_hero_id: user.hero_id,
        result: match review.result {
            ReviewResult::Win => Some(MatchResult::Win),
            ReviewResult::Loss => Some(MatchResult::Loss),
            ReviewResult::Abandoned => Some(MatchResult::Abandoned),
            ReviewResult::Unknown => None,
        },
        data_source: DataSource::Api,
        recording_path: None,
    };
    storage.insert_match(&row, &players)?;
    // `GameMode` only knows two modes, but the column is free text: store what the API said.
    let mode = snake_case_mode(&meta.game_mode);
    let mid = meta.match_id as i64;
    storage.with_conn(|c| {
        c.execute(
            "UPDATE matches SET game_mode = ?1 WHERE match_id = ?2",
            params![mode, mid],
        )
    })?;
    if !review.standard_mode {
        // A non-Standard match may have a review from an earlier run; none is kept now.
        let id = meta.match_id as i64;
        storage.with_conn(|c| c.execute("DELETE FROM reviews WHERE match_id = ?1", [id]))?;
        return Ok(());
    }

    let json = serde_json::to_string(review)?;
    let finding = review
        .r02
        .as_ref()
        .map(|f| serde_json::to_string(f).map(|j| (f.t_game_s, j)))
        .transpose()?;
    let created = crate::storage::now_rfc3339()?;
    let id = meta.match_id as i64;
    let partial = review.partial;
    storage.with_conn(|c| {
        let tx = c.transaction()?;
        tx.execute(
            "INSERT INTO reviews (match_id, accuracy, accuracy_lane, accuracy_mid, accuracy_late,
               tier_counts, facts_json, report_json, model_version, llm_provider, partial, created_at)
             VALUES (?1, NULL, NULL, NULL, NULL, NULL, ?2, ?2, NULL, 'off', ?3, ?4)
             ON CONFLICT(match_id) DO UPDATE SET
               accuracy = NULL, accuracy_lane = NULL, accuracy_mid = NULL, accuracy_late = NULL,
               tier_counts = NULL, facts_json = excluded.facts_json,
               report_json = excluded.report_json, model_version = NULL, llm_provider = 'off',
               partial = excluded.partial, created_at = excluded.created_at",
            params![id, json, partial, created],
        )?;
        tx.execute("DELETE FROM review_findings WHERE match_id = ?1", [id])?;
        if let Some((t, payload)) = &finding {
            tx.execute(
                "INSERT INTO review_findings (match_id, rule_id, t_game_s, payload_json)
                 VALUES (?1, 'R-02', ?2, ?3)",
                params![id, t, payload],
            )?;
        }
        tx.commit()
    })?;
    Ok(())
}
