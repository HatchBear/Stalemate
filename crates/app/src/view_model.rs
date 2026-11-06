//! Plain view models for the main window (PRG-20, FR-UI-001).
//!
//! These are ordinary Rust structs with no Slint types and no display strings. The core fills
//! them; `ui_map` turns them into Slint structs and formats text through Fluent. Hero and item
//! names and icons arrive as data (NFR-MAINT-001). Nothing here is a list of heroes or items.
//!
//! Live-Safe rule (NFR-FAIR-003): `LiveVm` is built only from `live_safe` output. KDA and level
//! are `Option`: they stay `None` until the author verifies them in-game (T-007).

use std::path::PathBuf;

pub type DurationSecs = u32;

/// What a view can be in (doc 09, Data states). Offline is global and lives in `AppVm`.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum DataState {
    #[default]
    Loaded,
    Loading,
    /// No public data, or a new account with nothing yet.
    Empty,
    /// Broadcast delay over the limit, or cached data older than its TTL.
    Stale {
        age_secs: u64,
    },
    /// The source failed after retries.
    Error {
        source: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CheckState {
    #[default]
    Pending,
    Pass,
    Fail,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CheckId {
    Steam,
    Condebug,
    AccountId,
    Broadcast,
    DeadlockApi,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupCheck {
    pub id: CheckId,
    pub state: CheckState,
    /// A fact to show next to a passing check, such as the Account ID. Not a sentence.
    pub value: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MatchSummary {
    pub won: bool,
    pub hero_name: String,
    pub duration_secs: DurationSecs,
    pub ended_ago_secs: Option<u64>,
}

#[derive(Clone, Debug, Default)]
pub struct HomeVm {
    pub checks: Vec<SetupCheck>,
    pub last_match: Option<MatchSummary>,
    /// Result of the last "Check again" in the first-run setup step.
    pub setup_result: CheckState,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Side {
    #[default]
    Amber,
    Sapphire,
}

impl Side {
    pub fn other(self) -> Side {
        match self {
            Side::Amber => Side::Sapphire,
            Side::Sapphire => Side::Amber,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Record {
    pub wins: u32,
    pub losses: u32,
}

impl Record {
    pub fn matches(self) -> u32 {
        self.wins + self.losses
    }

    /// Whole-number win rate, rounded. `None` when there are no matches.
    pub fn win_rate_pct(self) -> Option<u32> {
        let n = self.matches();
        (n > 0).then(|| (self.wins * 100 + n / 2) / n)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MainHero {
    pub hero_name: String,
    pub matches: u32,
    pub wins: u32,
}

/// Alpha fields only: FR-SCOUT-001 to 004.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutStats {
    pub overall: Record,
    /// Wins and losses over the last matches, up to 20. Fewer when the player has fewer.
    pub last: Record,
    /// The size the "last" window aims for. From config (20).
    pub last_window: u32,
    pub rank: Option<String>,
    /// MMR change per 10 matches. `None` shows "Not available yet": the source is an open question.
    pub mmr_trend_per_10: Option<i32>,
    /// Top 5 by matches.
    pub mains: Vec<MainHero>,
    pub matches_on_hero: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CardBody {
    Stats(ScoutStats),
    /// The player opted out of deadlock-api (NFR-FAIR-004).
    NoPublicData,
    Loading,
    /// A bot or an empty slot: the hero only.
    HeroOnly,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScoutCardVm {
    pub hero_name: String,
    pub player_name: String,
    pub is_you: bool,
    pub body: CardBody,
}

#[derive(Clone, Debug, Default)]
pub struct ScoutingVm {
    /// FP-10: false until the Broadcast shows the game has started.
    pub visible: bool,
    pub state: DataState,
    pub your_side: Side,
    /// Your team, including you if the roster has you here. Up to 6 cards in total with the enemy's.
    pub your_team: Vec<ScoutCardVm>,
    pub enemy_team: Vec<ScoutCardVm>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ItemVm {
    pub name: String,
    /// Runtime-loaded art path (API-15). `None` until the asset cache has it.
    pub icon: Option<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kda {
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EnemyRowVm {
    pub hero_name: String,
    pub player_name: String,
    pub items: Vec<ItemVm>,
    /// Blocked until verified (T-007): `None` for every row hides the column.
    pub kda: Option<Kda>,
    pub level: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PurchaseAction {
    Buy,
    Sell,
    Upgrade,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PurchaseEntryVm {
    pub t_game_secs: DurationSecs,
    pub player_name: String,
    pub item_name: String,
    pub action: PurchaseAction,
}

#[derive(Clone, Debug, Default)]
pub struct LiveVm {
    pub state: DataState,
    pub broadcast_delay_secs: Option<u32>,
    /// True when the delay is over the stale limit (config, 120 s). Decided by the core.
    pub delay_stale: bool,
    pub enemies: Vec<EnemyRowVm>,
    /// Newest first.
    pub purchase_log: Vec<PurchaseEntryVm>,
}

impl LiveVm {
    pub fn show_kda(&self) -> bool {
        self.enemies.iter().any(|e| e.kda.is_some())
    }

    pub fn show_level(&self) -> bool {
        self.enemies.iter().any(|e| e.level.is_some())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewListItemVm {
    pub match_id: u64,
    pub won: bool,
    pub hero_name: String,
    pub duration_secs: DurationSecs,
    pub ended_ago_secs: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SoulSourceKind {
    LaneTroopers,
    NeutralCamps,
    Breakables,
    Urn,
    SinnersSacrifice,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SoulSourceVm {
    pub kind: SoulSourceKind,
    pub souls: u32,
}

/// Alpha reviews are stats-only (doc 07): match record, lane soul lead at 9:00, soul sources.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReviewReportVm {
    pub match_id: u64,
    pub won: bool,
    pub hero_name: String,
    pub duration_secs: DurationSecs,
    /// When the lane lead was measured, in game seconds. From config (9:00 = 540).
    pub lane_check_secs: DurationSecs,
    /// Souls ahead (positive) or behind (negative) at `lane_check_secs`. `None` for matches
    /// shorter than that.
    pub lane_soul_lead: Option<i32>,
    pub soul_sources: Vec<SoulSourceVm>,
}

#[derive(Clone, Debug, Default)]
pub struct ReviewsVm {
    pub state: DataState,
    pub list: Vec<ReviewListItemVm>,
    pub selected: Option<usize>,
    pub report: Option<ReviewReportVm>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuildSource {
    InGame,
    External,
    Manual,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BuildVm {
    pub build_id: String,
    pub name: String,
    pub hero_name: String,
    pub source: BuildSource,
    pub item_count: u32,
    /// The active plan for its hero (FR-BUILD-005).
    pub active: bool,
    /// "Needs update: N items removed" (FR-BUILD-006).
    pub items_removed: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CommunityBuildVm {
    pub name: String,
    pub hero_name: String,
    pub usage: u32,
}

/// A hero or an item the user can pick, from API-15 at runtime.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NamedOption {
    pub id: u32,
    pub name: String,
}

#[derive(Clone, Debug, Default)]
pub struct BuildsVm {
    pub state: DataState,
    pub library: Vec<BuildVm>,
    pub heroes: Vec<NamedOption>,
    pub community_state: DataState,
    pub community: Vec<CommunityBuildVm>,
}

#[derive(Clone, Debug, Default)]
pub struct SettingsVm {
    pub account_id: Option<u32>,
    pub version: String,
}

/// Everything the main window shows.
#[derive(Clone, Debug, Default)]
pub struct AppVm {
    pub offline: bool,
    pub home: HomeVm,
    pub scouting: ScoutingVm,
    pub live: LiveVm,
    pub reviews: ReviewsVm,
    pub builds: BuildsVm,
    pub settings: SettingsVm,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn win_rate_rounds_and_handles_zero() {
        assert_eq!(Record { wins: 0, losses: 0 }.win_rate_pct(), None);
        assert_eq!(Record { wins: 1, losses: 2 }.win_rate_pct(), Some(33));
        assert_eq!(Record { wins: 2, losses: 1 }.win_rate_pct(), Some(67));
        assert_eq!(
            Record {
                wins: 10,
                losses: 0
            }
            .win_rate_pct(),
            Some(100)
        );
    }

    #[test]
    fn kda_and_level_columns_follow_the_data() {
        let row = |kda, level| EnemyRowVm {
            hero_name: "Hero 1".into(),
            player_name: "Player 1".into(),
            items: vec![],
            kda,
            level,
        };
        let mut live = LiveVm {
            enemies: vec![row(None, None)],
            ..Default::default()
        };
        assert!(!live.show_kda() && !live.show_level());
        live.enemies.push(row(
            Some(Kda {
                kills: 1,
                deaths: 2,
                assists: 3,
            }),
            Some(5),
        ));
        assert!(live.show_kda() && live.show_level());
    }
}
