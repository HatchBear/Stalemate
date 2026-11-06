//! Stats-only Review from API-05 metadata (FR-REV-001, FR-REV-003, FR-REV-004, R-02).
//!
//! Output is numbers, enums and IDs only. The UI renders text from `i18n`. Pure and
//! deterministic: the same input gives the same output.

use crate::api_client::{MatchMetadata, MetaPlayer, MetaStat};
use crate::settings::Rules;
use crate::types::{AccountId, HeroId, MatchId};
use serde::Serialize;

/// The `game_mode` string of a Standard match in API-05 (DEC-019). Anything else is non-Standard.
pub(crate) const STANDARD_GAME_MODE: &str = "Normal";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewResult {
    Win,
    Loss,
    Abandoned,
    /// The API gave no usable winning team.
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct ReviewFlags {
    /// Shorter than `review.min_match_s`.
    pub short_match: bool,
    pub abandoned: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LaneUnavailable {
    NonStandardMode,
    /// The match ended before the lane phase did.
    MatchTooShort,
    NoAssignedLane,
    NoLaneOpponents,
    /// No stat samples around the lane phase end for the user or any opponent.
    NoStats,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct LaneLead {
    pub t_game_s: u32,
    pub assigned_lane: i32,
    pub user_net_worth: i64,
    pub opponent_count: u32,
    pub opponent_mean_net_worth: f64,
    /// User minus the opponents' mean. Negative means behind.
    pub lead: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum LaneSection {
    Available(LaneLead),
    Unavailable { reason: LaneUnavailable },
}

/// R-02: soul deficit against lane opponents at the end of the lane phase (FR-REV-003).
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct R02Finding {
    pub t_game_s: u32,
    pub deficit: f64,
    pub threshold: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SoulSource {
    LaneTroopers,
    NeutralCamps,
    Breakables,
    SinnersSacrifice,
    /// No verified API-05 field (DEC-052). Always `souls: None`.
    Urn,
    /// Every other `gold_source_*_gold`, so the rows add up to the total.
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SoulRow {
    pub source: SoulSource,
    /// `None` is "unavailable", not zero.
    pub souls: Option<u64>,
    /// Percent of `total`, 0 when the total is 0. `None` when unavailable.
    pub share_pct: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SoulSources {
    pub total: u64,
    pub rows: Vec<SoulRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StatsReview {
    pub match_id: MatchId,
    pub result: ReviewResult,
    pub hero_id: HeroId,
    pub duration_s: u32,
    /// The raw API-05 `game_mode`.
    pub game_mode: String,
    /// False means a match record only: no lane, no souls, no findings (DEC-019).
    pub standard_mode: bool,
    pub flags: ReviewFlags,
    /// True when the user has no stat samples, so the stats sections are missing.
    pub partial: bool,
    pub lane: LaneSection,
    pub r02: Option<R02Finding>,
    pub soul_sources: Option<SoulSources>,
}

/// Team text of API-05 (`"Team0"`, `"Team1"`) as 0 or 1.
pub(crate) fn parse_team(s: &str) -> Option<u8> {
    match s {
        "Team0" => Some(0),
        "Team1" => Some(1),
        _ => None,
    }
}

pub(crate) fn find_user(meta: &MatchMetadata, me: AccountId) -> Option<&MetaPlayer> {
    meta.players.iter().find(|p| p.account_id == me)
}

fn sorted(stats: &[MetaStat]) -> Vec<&MetaStat> {
    let mut v: Vec<&MetaStat> = stats.iter().collect();
    v.sort_by_key(|s| s.time_stamp_s);
    v
}

/// Net worth at `t`: the exact sample, or linear interpolation (rounded half away from zero)
/// between the neighbours. `None` when `t` lies outside the samples.
fn net_worth_at(stats: &[MetaStat], t: u32) -> Option<i64> {
    let v = sorted(stats);
    if let Some(s) = v.iter().find(|s| s.time_stamp_s == t) {
        return Some(i64::from(s.net_worth));
    }
    let before = v.iter().rev().find(|s| s.time_stamp_s < t)?;
    let after = v.iter().find(|s| s.time_stamp_s > t)?;
    let (a, b) = (i64::from(before.net_worth), i64::from(after.net_worth));
    let num = (b - a) * i64::from(t - before.time_stamp_s);
    let den = i64::from(after.time_stamp_s - before.time_stamp_s);
    let half = if num < 0 { -den } else { den };
    Some(a + (2 * num + half) / (2 * den))
}

fn lane_section(
    meta: &MatchMetadata,
    user: &MetaPlayer,
    user_team: Option<u8>,
    rules: &Rules,
) -> LaneSection {
    let no = |reason| LaneSection::Unavailable { reason };
    let t = rules.counter.lane_phase_end_s;
    if meta.duration_s < t {
        return no(LaneUnavailable::MatchTooShort);
    }
    let Some(lane) = user.assigned_lane else {
        return no(LaneUnavailable::NoAssignedLane);
    };
    let opponents: Vec<&MetaPlayer> = meta
        .players
        .iter()
        .filter(|p| {
            p.assigned_lane == Some(lane)
                && parse_team(&p.team).is_some()
                && parse_team(&p.team) != user_team
        })
        .collect();
    if opponents.is_empty() {
        return no(LaneUnavailable::NoLaneOpponents);
    }
    let Some(user_nw) = net_worth_at(&user.stats, t) else {
        return no(LaneUnavailable::NoStats);
    };
    let opp: Vec<i64> = opponents
        .iter()
        .filter_map(|p| net_worth_at(&p.stats, t))
        .collect();
    if opp.is_empty() {
        return no(LaneUnavailable::NoStats);
    }
    let mean = opp.iter().sum::<i64>() as f64 / opp.len() as f64;
    LaneSection::Available(LaneLead {
        t_game_s: t,
        assigned_lane: lane,
        user_net_worth: user_nw,
        opponent_count: opp.len() as u32,
        opponent_mean_net_worth: mean,
        lead: user_nw as f64 - mean,
    })
}

fn soul_sources(user: &MetaPlayer) -> Option<SoulSources> {
    let s = sorted(&user.stats).into_iter().next_back()?;
    let g = |v: u32| u64::from(v);
    let lane = g(s.gold_source_lane_creeps_gold);
    let neutrals = g(s.gold_source_neutrals_gold);
    let breakables = g(s.gold_source_breakable_gold);
    let sacrifice = g(s.gold_source_item_cultist_sacrifice_gold);
    let other = [
        s.gold_source_players_gold,
        s.gold_source_assists_gold,
        s.gold_source_denies_gold,
        s.gold_source_bosses_gold,
        s.gold_source_team_bonus_gold,
        s.gold_source_treasure_gold,
        s.gold_source_item_trophy_collector_gold,
        s.gold_source_item_goose_egg_gold,
        s.gold_source_ability_assassinate_gold,
    ]
    .iter()
    .map(|&v| g(v))
    .sum::<u64>();
    let total = lane + neutrals + breakables + sacrifice + other;
    let row = |source, souls: u64| SoulRow {
        source,
        souls: Some(souls),
        share_pct: Some(if total == 0 {
            0.0
        } else {
            souls as f64 * 100.0 / total as f64
        }),
    };
    Some(SoulSources {
        total,
        rows: vec![
            row(SoulSource::LaneTroopers, lane),
            row(SoulSource::NeutralCamps, neutrals),
            row(SoulSource::Breakables, breakables),
            row(SoulSource::SinnersSacrifice, sacrifice),
            SoulRow {
                source: SoulSource::Urn,
                souls: None,
                share_pct: None,
            },
            row(SoulSource::Other, other),
        ],
    })
}

/// Build the stats-only Review for `me` in `meta`. `None` when `me` is not in the match.
pub fn build_stats_review(
    meta: &MatchMetadata,
    me: AccountId,
    rules: &Rules,
) -> Option<StatsReview> {
    let user = find_user(meta, me)?;
    let user_team = parse_team(&user.team);
    let abandoned = user.abandon_match_time_s > 0;
    let result = if abandoned {
        ReviewResult::Abandoned
    } else {
        match (user_team, parse_team(&meta.winning_team)) {
            (Some(u), Some(w)) if u == w => ReviewResult::Win,
            (Some(_), Some(_)) => ReviewResult::Loss,
            _ => ReviewResult::Unknown,
        }
    };
    let standard_mode = meta.game_mode == STANDARD_GAME_MODE;
    let flags = ReviewFlags {
        short_match: meta.duration_s < rules.review.min_match_s,
        abandoned,
    };
    let (lane, soul_sources) = if standard_mode {
        (
            lane_section(meta, user, user_team, rules),
            soul_sources(user),
        )
    } else {
        (
            LaneSection::Unavailable {
                reason: LaneUnavailable::NonStandardMode,
            },
            None,
        )
    };
    let r02 = match &lane {
        LaneSection::Available(l) if -l.lead >= f64::from(rules.review.r02_lane_deficit) => {
            Some(R02Finding {
                t_game_s: l.t_game_s,
                deficit: -l.lead,
                threshold: rules.review.r02_lane_deficit,
            })
        }
        _ => None,
    };
    Some(StatsReview {
        match_id: meta.match_id,
        result,
        hero_id: user.hero_id,
        duration_s: meta.duration_s,
        game_mode: meta.game_mode.clone(),
        standard_mode,
        flags,
        partial: standard_mode && user.stats.is_empty(),
        lane,
        r02,
        soul_sources,
    })
}
