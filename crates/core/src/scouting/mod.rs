//! M-08 Scouting service and Counter-buy filter (PRG-08, PRG-09). See 05-player-scouting.md.
//!
//! This file covers the basic Scouting Card (T-014, FR-SCOUT-001 to 004 and 015). Beta fields
//! (Build Profile, Counter-buy filter, power spikes, playstyle, flags, notes) are not here.
//!
//! # Timing and fair play (FP-10, DEC-047)
//! The service may fetch as soon as the roster is known, so cards are ready when the user needs
//! them. The UI must not show any card until [`scouting_visible`] is true for the current game
//! time. Showing cards in the pre-game swap phase is not allowed.
//!
//! # Opt-outs (NFR-FAIR-004, FP-7)
//! A 404 (`ApiError::NoPublicData`) makes that card `NoPublicData`. The service makes no further
//! call for that player and discards anything already fetched for them. Nothing is recovered
//! from another source.
//!
//! # Constants
//! Windows and thresholds come from `rules.scouting` (DATA-001). The `game_mode` value that means
//! Standard is not in DATA-001 yet, so it lives in [`ScoutingModes`]; see the report for the
//! doc 14 change that would move it into config.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use serde::Serialize;
use tokio::sync::mpsc;
use tokio::task::JoinSet;

use crate::api_client::{AccountRank, ApiClient, ApiError, MatchHistoryEntry, PlayerHeroStats};
use crate::settings::Rules;
use crate::{AccountId, GameSecs, HeroId, RosterEntry};

/// API enum values the card logic needs. Defaults match real responses (`game_mode` 1 is normal).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScoutingModes {
    /// `game_mode` value for Standard matches (DEC-019).
    pub standard_game_mode: i32,
}

impl Default for ScoutingModes {
    fn default() -> Self {
        Self {
            standard_game_mode: 1,
        }
    }
}

/// FP-10: scouting cards may be shown only after the swap phase has closed.
pub fn scouting_visible(t_game: GameSecs, rules: &Rules) -> bool {
    t_game > rules.scouting.swap_closed_after_s
}

/// The three deadlock-api calls scouting needs. `ApiClient` implements it; tests use a mock.
#[async_trait]
pub trait ScoutingSource: Send + Sync {
    async fn match_history(&self, id: AccountId) -> Result<Vec<MatchHistoryEntry>, ApiError>;
    async fn player_ranks(&self, ids: &[AccountId]) -> Result<Vec<AccountRank>, ApiError>;
    async fn player_hero_stats(&self, ids: &[AccountId]) -> Result<Vec<PlayerHeroStats>, ApiError>;
}

#[async_trait]
impl ScoutingSource for ApiClient {
    async fn match_history(&self, id: AccountId) -> Result<Vec<MatchHistoryEntry>, ApiError> {
        ApiClient::match_history(self, id).await
    }
    async fn player_ranks(&self, ids: &[AccountId]) -> Result<Vec<AccountRank>, ApiError> {
        ApiClient::player_ranks(self, ids).await
    }
    async fn player_hero_stats(&self, ids: &[AccountId]) -> Result<Vec<PlayerHeroStats>, ApiError> {
        ApiClient::player_hero_stats(self, ids).await
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind")]
pub enum CardState {
    Loading,
    Ready,
    NoPublicData,
    Error {
        source: String,
        retry_after_s: Option<u64>,
    },
    /// Bot or empty slot (`account_id == 0`): show the hero only.
    HeroOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WlOverall {
    pub wins: u32,
    pub matches: u32,
}

/// Recent Standard-mode W/L. `matches` can be below `window`; the UI then labels it with N.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WlRecent {
    pub wins: u32,
    pub matches: u32,
    pub window: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Main {
    pub hero_id: HeroId,
    pub matches: u32,
    pub wins: u32,
    /// 0.0 to 1.0.
    pub win_rate: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScoutingCard {
    pub account_id: AccountId,
    pub hero_id: HeroId,
    pub team: u8,
    pub name: String,
    pub state: CardState,
    pub wl_overall: Option<WlOverall>,
    pub wl_recent: Option<WlRecent>,
    pub rank_badge: Option<u32>,
    /// Rank progress points gained per 10 ranked matches (DEC-051).
    pub mmr_trend_per_10: Option<f64>,
    pub mains: Vec<Main>,
    /// Lifetime matches on the roster hero.
    pub hero_matches: Option<u32>,
}

impl ScoutingCard {
    fn new(e: &RosterEntry, state: CardState) -> Self {
        Self {
            account_id: e.account_id,
            hero_id: e.hero_id,
            team: e.team,
            name: e.name.clone(),
            state,
            wl_overall: None,
            wl_recent: None,
            rank_badge: None,
            mmr_trend_per_10: None,
            mains: Vec::new(),
            hero_matches: None,
        }
    }
}

/// Least-squares slope of `series` against its index, times 10 (change per 10 matches).
/// Fewer than 2 points gives `None` (TC-08-3).
pub fn slope_per_10(series: &[f64]) -> Option<f64> {
    let n = series.len();
    if n < 2 {
        return None;
    }
    let nf = n as f64;
    let mean_x = (nf - 1.0) / 2.0;
    let mean_y = series.iter().sum::<f64>() / nf;
    let (mut num, mut den) = (0.0, 0.0);
    for (i, y) in series.iter().enumerate() {
        let dx = i as f64 - mean_x;
        num += dx * (y - mean_y);
        den += dx * dx;
    }
    Some(num / den * 10.0)
}

/// DEC-051: the most recent `mmr_window` ranked matches with a `ranked_delta`, oldest first,
/// as a cumulative series starting at 0, then the slope per 10 matches. Fewer than `min_points`
/// ranked matches in the window means "not enough data", not a trend of 0 (DEC-061, DATA-001
/// `[scouting] mmr_trend_min_points`).
pub fn mmr_trend(history: &[MatchHistoryEntry], mmr_window: u32, min_points: u32) -> Option<f64> {
    let mut ranked: Vec<&MatchHistoryEntry> = history
        .iter()
        .filter(|m| m.ranked_delta.is_some())
        .collect();
    ranked.sort_by_key(|m| std::cmp::Reverse(m.start_time));
    ranked.truncate(mmr_window as usize);
    if ranked.len() < min_points as usize {
        return None;
    }
    let mut total = 0.0;
    let mut series = vec![0.0];
    for m in ranked.iter().rev() {
        total += f64::from(m.ranked_delta.unwrap_or(0));
        series.push(total);
    }
    slope_per_10(&series)
}

/// W/L over the most recent `window` Standard-mode matches (DEC-019).
pub fn recent_wl(history: &[MatchHistoryEntry], window: u32, modes: &ScoutingModes) -> WlRecent {
    let mut std: Vec<&MatchHistoryEntry> = history
        .iter()
        .filter(|m| m.game_mode == modes.standard_game_mode)
        .collect();
    std.sort_by_key(|m| std::cmp::Reverse(m.start_time));
    std.truncate(window as usize);
    WlRecent {
        wins: std.iter().filter(|m| m.won()).count() as u32,
        matches: std.len() as u32,
        window,
    }
}

/// Which data parts each card waits for: hero stats, ranks, history.
const PARTS: u8 = 3;

struct Slot {
    card: ScoutingCard,
    left: u8,
}

type Shared = Arc<Mutex<HashMap<AccountId, Slot>>>;

/// Applies `f` to the card unless it is already `NoPublicData`, finishes it when every part is
/// in, then sends a snapshot.
fn update(
    shared: &Shared,
    tx: &mpsc::UnboundedSender<ScoutingCard>,
    id: AccountId,
    f: impl FnOnce(&mut ScoutingCard),
) {
    let snapshot = {
        let mut map = shared.lock().unwrap();
        let Some(slot) = map.get_mut(&id) else { return };
        if slot.card.state == CardState::NoPublicData {
            return;
        }
        f(&mut slot.card);
        slot.left = slot.left.saturating_sub(1);
        if slot.left == 0 && slot.card.state == CardState::Loading {
            slot.card.state = CardState::Ready;
        }
        slot.card.clone()
    };
    let _ = tx.send(snapshot);
}

/// Marks a card failed. `NoPublicData` also clears everything fetched for that player.
fn fail(
    shared: &Shared,
    tx: &mpsc::UnboundedSender<ScoutingCard>,
    id: AccountId,
    err: &ApiError,
    source: &str,
) {
    update(shared, tx, id, |c| match err {
        ApiError::NoPublicData => {
            let fresh = ScoutingCard::new(
                &RosterEntry {
                    account_id: c.account_id,
                    name: c.name.clone(),
                    team: c.team,
                    hero_id: c.hero_id,
                },
                CardState::NoPublicData,
            );
            *c = fresh;
        }
        ApiError::Busy { retry_after_s, .. } => {
            c.state = CardState::Error {
                source: source.to_string(),
                retry_after_s: Some(*retry_after_s),
            };
        }
        _ => {
            c.state = CardState::Error {
                source: source.to_string(),
                retry_after_s: None,
            };
        }
    });
}

pub struct ScoutingService<S: ScoutingSource + 'static> {
    source: Arc<S>,
    modes: ScoutingModes,
}

impl<S: ScoutingSource + 'static> ScoutingService<S> {
    pub fn new(source: Arc<S>) -> Self {
        Self {
            source,
            modes: ScoutingModes::default(),
        }
    }

    pub fn with_modes(mut self, modes: ScoutingModes) -> Self {
        self.modes = modes;
        self
    }

    /// Scouts every roster player except `me` (an account ID; the user is not scouted).
    ///
    /// Every card is sent once at the start (`Loading`, or `HeroOnly` for `account_id == 0`),
    /// then again each time it gains data. The future ends when every call has finished. The
    /// UI must hold the cards back until [`scouting_visible`] (FP-10).
    pub async fn scout(
        &self,
        roster: &[RosterEntry],
        me: AccountId,
        rules: &Rules,
        tx: mpsc::UnboundedSender<ScoutingCard>,
    ) {
        let shared: Shared = Arc::new(Mutex::new(HashMap::new()));
        let mut ids: Vec<AccountId> = Vec::new();
        let mut seen = HashSet::new();
        for e in roster {
            if e.account_id != 0 && e.account_id == me {
                continue;
            }
            if e.account_id == 0 {
                let _ = tx.send(ScoutingCard::new(e, CardState::HeroOnly));
                continue;
            }
            if !seen.insert(e.account_id) {
                continue;
            }
            let card = ScoutingCard::new(e, CardState::Loading);
            let _ = tx.send(card.clone());
            shared
                .lock()
                .unwrap()
                .insert(e.account_id, Slot { card, left: PARTS });
            ids.push(e.account_id);
        }
        if ids.is_empty() {
            return;
        }

        let history_window = rules.scouting.history_window;
        let mmr_window = rules.scouting.mmr_window;
        let mmr_min_points = rules.scouting.mmr_trend_min_points;
        let mains_shown = rules.scouting.mains_shown as usize;
        let modes = self.modes;
        let mut set = JoinSet::new();

        {
            let (src, shared, tx, ids) =
                (self.source.clone(), shared.clone(), tx.clone(), ids.clone());
            set.spawn(async move {
                match src.player_hero_stats(&ids).await {
                    Ok(rows) => {
                        for id in &ids {
                            let mine: Vec<&PlayerHeroStats> =
                                rows.iter().filter(|r| r.account_id == *id).collect();
                            update(&shared, &tx, *id, |c| {
                                apply_hero_stats(c, &mine, mains_shown)
                            });
                        }
                    }
                    Err(e) => {
                        for id in &ids {
                            // A batch 404 cannot be pinned on one player, so it is a plain error.
                            let e = match &e {
                                ApiError::NoPublicData => ApiError::Http(404),
                                other => clone_err(other),
                            };
                            fail(&shared, &tx, *id, &e, "hero-stats");
                        }
                    }
                }
            });
        }
        {
            let (src, shared, tx, ids) =
                (self.source.clone(), shared.clone(), tx.clone(), ids.clone());
            set.spawn(async move {
                match src.player_ranks(&ids).await {
                    Ok(rows) => {
                        for id in &ids {
                            let badge = rows
                                .iter()
                                .find(|r| r.account_id == *id)
                                .map(|r| r.badge)
                                .filter(|b| *b != 0);
                            update(&shared, &tx, *id, |c| c.rank_badge = badge);
                        }
                    }
                    Err(e) => {
                        for id in &ids {
                            let e = match &e {
                                ApiError::NoPublicData => ApiError::Http(404),
                                other => clone_err(other),
                            };
                            fail(&shared, &tx, *id, &e, "rank");
                        }
                    }
                }
            });
        }
        for id in ids {
            let (src, shared, tx) = (self.source.clone(), shared.clone(), tx.clone());
            set.spawn(async move {
                match src.match_history(id).await {
                    Ok(h) => update(&shared, &tx, id, |c| {
                        c.wl_recent = Some(recent_wl(&h, history_window, &modes));
                        c.mmr_trend_per_10 = mmr_trend(&h, mmr_window, mmr_min_points);
                    }),
                    Err(e) => fail(&shared, &tx, id, &e, "match-history"),
                }
            });
        }
        while set.join_next().await.is_some() {}
    }
}

/// `ApiError` is not `Clone` (it wraps reqwest errors); keep only what the card shows.
fn clone_err(e: &ApiError) -> ApiError {
    match e {
        ApiError::Busy {
            retry_after_s,
            timeout,
        } => ApiError::Busy {
            retry_after_s: *retry_after_s,
            timeout: *timeout,
        },
        ApiError::Http(c) => ApiError::Http(*c),
        _ => ApiError::Http(0),
    }
}

fn apply_hero_stats(c: &mut ScoutingCard, rows: &[&PlayerHeroStats], mains_shown: usize) {
    let matches: u32 = rows.iter().map(|r| r.matches_played).sum();
    let wins: u32 = rows.iter().map(|r| r.wins).sum();
    c.wl_overall = Some(WlOverall { wins, matches });
    c.hero_matches = Some(
        rows.iter()
            .filter(|r| r.hero_id == c.hero_id)
            .map(|r| r.matches_played)
            .sum(),
    );
    let mut mains: Vec<Main> = rows
        .iter()
        .filter(|r| r.matches_played > 0)
        .map(|r| Main {
            hero_id: r.hero_id,
            matches: r.matches_played,
            wins: r.wins,
            win_rate: f64::from(r.wins) / f64::from(r.matches_played),
        })
        .collect();
    mains.sort_by(|a, b| b.matches.cmp(&a.matches).then(a.hero_id.cmp(&b.hero_id)));
    mains.truncate(mains_shown);
    c.mains = mains;
}

#[cfg(test)]
mod tests;
