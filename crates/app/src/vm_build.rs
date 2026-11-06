//! Builds view models from core output (PRG-20). Pure functions: no Slint, no I/O, no tokio.
//!
//! Fair play (NFR-FAIR-003): `live_vm` takes a `LiveSafeState` and nothing else from the game, so
//! the Live view can only show what `live_safe` let through. Scouting reads the roster from the
//! same Live-Safe players. Nothing here knows about chat, positions or fights.
//!
//! No hero, item or rank names live here (NFR-MAINT-001). Names come from the API-15 catalog and
//! the words for "unknown" come from Fluent through `Fallbacks`.

use crate::view_model::*;
use stalemate_core::{
    account::ConsoleLogState,
    api_client::{AssetCatalog, AssetDirs, CommunityBuild, ImageKind},
    builds::Build,
    live_safe::{LiveSafeState, PlayerLs},
    purchase_log::{Action, PurchaseEvent},
    review::{LaneSection, ReviewResult, SoulSource, StatsReview},
    scouting::{CardState, ScoutingCard},
    storage::{MatchResult, MatchRow},
    AccountId, HeroId, ItemId, RosterEntry,
};

/// Words shown when the data has no name. They come from Fluent, not from this file.
#[derive(Clone, Debug)]
pub struct Fallbacks {
    pub hero: String,
    pub item: String,
    pub player: String,
}

pub fn hero_name(catalog: Option<&AssetCatalog>, id: HeroId, fb: &Fallbacks) -> String {
    catalog
        .and_then(|c| c.hero(id))
        .map(|h| h.name.clone())
        .unwrap_or_else(|| fb.hero.clone())
}

pub fn item_name(catalog: Option<&AssetCatalog>, id: ItemId, fb: &Fallbacks) -> String {
    catalog
        .and_then(|c| c.item(id))
        .map(|i| i.name.clone())
        .unwrap_or_else(|| fb.item.clone())
}

/// The shop category of an item, from its API-15 `slot` (DEC-068). The three slot values map to
/// the three shop tabs. An item with no slot, or a slot the app doesn't know, has no category and
/// shows as a neutral chip. Nothing here lists items.
pub fn item_category(catalog: Option<&AssetCatalog>, id: ItemId) -> Option<ItemCategory> {
    category_from_slot(catalog?.item(id)?.slot.as_deref()?)
}

pub fn category_from_slot(slot: &str) -> Option<ItemCategory> {
    // The live catalog uses weapon, vitality and spirit. "armor" and "tech" are aliases for older
    // catalog data. Matching ignores case.
    match slot.to_ascii_lowercase().as_str() {
        "weapon" => Some(ItemCategory::Weapon),
        "vitality" | "armor" => Some(ItemCategory::Vitality),
        "spirit" | "tech" => Some(ItemCategory::Spirit),
        _ => None,
    }
}

fn item_vm(
    catalog: Option<&AssetCatalog>,
    dirs: Option<&AssetDirs>,
    id: ItemId,
    fb: &Fallbacks,
) -> ItemVm {
    ItemVm {
        name: item_name(catalog, id, fb),
        icon: item_icon(catalog, dirs, id),
        category: item_category(catalog, id),
    }
}

/// The small hero icon, if the cache has it.
fn hero_icon(
    catalog: Option<&AssetCatalog>,
    dirs: Option<&AssetDirs>,
    id: HeroId,
) -> Option<std::path::PathBuf> {
    let (c, d) = (catalog?, dirs?);
    let hero = c.hero(id)?;
    let url = hero.small_image.as_ref().or(hero.card_image.as_ref())?;
    let path = d.image_path(c.patch(), ImageKind::HeroSmall, id, url);
    path.is_file().then_some(path)
}

/// Art the asset cache already has on disk. Never downloads; `None` falls back to the name.
fn item_icon(
    catalog: Option<&AssetCatalog>,
    dirs: Option<&AssetDirs>,
    id: ItemId,
) -> Option<std::path::PathBuf> {
    let (c, d) = (catalog?, dirs?);
    let item = c.item(id)?;
    let url = item.shop_image.as_ref().or(item.image.as_ref())?;
    let path = d.image_path(c.patch(), ImageKind::Item, id, url);
    path.is_file().then_some(path)
}

fn player_name(p: &PlayerLs, fb: &Fallbacks) -> String {
    match p.name.as_deref().map(str::trim) {
        Some(n) if !n.is_empty() => n.to_owned(),
        _ => fb.player.clone(),
    }
}

/// The user's team, or team 0 when the user is not in the roster (a match watched by ID).
pub fn my_team(ls: &LiveSafeState, me: Option<AccountId>) -> u8 {
    me.filter(|m| *m != 0)
        .and_then(|m| ls.players.iter().find(|p| p.account_id == m))
        .map_or(0, |p| p.team)
}

/// Team 0 is Amber and team 1 is Sapphire (broadcast `normalize_team`: raw 2 and 3).
pub fn side_of(team: u8) -> Side {
    if team == 0 {
        Side::Amber
    } else {
        Side::Sapphire
    }
}

pub struct LiveInput<'a> {
    pub state: &'a LiveSafeState,
    pub me: Option<AccountId>,
    pub purchases: &'a [PurchaseEvent],
    pub catalog: Option<&'a AssetCatalog>,
    pub dirs: Option<&'a AssetDirs>,
    pub delay_seen: bool,
    /// `rules.broadcast.stale_delay_s`.
    pub stale_delay_s: u32,
    pub data_state: DataState,
}

pub fn live_vm(inp: &LiveInput<'_>, fb: &Fallbacks) -> LiveVm {
    let ls = inp.state;
    let mine = my_team(ls, inp.me);
    let enemies = ls
        .players
        .iter()
        .filter(|p| p.team != mine)
        .map(|p| EnemyRowVm {
            hero_name: hero_name(inp.catalog, p.hero_id, fb),
            portrait: hero_icon(inp.catalog, inp.dirs, p.hero_id),
            player_name: player_name(p, fb),
            team: side_of(p.team),
            souls: p.souls,
            items: p
                .items
                .iter()
                .flatten()
                .map(|&id| item_vm(inp.catalog, inp.dirs, id, fb))
                .collect(),
            kda: p.kda.map(|(kills, deaths, assists)| Kda {
                kills,
                deaths,
                assists,
            }),
            level: p.level,
        })
        .collect();
    let name_of = |account: AccountId| {
        ls.players
            .iter()
            .find(|p| p.account_id == account && account != 0)
            .map_or_else(|| fb.player.clone(), |p| player_name(p, fb))
    };
    let purchase_log = inp
        .purchases
        .iter()
        .rev()
        .map(|e| PurchaseEntryVm {
            t_game_secs: e.t_game_s,
            player_name: name_of(e.account_id),
            item_name: item_name(inp.catalog, e.item_id, fb),
            category: item_category(inp.catalog, e.item_id),
            action: match e.action {
                Action::Buy => PurchaseAction::Buy,
                Action::Sell => PurchaseAction::Sell,
                Action::Upgrade => PurchaseAction::Upgrade,
            },
        })
        .collect();
    LiveVm {
        state: inp.data_state.clone(),
        broadcast_delay_secs: inp.delay_seen.then_some(ls.delay_s),
        delay_stale: inp.delay_seen && ls.delay_s > inp.stale_delay_s,
        enemies,
        purchase_log,
    }
}

// ---- scouting ----------------------------------------------------------------------------

/// The roster for scouting, taken from the Live-Safe players (the name is the LS-02 name).
pub fn roster_from_live_safe(ls: &LiveSafeState) -> Vec<RosterEntry> {
    ls.players
        .iter()
        .map(|p| RosterEntry {
            account_id: p.account_id,
            name: p.name.clone().unwrap_or_default(),
            team: p.team,
            hero_id: p.hero_id,
        })
        .collect()
}

/// Card identity: an account, or team and hero for bots (all bots share account 0).
pub type CardKey = (AccountId, u8, HeroId);

pub fn card_key(account_id: AccountId, team: u8, hero_id: HeroId) -> CardKey {
    if account_id == 0 {
        (0, team, hero_id)
    } else {
        (account_id, 0, 0)
    }
}

/// The rank badge splits into a tier and a step (`64` is tier 6, step 4). Names for tiers are API
/// data the client doesn't load yet, so the card shows numbers.
pub fn badge_parts(badge: u32) -> (u32, u32) {
    (badge / 10, badge % 10)
}

pub struct ScoutInput<'a> {
    pub roster: &'a [RosterEntry],
    pub cards: &'a std::collections::HashMap<CardKey, ScoutingCard>,
    pub me: Option<AccountId>,
    pub visible: bool,
    pub history_window: u32,
    pub catalog: Option<&'a AssetCatalog>,
}

pub fn scouting_vm(
    inp: &ScoutInput<'_>,
    fb: &Fallbacks,
    rank_text: &dyn Fn(u32) -> String,
) -> ScoutingVm {
    let mine = inp
        .me
        .filter(|m| *m != 0)
        .and_then(|m| inp.roster.iter().find(|e| e.account_id == m))
        .map_or(0, |e| e.team);
    let mut your_team = Vec::new();
    let mut enemy_team = Vec::new();
    let mut any_ok = false;
    let mut errors = 0usize;
    let mut scouted = 0usize;
    for e in inp.roster {
        let is_you = inp.me.is_some_and(|m| m != 0 && m == e.account_id);
        let hero = hero_name(inp.catalog, e.hero_id, fb);
        let player = if e.name.trim().is_empty() {
            fb.player.clone()
        } else {
            e.name.clone()
        };
        let body = if is_you || e.account_id == 0 {
            CardBody::HeroOnly
        } else {
            scouted += 1;
            match inp.cards.get(&card_key(e.account_id, e.team, e.hero_id)) {
                None => CardBody::Loading,
                Some(c) => match &c.state {
                    CardState::Loading => CardBody::Loading,
                    CardState::NoPublicData => CardBody::NoPublicData,
                    CardState::HeroOnly => CardBody::HeroOnly,
                    CardState::Error { .. } => {
                        errors += 1;
                        CardBody::Unavailable
                    }
                    CardState::Ready => {
                        any_ok = true;
                        CardBody::Stats(stats_from_card(c, inp, fb, rank_text))
                    }
                },
            }
        };
        let card = ScoutCardVm {
            hero_name: hero,
            player_name: player,
            is_you,
            body,
        };
        if e.team == mine {
            your_team.push(card);
        } else {
            enemy_team.push(card);
        }
    }
    let state = if inp.roster.is_empty() {
        DataState::Empty
    } else if scouted > 0 && errors == scouted && !any_ok {
        DataState::Error {
            source: "deadlock-api".into(),
        }
    } else {
        DataState::Loaded
    };
    ScoutingVm {
        visible: inp.visible,
        state,
        your_side: side_of(mine),
        your_team,
        enemy_team,
    }
}

fn stats_from_card(
    c: &ScoutingCard,
    inp: &ScoutInput<'_>,
    fb: &Fallbacks,
    rank_text: &dyn Fn(u32) -> String,
) -> ScoutStats {
    let overall = c.wl_overall.map_or(Record::default(), |w| Record {
        wins: w.wins,
        losses: w.matches.saturating_sub(w.wins),
    });
    let last = c.wl_recent.map_or(Record::default(), |w| Record {
        wins: w.wins,
        losses: w.matches.saturating_sub(w.wins),
    });
    ScoutStats {
        overall,
        last,
        last_window: c.wl_recent.map_or(inp.history_window, |w| w.window),
        rank: c.rank_badge.map(rank_text),
        mmr_trend_per_10: c.mmr_trend_per_10.map(|v| v.round() as i32),
        mains: c
            .mains
            .iter()
            .map(|m| MainHero {
                hero_name: hero_name(inp.catalog, m.hero_id, fb),
                matches: m.matches,
                wins: m.wins,
            })
            .collect(),
        matches_on_hero: c.hero_matches.unwrap_or(0),
    }
}

// ---- home --------------------------------------------------------------------------------

pub struct CheckInput<'a> {
    /// A Steam folder was found.
    pub steam_found: bool,
    /// `Some(true)` when the launch options hold `-condebug`; `None` when that can't be read.
    pub condebug_option: Option<bool>,
    pub console_log: Option<&'a ConsoleLogState>,
    pub account_id: Option<AccountId>,
    /// `Pass` when the Match Watcher patterns are configured.
    pub match_watcher: CheckState,
    pub broadcast: CheckState,
    pub deadlock_api: CheckState,
}

/// `-condebug` passes when the launch option is set, or when it can't be read and `console.log`
/// exists (FR-SET-003). A set option with no log yet still fails: the game hasn't been restarted.
pub fn condebug_state(option: Option<bool>, log: Option<&ConsoleLogState>) -> CheckState {
    let log_present = matches!(log, Some(ConsoleLogState::Present { .. }));
    match option {
        Some(true) | None if log_present => CheckState::Pass,
        _ => CheckState::Fail,
    }
}

pub fn setup_checks(inp: &CheckInput<'_>) -> Vec<SetupCheck> {
    let pass = |b: bool| {
        if b {
            CheckState::Pass
        } else {
            CheckState::Fail
        }
    };
    vec![
        SetupCheck {
            id: CheckId::Steam,
            state: pass(inp.steam_found),
            value: None,
        },
        SetupCheck {
            id: CheckId::Condebug,
            state: condebug_state(inp.condebug_option, inp.console_log),
            value: None,
        },
        SetupCheck {
            id: CheckId::AccountId,
            state: pass(inp.account_id.is_some()),
            value: inp.account_id.map(|a| a.to_string()),
        },
        SetupCheck {
            id: CheckId::MatchWatcher,
            state: inp.match_watcher,
            value: None,
        },
        SetupCheck {
            id: CheckId::Broadcast,
            state: inp.broadcast,
            value: None,
        },
        SetupCheck {
            id: CheckId::DeadlockApi,
            state: inp.deadlock_api,
            value: None,
        },
    ]
}

// ---- reviews -----------------------------------------------------------------------------

/// Seconds since the match ended: `started_at` (RFC 3339) plus the duration, against `now`.
pub fn ended_ago_secs(
    started_at: &str,
    duration_s: Option<u32>,
    now: time::OffsetDateTime,
) -> Option<u64> {
    use time::format_description::well_known::Rfc3339;
    let start = time::OffsetDateTime::parse(started_at, &Rfc3339).ok()?;
    let end = start + time::Duration::seconds(i64::from(duration_s.unwrap_or(0)));
    u64::try_from((now - end).whole_seconds()).ok()
}

/// Four results. Abandoned has its own label; unknown or NULL is "no result". Neither is a loss.
pub fn outcome_of(r: Option<MatchResult>) -> Outcome {
    match r {
        Some(MatchResult::Win) => Outcome::Win,
        Some(MatchResult::Loss) => Outcome::Loss,
        Some(MatchResult::Abandoned) => Outcome::Abandoned,
        None => Outcome::NoResult,
    }
}

/// A saved match with a Review row, or a record-only match, is ready. A match with neither
/// (API-05 had nothing yet) is pending and offers a retry.
pub fn review_list_item(
    m: &MatchRow,
    has_review: bool,
    catalog: Option<&AssetCatalog>,
    fb: &Fallbacks,
    now: time::OffsetDateTime,
) -> ReviewListItemVm {
    ReviewListItemVm {
        match_id: m.match_id,
        pending: !has_review && m.result.is_none(),
        outcome: outcome_of(m.result),
        hero_name: hero_name(catalog, m.user_hero_id, fb),
        duration_secs: m.duration_s.unwrap_or(0),
        ended_ago_secs: ended_ago_secs(&m.started_at, m.duration_s, now),
    }
}

/// A match with no Review row: the record only (a non-Standard mode, DEC-019).
pub fn record_only_report(
    m: &MatchRow,
    catalog: Option<&AssetCatalog>,
    fb: &Fallbacks,
    lane_check_secs: u32,
) -> ReviewReportVm {
    ReviewReportVm {
        match_id: m.match_id,
        outcome: outcome_of(m.result),
        hero_name: hero_name(catalog, m.user_hero_id, fb),
        duration_secs: m.duration_s.unwrap_or(0),
        lane_check_secs,
        lane_soul_lead: None,
        soul_sources: Vec::new(),
    }
}

/// Reads the saved `StatsReview` JSON (`reviews.report_json`). `None` if it isn't one.
pub fn report_from_json(
    json: &str,
    catalog: Option<&AssetCatalog>,
    fb: &Fallbacks,
    default_lane_check_secs: u32,
) -> Option<ReviewReportVm> {
    let r: StatsReview = serde_json::from_str(json).ok()?;
    let outcome = match r.result {
        ReviewResult::Win => Outcome::Win,
        ReviewResult::Loss => Outcome::Loss,
        ReviewResult::Abandoned => Outcome::Abandoned,
        ReviewResult::Unknown => Outcome::NoResult,
    };
    let (lane_check_secs, lane_soul_lead) = match &r.lane {
        LaneSection::Available(l) => (l.t_game_s, Some(l.lead.round() as i32)),
        LaneSection::Unavailable { .. } => (default_lane_check_secs, None),
    };
    // `souls: None` means unavailable, which is not zero: the row is left out.
    let soul_sources = r
        .soul_sources
        .iter()
        .flat_map(|s| s.rows.iter())
        .filter_map(|row| {
            let souls = u32::try_from(row.souls?).ok()?;
            let kind = match row.source {
                SoulSource::LaneTroopers => SoulSourceKind::LaneTroopers,
                SoulSource::NeutralCamps => SoulSourceKind::NeutralCamps,
                SoulSource::Breakables => SoulSourceKind::Breakables,
                SoulSource::SinnersSacrifice => SoulSourceKind::SinnersSacrifice,
                SoulSource::Urn => SoulSourceKind::Urn,
                SoulSource::Other => SoulSourceKind::Other,
            };
            Some(SoulSourceVm { kind, souls })
        })
        .collect();
    Some(ReviewReportVm {
        match_id: r.match_id,
        outcome,
        hero_name: hero_name(catalog, r.hero_id, fb),
        duration_secs: r.duration_s,
        lane_check_secs,
        lane_soul_lead,
        soul_sources,
    })
}

// ---- builds ------------------------------------------------------------------------------

/// The sections of a community build, with each item's category from the catalog (DEC-068).
pub fn community_sections(
    b: &CommunityBuild,
    catalog: Option<&AssetCatalog>,
    fb: &Fallbacks,
) -> Vec<SectionVm> {
    b.hero_build
        .details
        .mod_categories
        .iter()
        .map(|c| SectionVm {
            title: c.name.clone(),
            items: c
                .mods
                .iter()
                .map(|m| item_vm(catalog, None, m.ability_id, fb))
                .collect(),
        })
        .collect()
}

pub fn build_vm(
    b: &Build,
    catalog: Option<&AssetCatalog>,
    active: bool,
    fb: &Fallbacks,
) -> BuildVm {
    use stalemate_core::builds::BuildSource as Src;
    BuildVm {
        build_id: b.build_id.clone(),
        name: b.name.clone(),
        hero_name: hero_name(catalog, b.hero_id, fb),
        source: match b.source {
            Src::Ingame => BuildSource::InGame,
            Src::External => BuildSource::External,
            Src::Manual => BuildSource::Manual,
        },
        item_count: b.all_items().count() as u32,
        sections: b
            .sections
            .iter()
            .map(|sec| SectionVm {
                title: sec.title.clone(),
                items: sec
                    .item_ids
                    .iter()
                    .map(|&id| item_vm(catalog, None, id, fb))
                    .collect(),
            })
            .collect(),
        active,
        items_removed: catalog.map_or(0, |c| {
            if b.needs_update {
                b.removed_items(c).len() as u32
            } else {
                0
            }
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use stalemate_core::game_state::GameState;
    use stalemate_core::live_safe::{project_state, Ls, Whitelist};
    use stalemate_core::ParserEvent;
    use std::collections::HashMap;
    use std::time::SystemTime;

    fn fb() -> Fallbacks {
        Fallbacks {
            hero: "Unknown hero".into(),
            item: "Unknown item".into(),
            player: "Player".into(),
        }
    }

    fn state(wl: &Whitelist) -> LiveSafeState {
        let mut gs = GameState::new(9, 1, SystemTime::UNIX_EPOCH);
        gs.apply(
            &ParserEvent::Roster {
                players: vec![
                    RosterEntry {
                        account_id: 1,
                        name: "me".into(),
                        team: 1,
                        hero_id: 10,
                    },
                    RosterEntry {
                        account_id: 2,
                        name: "foe".into(),
                        team: 0,
                        hero_id: 20,
                    },
                ],
            },
            SystemTime::UNIX_EPOCH,
        );
        gs.apply(
            &ParserEvent::Kda {
                account_id: 2,
                kills: 3,
                deaths: 2,
                assists: 1,
            },
            SystemTime::UNIX_EPOCH,
        );
        gs.apply(
            &ParserEvent::Position {
                account_id: 2,
                x: 1.0,
                y: 2.0,
                z: 3.0,
            },
            SystemTime::UNIX_EPOCH,
        );
        gs.apply(
            &ParserEvent::Chat {
                account_id: 2,
                team_only: false,
                text: "secret".into(),
            },
            SystemTime::UNIX_EPOCH,
        );
        project_state(&gs, wl)
    }

    fn input<'a>(ls: &'a LiveSafeState) -> LiveInput<'a> {
        LiveInput {
            state: ls,
            me: Some(1),
            purchases: &[],
            catalog: None,
            dirs: None,
            delay_seen: true,
            stale_delay_s: 60,
            data_state: DataState::Loaded,
        }
    }

    #[test]
    fn enemy_board_follows_the_whitelist_and_hides_blocked_fields() {
        // KDA and level are off here, as before the author verifies them: the columns stay hidden.
        let wl = Whitelist([Ls::Ls02Name].into_iter().collect());
        let ls = state(&wl);
        let vm = live_vm(&input(&ls), &fb());
        assert_eq!(vm.enemies.len(), 1);
        assert_eq!(vm.enemies[0].player_name, "foe");
        assert_eq!(vm.enemies[0].kda, None);
        assert!(!vm.show_kda() && !vm.show_level());
        // With KDA on, the column shows.
        let wl = Whitelist([Ls::Ls02Name, Ls::Ls06Kda].into_iter().collect());
        let ls = state(&wl);
        let vm = live_vm(&input(&ls), &fb());
        assert_eq!(
            vm.enemies[0].kda,
            Some(Kda {
                kills: 3,
                deaths: 2,
                assists: 1
            })
        );
    }

    #[test]
    fn names_fall_back_when_ls02_is_off() {
        let ls = state(&Whitelist::default());
        let vm = live_vm(&input(&ls), &fb());
        assert_eq!(vm.enemies[0].player_name, "Player");
    }

    #[test]
    fn stale_delay_is_flagged_from_config_value() {
        let ls = {
            let mut s = state(&Whitelist::default());
            s.delay_s = 61;
            s
        };
        assert!(live_vm(&input(&ls), &fb()).delay_stale);
        let ls = {
            let mut s = state(&Whitelist::default());
            s.delay_s = 60;
            s
        };
        let vm = live_vm(&input(&ls), &fb());
        assert!(!vm.delay_stale);
        assert_eq!(vm.broadcast_delay_secs, Some(60));
    }

    #[test]
    fn purchase_log_is_newest_first() {
        let ls = state(&Whitelist::default());
        let events = [1u32, 2].map(|n| PurchaseEvent {
            match_id: 9,
            account_id: 2,
            t_game_s: n * 10,
            item_id: n,
            action: Action::Buy,
            cost: 0,
        });
        let mut inp = input(&ls);
        inp.purchases = &events;
        let vm = live_vm(&inp, &fb());
        assert_eq!(vm.purchase_log[0].t_game_secs, 20);
        assert_eq!(vm.purchase_log[1].t_game_secs, 10);
    }

    #[test]
    fn scouting_cards_split_by_team_and_hold_you_as_hero_only() {
        let wl = Whitelist([Ls::Ls02Name].into_iter().collect());
        let ls = state(&wl);
        let roster = roster_from_live_safe(&ls);
        let cards = HashMap::new();
        let vm = scouting_vm(
            &ScoutInput {
                roster: &roster,
                cards: &cards,
                me: Some(1),
                visible: true,
                history_window: 20,
                catalog: None,
            },
            &fb(),
            &|b| b.to_string(),
        );
        assert_eq!(vm.your_team.len(), 1);
        assert!(vm.your_team[0].is_you);
        assert_eq!(vm.your_team[0].body, CardBody::HeroOnly);
        assert_eq!(vm.your_side, Side::Sapphire);
        assert_eq!(vm.enemy_team.len(), 1);
        assert_eq!(vm.enemy_team[0].body, CardBody::Loading);
        assert!(vm.visible);
    }

    #[test]
    fn opted_out_player_shows_no_public_data() {
        let wl = Whitelist([Ls::Ls02Name].into_iter().collect());
        let ls = state(&wl);
        let roster = roster_from_live_safe(&ls);
        let mut cards = HashMap::new();
        cards.insert(
            card_key(2, 0, 20),
            ScoutingCard {
                account_id: 2,
                hero_id: 20,
                team: 0,
                name: "foe".into(),
                state: CardState::NoPublicData,
                wl_overall: None,
                wl_recent: None,
                rank_badge: None,
                mmr_trend_per_10: None,
                mains: vec![],
                hero_matches: None,
            },
        );
        let vm = scouting_vm(
            &ScoutInput {
                roster: &roster,
                cards: &cards,
                me: Some(1),
                visible: false,
                history_window: 20,
                catalog: None,
            },
            &fb(),
            &|b| b.to_string(),
        );
        assert_eq!(vm.enemy_team[0].body, CardBody::NoPublicData);
        assert!(!vm.visible);
    }

    #[test]
    fn tc_20_1_no_steam_gives_failed_checks() {
        let checks = setup_checks(&CheckInput {
            steam_found: false,
            condebug_option: None,
            console_log: Some(&ConsoleLogState::Missing),
            account_id: None,
            match_watcher: CheckState::Pass,
            broadcast: CheckState::Pending,
            deadlock_api: CheckState::Pending,
        });
        let get = |id| checks.iter().find(|c| c.id == id).unwrap().state;
        assert_eq!(get(CheckId::Steam), CheckState::Fail);
        assert_eq!(get(CheckId::Condebug), CheckState::Fail);
        assert_eq!(get(CheckId::AccountId), CheckState::Fail);
    }

    #[test]
    fn match_watcher_not_configured_is_a_failed_check_with_the_rest_intact() {
        let checks = setup_checks(&CheckInput {
            steam_found: true,
            condebug_option: Some(true),
            console_log: Some(&ConsoleLogState::Present {
                len: 1,
                modified: None,
            }),
            account_id: Some(5),
            match_watcher: CheckState::Fail,
            broadcast: CheckState::Pending,
            deadlock_api: CheckState::Pass,
        });
        let get = |id| checks.iter().find(|c| c.id == id).unwrap().state;
        assert_eq!(get(CheckId::MatchWatcher), CheckState::Fail);
        assert_eq!(get(CheckId::Steam), CheckState::Pass);
        assert_eq!(get(CheckId::Condebug), CheckState::Pass);
        assert_eq!(get(CheckId::AccountId), CheckState::Pass);
    }

    #[test]
    fn condebug_needs_the_log_even_when_the_option_is_set() {
        let missing = ConsoleLogState::Missing;
        let present = ConsoleLogState::Present {
            len: 0,
            modified: None,
        };
        assert_eq!(condebug_state(Some(true), Some(&missing)), CheckState::Fail);
        assert_eq!(condebug_state(Some(true), Some(&present)), CheckState::Pass);
        assert_eq!(
            condebug_state(Some(false), Some(&present)),
            CheckState::Fail
        );
        assert_eq!(condebug_state(None, Some(&present)), CheckState::Pass);
        assert_eq!(condebug_state(None, None), CheckState::Fail);
    }

    #[test]
    fn saved_review_json_becomes_a_report() {
        let v = serde_json::json!({
            "match_id": 5, "result": "win", "hero_id": 7, "duration_s": 1800,
            "game_mode": "Normal", "standard_mode": true, "partial": false,
            "flags": {"short_match": false, "abandoned": false}, "r02": null,
            "lane": {"state": "available", "t_game_s": 540, "lead": -1200.4,
                     "assigned_lane": 1, "user_net_worth": 1, "opponent_count": 1,
                     "opponent_mean_net_worth": 1.0},
            "soul_sources": {"total": 100, "rows": [
                {"source": "lane_troopers", "souls": 60, "share_pct": 60.0},
                {"source": "urn", "souls": null, "share_pct": null},
                {"source": "other", "souls": 40, "share_pct": 40.0}]}
        });
        let r = report_from_json(&v.to_string(), None, &fb(), 540).unwrap();
        assert_eq!(r.outcome, Outcome::Win);
        assert_eq!(r.lane_soul_lead, Some(-1200));
        assert_eq!(
            r.soul_sources.len(),
            2,
            "the unavailable Urn row is left out"
        );
        assert_eq!(r.hero_name, "Unknown hero");
    }

    #[test]
    fn abandoned_has_its_own_state_and_unknown_is_no_result_never_a_loss() {
        for (text, abandoned) in [("abandoned", true), ("unknown", false)] {
            let v = serde_json::json!({
                "match_id": 5, "result": text, "hero_id": 7, "duration_s": 300,
                "game_mode": "Normal", "standard_mode": true, "partial": false,
                "flags": {"short_match": false, "abandoned": abandoned}, "r02": null,
                "lane": {"state": "unavailable", "reason": "match_too_short"},
                "soul_sources": null
            });
            let r = report_from_json(&v.to_string(), None, &fb(), 540).unwrap();
            let want = if abandoned {
                Outcome::Abandoned
            } else {
                Outcome::NoResult
            };
            assert_eq!(r.outcome, want);
        }
        assert_eq!(outcome_of(Some(MatchResult::Abandoned)), Outcome::Abandoned);
        assert_eq!(outcome_of(None), Outcome::NoResult);
        assert_eq!(outcome_of(Some(MatchResult::Loss)), Outcome::Loss);
    }

    #[test]
    fn unavailable_lane_has_no_lead() {
        let v = serde_json::json!({
            "match_id": 5, "result": "loss", "hero_id": 7, "duration_s": 300,
            "game_mode": "Normal", "standard_mode": true, "partial": false,
            "flags": {"short_match": true, "abandoned": false}, "r02": null,
            "lane": {"state": "unavailable", "reason": "match_too_short"},
            "soul_sources": null
        });
        let r = report_from_json(&v.to_string(), None, &fb(), 540).unwrap();
        assert_eq!(r.outcome, Outcome::Loss);
        assert_eq!(r.lane_soul_lead, None);
        assert_eq!(r.lane_check_secs, 540);
    }

    #[test]
    fn ended_ago_counts_from_the_end_of_the_match() {
        let now = time::OffsetDateTime::from_unix_timestamp(10_000).unwrap();
        // Started at 1000 s, lasted 2000 s: ended at 3000 s, so 7000 s ago.
        let started = "1970-01-01T00:16:40Z";
        assert_eq!(ended_ago_secs(started, Some(2000), now), Some(7000));
        assert_eq!(ended_ago_secs("garbage", Some(1), now), None);
    }

    #[test]
    fn pending_means_no_review_and_no_result() {
        use stalemate_core::storage::{DataSource, GameMode};
        let m = MatchRow {
            match_id: 1,
            started_at: "1970-01-01T00:16:40Z".into(),
            duration_s: None,
            game_mode: GameMode::Other("unknown".into()),
            match_mode: None,
            patch: "p".into(),
            user_team: 0,
            user_hero_id: 1,
            result: None,
            data_source: DataSource::Broadcast,
            recording_path: None,
        };
        let now = time::OffsetDateTime::from_unix_timestamp(10_000).unwrap();
        assert!(review_list_item(&m, false, None, &fb(), now).pending);
        assert!(!review_list_item(&m, true, None, &fb(), now).pending);
    }

    fn catalog_with_slots() -> AssetCatalog {
        use stalemate_core::api_client::ItemAsset;
        let item = |id, slot: Option<&str>| ItemAsset {
            id,
            class_name: format!("item_{id}"),
            name: format!("Item {id}"),
            kind: "upgrade".into(),
            cost: 500,
            tier: Some(1),
            slot: slot.map(str::to_owned),
            shopable: true,
            disabled: false,
            component_items: vec![],
            image: None,
            shop_image: None,
        };
        AssetCatalog::from_parts(
            "p",
            vec![],
            vec![
                item(1, Some("weapon")),
                item(2, Some("vitality")),
                item(3, Some("spirit")),
                item(4, None),
                item(5, Some("something-new")),
            ],
        )
    }

    // DEC-068: the category comes from the catalog's slot, never from a list in the app.
    #[test]
    fn item_category_comes_from_the_catalog_slot() {
        let cat = catalog_with_slots();
        assert_eq!(item_category(Some(&cat), 1), Some(ItemCategory::Weapon));
        assert_eq!(item_category(Some(&cat), 2), Some(ItemCategory::Vitality));
        assert_eq!(item_category(Some(&cat), 3), Some(ItemCategory::Spirit));
        assert_eq!(item_category(Some(&cat), 4), None, "no slot, no category");
        assert_eq!(item_category(Some(&cat), 5), None, "unknown slot");
        assert_eq!(item_category(Some(&cat), 99), None, "not in the catalog");
        assert_eq!(item_category(None, 1), None, "no catalog yet");
    }

    #[test]
    fn real_catalog_slot_names_map_to_categories() {
        assert_eq!(category_from_slot("weapon"), Some(ItemCategory::Weapon));
        assert_eq!(category_from_slot("vitality"), Some(ItemCategory::Vitality));
        assert_eq!(category_from_slot("spirit"), Some(ItemCategory::Spirit));
    }

    #[test]
    fn legacy_aliases_and_case_are_accepted() {
        assert_eq!(category_from_slot("armor"), Some(ItemCategory::Vitality));
        assert_eq!(category_from_slot("tech"), Some(ItemCategory::Spirit));
        assert_eq!(category_from_slot("Spirit"), Some(ItemCategory::Spirit));
        assert_eq!(category_from_slot("WEAPON"), Some(ItemCategory::Weapon));
        assert_eq!(category_from_slot("Armor"), Some(ItemCategory::Vitality));
        assert_eq!(category_from_slot("other"), None);
    }

    #[test]
    fn live_rows_carry_team_souls_and_item_categories() {
        let cat = catalog_with_slots();
        let wl = Whitelist(
            [Ls::Ls02Name, Ls::Ls05Items, Ls::Ls08Souls]
                .into_iter()
                .collect(),
        );
        let mut gs = GameState::new(9, 1, SystemTime::UNIX_EPOCH);
        gs.apply(
            &ParserEvent::Roster {
                players: vec![
                    RosterEntry {
                        account_id: 1,
                        name: "me".into(),
                        team: 1,
                        hero_id: 10,
                    },
                    RosterEntry {
                        account_id: 2,
                        name: "foe".into(),
                        team: 0,
                        hero_id: 20,
                    },
                ],
            },
            SystemTime::UNIX_EPOCH,
        );
        gs.apply(
            &ParserEvent::Inventory {
                account_id: 2,
                items: vec![1, 2, 3, 4],
            },
            SystemTime::UNIX_EPOCH,
        );
        gs.apply(
            &ParserEvent::NetWorth {
                account_id: 2,
                souls: 4321,
            },
            SystemTime::UNIX_EPOCH,
        );
        let ls = project_state(&gs, &wl);
        let mut inp = input(&ls);
        inp.catalog = Some(&cat);
        let vm = live_vm(&inp, &fb());
        assert_eq!(vm.enemies.len(), 1);
        assert_eq!(vm.enemies[0].team, Side::Amber);
        assert_eq!(vm.enemies[0].souls, Some(4321));
        let cats: Vec<_> = vm.enemies[0].items.iter().map(|i| i.category).collect();
        assert_eq!(
            cats,
            [
                Some(ItemCategory::Weapon),
                Some(ItemCategory::Vitality),
                Some(ItemCategory::Spirit),
                None
            ]
        );
        assert!(vm.show_souls());
        // With LS-08 off, the souls column is gone.
        let ls = project_state(&gs, &Whitelist::default());
        let vm = live_vm(&input(&ls), &fb());
        assert_eq!(vm.enemies[0].souls, None);
        assert!(!vm.show_souls());
    }
}
