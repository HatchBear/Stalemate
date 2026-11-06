//! Typed deadlock-api responses, from the live OpenAPI spec and real responses checked on 2026-11-05.
//!
//! Required spec fields are plain. Fields the spec marks optional or nullable use `Option` or
//! `#[serde(default)]`. Unknown fields are ignored, so new fields upstream never break decoding.
//! Only fields Stalemate reads are listed. Personal fields we don't need (Steam real name, friend
//! lists) are deliberately left out.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::types::{AccountId, HeroId, MatchId};

/// API-01 `GET /v1/players/{account_id}/match-history`.
///
/// `match_result` is the winning team: the player won when it equals `player_team`.
/// `player_match_outcome` is 1 for a win and 2 for a loss; other values (0, 5) have been seen and
/// are not mapped. `game_mode` 1 is normal and `match_mode` 1 is unranked in real responses.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MatchHistoryEntry {
    pub account_id: AccountId,
    pub match_id: MatchId,
    pub hero_id: HeroId,
    pub hero_level: u32,
    /// Unix seconds.
    pub start_time: u64,
    pub match_duration_s: u32,
    pub match_result: i32,
    pub player_team: i32,
    pub player_match_outcome: i32,
    pub game_mode: i32,
    pub match_mode: i32,
    pub player_kills: u32,
    pub player_deaths: u32,
    pub player_assists: u32,
    pub last_hits: u32,
    pub denies: u32,
    pub net_worth: u32,
    pub objectives_mask_team0: u64,
    pub objectives_mask_team1: u64,
    #[serde(default)]
    pub abandoned_time_s: Option<u32>,
    #[serde(default)]
    pub low_pri_pool: Option<bool>,
    #[serde(default)]
    pub ranked_delta: Option<i32>,
    #[serde(default)]
    pub ranked_display_badge: Option<u32>,
    #[serde(default)]
    pub ranked_calibration_match: Option<i32>,
    #[serde(default)]
    pub ranked_used_demotion_protection: Option<bool>,
}

impl MatchHistoryEntry {
    pub fn won(&self) -> bool {
        self.match_result == self.player_team
    }
}

/// API-02 `GET /v1/players/rank` (batch). `badge = tier * 10 + subrank`, 0 when unknown.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct AccountRank {
    pub account_id: AccountId,
    pub badge: u32,
    pub rank: u32,
    pub subrank: u32,
    #[serde(default)]
    pub last_match: Option<LastRankedMatch>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LastRankedMatch {
    pub match_id: MatchId,
    /// Unix seconds or an RFC 3339 string, whichever the API sends; kept as raw JSON.
    #[serde(default)]
    pub start_time: serde_json::Value,
    #[serde(default)]
    pub player_rank_final_flat_progress: Option<i32>,
}

/// API-03 `GET /v1/players/hero-stats` (batch).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct PlayerHeroStats {
    pub account_id: AccountId,
    pub hero_id: HeroId,
    #[serde(default)]
    pub matches_played: u32,
    #[serde(default)]
    pub wins: u32,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    /// Unix seconds.
    #[serde(default)]
    pub last_played: u64,
    #[serde(default)]
    pub time_played: u64,
    #[serde(default)]
    pub ending_level: f64,
    #[serde(default)]
    pub networth_per_min: f64,
    #[serde(default)]
    pub creeps_per_min: f64,
    #[serde(default)]
    pub deaths_per_min: f64,
    #[serde(default)]
    pub accuracy: f64,
    #[serde(default)]
    pub crit_shot_rate: f64,
    #[serde(default)]
    pub damage_per_min: f64,
    #[serde(default)]
    pub obj_damage_per_min: f64,
    /// Match IDs on this hero.
    #[serde(default)]
    pub matches: Vec<MatchId>,
}

/// API-04 `GET /v1/players/steam` (batch, read only).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SteamProfile {
    pub account_id: u64,
    pub personaname: String,
    #[serde(default)]
    pub avatar: String,
    #[serde(default)]
    pub avatarmedium: String,
    #[serde(default)]
    pub avatarfull: String,
    #[serde(default)]
    pub profileurl: String,
    #[serde(default)]
    pub countrycode: Option<String>,
    #[serde(default)]
    pub last_team_avg_badge: Option<i32>,
    #[serde(default)]
    pub matches_played_last_30d: u32,
    #[serde(default)]
    pub last_updated: String,
}

/// API-06 `GET /v1/analytics/hero-build-stats/{hero_id}`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct HeroBuildStats {
    pub hero_build_id: u64,
    pub hero_id: HeroId,
    pub matches: u64,
    pub wins: u64,
    pub losses: u64,
    pub players: u64,
    #[serde(default)]
    pub bucket: i64,
    #[serde(default)]
    pub avg_average_badge: Option<f64>,
    #[serde(default)]
    pub total_kills: Option<u64>,
    #[serde(default)]
    pub total_deaths: Option<u64>,
    #[serde(default)]
    pub total_assists: Option<u64>,
    #[serde(default)]
    pub total_duration_s: Option<u64>,
    #[serde(default)]
    pub total_net_worth: Option<u64>,
}

// ---------------------------------------------------------------------------------------------
// API-05 bulk match metadata (OQ-05, checked against real responses on 2026-11-05)
// ---------------------------------------------------------------------------------------------

/// One match from `GET /v1/matches/metadata`. The spec types the items as bare objects, so this
/// follows real responses. Enums arrive as strings here (`"Team0"`, `"TeamWin"`, `"Unranked"`),
/// unlike the integers of API-01 and the single-match endpoint.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MatchMetadata {
    pub match_id: MatchId,
    /// `"YYYY-MM-DD HH:MM:SS"`, UTC.
    #[serde(default)]
    pub start_time: String,
    #[serde(default)]
    pub duration_s: u32,
    /// `"Team0"` or `"Team1"`.
    #[serde(default)]
    pub winning_team: String,
    #[serde(default)]
    pub match_outcome: String,
    /// `"Unranked"`, `"Ranked"`, ...
    #[serde(default)]
    pub match_mode: String,
    /// `"Normal"`, `"StreetBrawl"`, ...
    #[serde(default)]
    pub game_mode: String,
    #[serde(default)]
    pub average_badge_team0: Option<u32>,
    #[serde(default)]
    pub average_badge_team1: Option<u32>,
    #[serde(default)]
    pub objectives: Vec<MetaObjective>,
    #[serde(default)]
    pub mid_boss: Vec<serde_json::Value>,
    #[serde(default)]
    pub players: Vec<MetaPlayer>,
    #[serde(default)]
    pub banned_hero_ids: Vec<HeroId>,
}

/// One destroyed or damaged objective. `team` is the team that owns it. `team_objective` is a name
/// such as `Tier1Lane1`, `Tier2Lane3`, `BarrackBossLane4`, `Core`, `Titan`, `TitanShieldGenerator1`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MetaObjective {
    #[serde(default)]
    pub team_objective: String,
    #[serde(default)]
    pub team: String,
    /// 0 when the objective was never destroyed.
    #[serde(default)]
    pub destroyed_time_s: u32,
    #[serde(default)]
    pub first_damage_time_s: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MetaPlayer {
    pub account_id: AccountId,
    pub hero_id: HeroId,
    #[serde(default)]
    pub player_slot: u32,
    /// `"Team0"` or `"Team1"`.
    #[serde(default)]
    pub team: String,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub net_worth: u32,
    #[serde(default)]
    pub last_hits: u32,
    #[serde(default)]
    pub denies: u32,
    /// The lane the game assigned at the start (1, 4 or 6 in real data). Not the lane played.
    #[serde(default)]
    pub assigned_lane: Option<i32>,
    #[serde(default)]
    pub player_level: u32,
    #[serde(default)]
    pub abandon_match_time_s: u32,
    #[serde(default)]
    pub player_match_outcome: String,
    /// Absent unless demo analysis found it.
    #[serde(default)]
    pub hero_build_id: Option<u64>,
    /// Absent unless demo analysis found it.
    #[serde(default)]
    pub pregame_hero_id: Option<HeroId>,
    #[serde(default)]
    pub items: Vec<MetaItem>,
    /// Cumulative samples every 180 s of game time, plus a final sample at the match end.
    #[serde(default)]
    pub stats: Vec<MetaStat>,
    #[serde(default)]
    pub death_details: Vec<MetaDeath>,
}

/// A purchase. `sold_time_s` is 0 when never sold. `upgrade_id` is the item this was upgraded into
/// (0 when none). IDs are the API-15 item `id`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MetaItem {
    pub item_id: u32,
    #[serde(default)]
    pub game_time_s: u32,
    #[serde(default)]
    pub sold_time_s: u32,
    #[serde(default)]
    pub upgrade_id: u32,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub net_worth_at_buy: Option<u32>,
}

/// One cumulative sample. The `gold_source_*_gold` fields are souls by source:
/// lane troopers `gold_source_lane_creeps_gold`, neutral camps `gold_source_neutrals_gold`,
/// breakables `gold_source_breakable_gold`, Sinner's Sacrifice `gold_source_item_cultist_sacrifice_gold`
/// (the internal name is "cultist"). No field is named for the Urn: `gold_source_treasure_gold`
/// is the likely one, unverified.
#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct MetaStat {
    #[serde(default)]
    pub time_stamp_s: u32,
    #[serde(default)]
    pub net_worth: u32,
    #[serde(default)]
    pub level: u32,
    #[serde(default)]
    pub kills: u32,
    #[serde(default)]
    pub deaths: u32,
    #[serde(default)]
    pub assists: u32,
    #[serde(default)]
    pub creep_kills: u32,
    #[serde(default)]
    pub neutral_kills: u32,
    #[serde(default)]
    pub denies: u32,
    #[serde(default)]
    pub player_damage: u32,
    #[serde(default)]
    pub shots_hit: u32,
    #[serde(default)]
    pub shots_missed: u32,
    #[serde(default)]
    pub hero_bullets_hit: u32,
    #[serde(default)]
    pub hero_bullets_hit_crit: u32,
    #[serde(default)]
    pub gold_player: u32,
    #[serde(default)]
    pub gold_lane_creep: u32,
    #[serde(default)]
    pub gold_neutral_creep: u32,
    #[serde(default)]
    pub gold_boss: u32,
    #[serde(default)]
    pub gold_treasure: u32,
    #[serde(default)]
    pub gold_denied: u32,
    #[serde(default)]
    pub gold_death_loss: u32,
    #[serde(default)]
    pub gold_source_players_gold: u32,
    #[serde(default)]
    pub gold_source_lane_creeps_gold: u32,
    #[serde(default)]
    pub gold_source_neutrals_gold: u32,
    #[serde(default)]
    pub gold_source_bosses_gold: u32,
    #[serde(default)]
    pub gold_source_breakable_gold: u32,
    #[serde(default)]
    pub gold_source_treasure_gold: u32,
    #[serde(default)]
    pub gold_source_assists_gold: u32,
    #[serde(default)]
    pub gold_source_denies_gold: u32,
    #[serde(default)]
    pub gold_source_team_bonus_gold: u32,
    #[serde(default)]
    pub gold_source_item_cultist_sacrifice_gold: u32,
    #[serde(default)]
    pub gold_source_item_trophy_collector_gold: u32,
    #[serde(default)]
    pub gold_source_item_goose_egg_gold: u32,
    #[serde(default)]
    pub gold_source_ability_assassinate_gold: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MetaDeath {
    #[serde(default)]
    pub game_time_s: u32,
    #[serde(default)]
    pub killer_player_slot: u32,
    #[serde(default)]
    pub death_duration_s: u32,
}

/// Which parts of the bulk response to ask for. Each costs response size, so ask only for what is
/// used. The default is what the stats-only Review and the Build Profile need.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MetadataOptions {
    pub info: bool,
    pub objectives: bool,
    pub mid_boss: bool,
    pub player_info: bool,
    pub player_items: bool,
    pub player_stats: bool,
    pub player_death_details: bool,
}

impl Default for MetadataOptions {
    fn default() -> Self {
        Self {
            info: true,
            objectives: true,
            mid_boss: false,
            player_info: true,
            player_items: true,
            player_stats: true,
            player_death_details: false,
        }
    }
}

impl MetadataOptions {
    pub(crate) fn query(&self) -> Vec<(&'static str, String)> {
        [
            ("include_info", self.info),
            ("include_objectives", self.objectives),
            ("include_mid_boss", self.mid_boss),
            ("include_player_info", self.player_info),
            ("include_player_items", self.player_items),
            ("include_player_stats", self.player_stats),
            ("include_player_death_details", self.player_death_details),
        ]
        .into_iter()
        .map(|(k, v)| (k, v.to_string()))
        .collect()
    }

    /// Stable text for cache keys: the same options always give the same text.
    pub(crate) fn fingerprint(&self) -> String {
        self.query()
            .iter()
            .map(|(_, v)| if v == "true" { '1' } else { '0' })
            .collect()
    }
}

/// API-05 single `GET /v1/matches/{id}/metadata`. `match_info` is the raw protobuf-shaped JSON
/// (integers for enums, `team` 0/1). Prefer the bulk endpoint; this one exists for the maps.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct SingleMatchMetadata {
    pub match_info: serde_json::Value,
    /// `account_id` (as a string key) to `hero_build_id`. Empty when demo analysis has nothing.
    #[serde(default)]
    pub hero_build_ids: HashMap<String, u64>,
    /// `account_id` (as a string key) to the hero locked before the swap window.
    #[serde(default)]
    pub pregame_hero_ids: HashMap<String, HeroId>,
    #[serde(default)]
    pub banned_hero_ids: Vec<HeroId>,
}

// ---------------------------------------------------------------------------------------------
// API-16 community builds
// ---------------------------------------------------------------------------------------------

/// One entry of `GET /v1/builds`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommunityBuild {
    pub hero_build: CommunityHeroBuild,
    #[serde(default)]
    pub num_favorites: Option<u32>,
    #[serde(default)]
    pub num_weekly_favorites: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommunityHeroBuild {
    pub hero_id: HeroId,
    pub hero_build_id: u32,
    pub name: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author_account_id: u32,
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub origin_build_id: u32,
    #[serde(default)]
    pub last_updated_timestamp: Option<i64>,
    pub details: CommunityBuildDetails,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct CommunityBuildDetails {
    #[serde(default)]
    pub mod_categories: Vec<CommunityModCategory>,
    #[serde(default)]
    pub ability_order: Option<CommunityAbilityOrder>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommunityModCategory {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub mods: Vec<CommunityMod>,
}

/// `ability_id` is the API-15 item `id` for shop items.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommunityMod {
    pub ability_id: u32,
}

#[derive(Debug, Clone, PartialEq, Default, Deserialize, Serialize)]
pub struct CommunityAbilityOrder {
    #[serde(default)]
    pub currency_changes: Vec<CommunityCurrencyChange>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct CommunityCurrencyChange {
    pub ability_id: u32,
    #[serde(default)]
    pub currency_type: i32,
    #[serde(default)]
    pub delta: i32,
}

/// One entry of `GET /v1/matches/live/urls`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct LiveUrl {
    pub match_id: MatchId,
    pub broadcast_url: String,
    #[serde(default)]
    pub lobby_id: Option<u64>,
    #[serde(default)]
    pub updated_at: Option<i64>,
}

/// `GET /v1/matches/{match_id}/live/url`.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct MatchSpectate {
    pub broadcast_url: String,
    #[serde(default)]
    pub lobby_id: Option<u64>,
}
