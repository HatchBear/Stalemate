//! TST-063 style tests on hand-built synthetic matches (no real match data).

use super::*;
use crate::api_client::MatchMetadata;
use crate::settings::Rules;
use crate::storage::{GameMode, MatchResult, Storage};
use serde_json::{json, Value};
use std::path::Path;

const ME: u32 = 100;

fn rules() -> Rules {
    Rules::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml")).unwrap()
}

fn stat(t: u32, nw: u32, lane: u32, neutrals: u32, brk: u32, sac: u32) -> Value {
    json!({"time_stamp_s": t, "net_worth": nw,
      "gold_source_lane_creeps_gold": lane, "gold_source_neutrals_gold": neutrals,
      "gold_source_breakable_gold": brk, "gold_source_item_cultist_sacrifice_gold": sac,
      "gold_source_players_gold": 10, "gold_source_assists_gold": 20,
      "gold_source_denies_gold": 30, "gold_source_bosses_gold": 40,
      "gold_source_team_bonus_gold": 50, "gold_source_treasure_gold": 60,
      "gold_source_item_trophy_collector_gold": 70, "gold_source_item_goose_egg_gold": 80,
      "gold_source_ability_assassinate_gold": 90})
}

fn player(id: u32, team: &str, lane: i32, stats: Vec<Value>) -> Value {
    json!({"account_id": id, "hero_id": id, "team": team, "assigned_lane": lane, "stats": stats})
}

/// 12 players. Lane 1: me (Team0) against 101 and 102 (Team1). Others fill lanes 4 and 6.
fn match_json(nw_540: [u32; 3], mode: &str, duration: u32) -> Value {
    let mut players = vec![
        player(
            ME,
            "Team0",
            1,
            vec![
                stat(360, 3000, 1, 1, 1, 1),
                stat(540, nw_540[0], 1, 1, 1, 1),
                // 450 + 150 + 30 + 20 + 450 (other) = 1100 total
                stat(duration, 20000, 450, 150, 30, 20),
            ],
        ),
        player(101, "Team1", 1, vec![stat(540, nw_540[1], 0, 0, 0, 0)]),
        player(102, "Team1", 1, vec![stat(540, nw_540[2], 0, 0, 0, 0)]),
    ];
    for (i, (team, lane)) in [
        ("Team0", 4),
        ("Team0", 4),
        ("Team0", 6),
        ("Team0", 6),
        ("Team0", 1),
        ("Team1", 4),
        ("Team1", 4),
        ("Team1", 6),
        ("Team1", 6),
    ]
    .iter()
    .enumerate()
    {
        players.push(player(
            200 + i as u32,
            team,
            *lane,
            vec![stat(540, 5000, 0, 0, 0, 0)],
        ));
    }
    json!({"match_id": 777, "start_time": "2026-11-01 12:30:00", "duration_s": duration,
      "winning_team": "Team0", "match_mode": "Unranked", "game_mode": mode, "players": players})
}

fn meta(v: Value) -> MatchMetadata {
    serde_json::from_value(v).unwrap()
}

fn lane_of(r: &StatsReview) -> &LaneLead {
    match &r.lane {
        LaneSection::Available(l) => l,
        other => panic!("lane unavailable: {other:?}"),
    }
}

#[test]
fn tst_063_lane_lead_and_soul_rows_match_hand_calculation() {
    // Opponents 6000 and 7000: mean 6500. Me 5000: lead -1500.
    let m = meta(match_json([5000, 6000, 7000], "Normal", 1800));
    let r = build_stats_review(&m, ME, &rules()).unwrap();
    assert_eq!(r.result, ReviewResult::Win);
    assert_eq!(r.hero_id, ME);
    assert!(r.standard_mode);
    let l = lane_of(&r);
    assert_eq!(l.t_game_s, 540);
    assert_eq!(l.user_net_worth, 5000);
    assert_eq!(l.opponent_count, 2);
    assert_eq!(l.opponent_mean_net_worth, 6500.0);
    assert_eq!(l.lead, -1500.0);

    let s = r.soul_sources.unwrap();
    // Other: 10+20+...+90 = 450.
    assert_eq!(s.total, 450 + 150 + 30 + 20 + 450);
    let souls: Vec<Option<u64>> = s.rows.iter().map(|r| r.souls).collect();
    assert_eq!(
        souls,
        [Some(450), Some(150), Some(30), Some(20), None, Some(450)]
    );
    assert_eq!(s.rows[4].source, SoulSource::Urn);
    assert_eq!(s.rows[4].share_pct, None);
    assert_eq!(s.rows[0].share_pct, Some(450.0 * 100.0 / 1100.0));
    assert_eq!(s.rows.iter().filter_map(|r| r.souls).sum::<u64>(), s.total);
}

#[test]
fn r02_fires_at_exactly_the_threshold_and_not_one_below() {
    let rules = rules();
    assert_eq!(rules.review.r02_lane_deficit, 1500);
    // Lead exactly -1500.
    let r = build_stats_review(
        &meta(match_json([5000, 6000, 7000], "Normal", 1800)),
        ME,
        &rules,
    )
    .unwrap();
    let f = r.r02.expect("fires at the threshold");
    assert_eq!((f.t_game_s, f.deficit, f.threshold), (540, 1500.0, 1500));
    // Lead -1499.
    let r = build_stats_review(
        &meta(match_json([5001, 6000, 7000], "Normal", 1800)),
        ME,
        &rules,
    )
    .unwrap();
    assert!(r.r02.is_none());
}

#[test]
fn missing_540_sample_is_interpolated() {
    let mut v = match_json([0, 6000, 7000], "Normal", 1800);
    // Me: samples at 360 (3000) and 900 (6000) only: 540 is 1/3 of the way, so 4000.
    v["players"][0]["stats"] = json!([stat(360, 3000, 0, 0, 0, 0), stat(900, 6000, 0, 0, 0, 0)]);
    // Opponent 101: 360 -> 5000, 720 -> 6000: 540 is halfway, so 5500. Opponent 102 stays at 7000.
    v["players"][1]["stats"] = json!([stat(360, 5000, 0, 0, 0, 0), stat(720, 6000, 0, 0, 0, 0)]);
    let r = build_stats_review(&meta(v), ME, &rules()).unwrap();
    let l = lane_of(&r);
    assert_eq!(l.user_net_worth, 4000);
    assert_eq!(l.opponent_mean_net_worth, 6250.0);
    assert_eq!(l.lead, -2250.0);
    assert!(r.r02.is_some());
}

#[test]
fn no_lane_opponents_gives_no_lane_section() {
    let mut v = match_json([5000, 6000, 7000], "Normal", 1800);
    v["players"][1]["assigned_lane"] = json!(4);
    v["players"][2]["assigned_lane"] = json!(4);
    for i in 3..12 {
        if v["players"][i]["team"] == "Team1" {
            v["players"][i]["assigned_lane"] = json!(6);
        }
    }
    let r = build_stats_review(&meta(v), ME, &rules()).unwrap();
    assert_eq!(
        r.lane,
        LaneSection::Unavailable {
            reason: LaneUnavailable::NoLaneOpponents
        }
    );
    assert!(r.r02.is_none());
    assert!(r.soul_sources.is_some());
}

#[test]
fn match_shorter_than_lane_phase_has_no_lane_but_is_flagged_short() {
    let r = build_stats_review(
        &meta(match_json([5000, 6000, 7000], "Normal", 500)),
        ME,
        &rules(),
    )
    .unwrap();
    assert_eq!(
        r.lane,
        LaneSection::Unavailable {
            reason: LaneUnavailable::MatchTooShort
        }
    );
    assert!(r.flags.short_match);
    assert!(!r.flags.abandoned);
    assert!(r.soul_sources.is_some());
}

#[test]
fn abandoned_user_is_flagged() {
    let mut v = match_json([5000, 6000, 7000], "Normal", 1800);
    v["players"][0]["abandon_match_time_s"] = json!(900);
    let r = build_stats_review(&meta(v), ME, &rules()).unwrap();
    assert_eq!(r.result, ReviewResult::Abandoned);
    assert!(r.flags.abandoned);
}

#[test]
fn non_standard_match_gets_a_record_but_no_review() {
    let m = meta(match_json([5000, 6000, 7000], "StreetBrawl", 1800));
    let r = build_stats_review(&m, ME, &rules()).unwrap();
    assert!(!r.standard_mode);
    assert!(r.soul_sources.is_none() && r.r02.is_none());
    let s = Storage::open_in_memory().unwrap();
    persist_stats_review(&s, &m, ME, &r, "p1").unwrap();
    assert_eq!(
        s.get_match(777).unwrap().unwrap().game_mode,
        GameMode::StreetBrawl
    );
    assert_eq!(count(&s, "reviews"), 0);
}

fn stored_mode(s: &Storage) -> String {
    s.with_conn(|c| {
        c.query_row(
            "SELECT game_mode FROM matches WHERE match_id = 777",
            [],
            |r| r.get(0),
        )
    })
    .unwrap()
}

#[test]
fn unknown_mode_is_stored_as_its_own_name_with_no_review() {
    let m = meta(match_json([5000, 6000, 7000], "SomeNewMode", 1800));
    let r = build_stats_review(&m, ME, &rules()).unwrap();
    let s = Storage::open_in_memory().unwrap();
    persist_stats_review(&s, &m, ME, &r, "p1").unwrap();
    assert_eq!(stored_mode(&s), "some_new_mode");
    assert_eq!(count(&s, "matches"), 1);
    assert_eq!(count(&s, "reviews"), 0);
}

#[test]
fn known_modes_are_stored_snake_case() {
    for (api, want) in [("Normal", "normal"), ("StreetBrawl", "street_brawl")] {
        let m = meta(match_json([5000, 6000, 7000], api, 1800));
        let r = build_stats_review(&m, ME, &rules()).unwrap();
        let s = Storage::open_in_memory().unwrap();
        persist_stats_review(&s, &m, ME, &r, "p1").unwrap();
        assert_eq!(stored_mode(&s), want);
    }
}

#[test]
fn user_not_in_match_is_none() {
    let m = meta(match_json([5000, 6000, 7000], "Normal", 1800));
    assert!(build_stats_review(&m, 5, &rules()).is_none());
}

fn count(s: &Storage, table: &str) -> i64 {
    s.with_conn(|c| c.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0)))
        .unwrap()
}

#[test]
fn persist_twice_keeps_one_row_each_and_delete_cascades() {
    let m = meta(match_json([5000, 6000, 7000], "Normal", 1800));
    let rules = rules();
    let r = build_stats_review(&m, ME, &rules).unwrap();
    let s = Storage::open_in_memory().unwrap();
    persist_stats_review(&s, &m, ME, &r, "2026-10-30").unwrap();
    persist_stats_review(&s, &m, ME, &r, "2026-10-30").unwrap();
    assert_eq!(count(&s, "matches"), 1);
    assert_eq!(count(&s, "match_players"), 12);
    assert_eq!(count(&s, "reviews"), 1);
    assert_eq!(count(&s, "review_findings"), 1);

    let row = s.get_match(777).unwrap().unwrap();
    assert_eq!(row.result, Some(MatchResult::Win));
    assert_eq!(row.started_at, "2026-11-01T12:30:00Z");
    assert_eq!(row.match_mode.as_deref(), Some("unranked"));
    assert_eq!(row.patch, "2026-10-30");
    let players = s.get_match_players(777).unwrap();
    assert!(players
        .iter()
        .all(|p| p.party_id.is_none() && p.assigned_lane.is_some()));

    let (acc, tier, provider, partial, facts): (Option<i64>, Option<String>, String, i64, String) =
        s.with_conn(|c| {
            c.query_row(
                "SELECT accuracy, tier_counts, llm_provider, partial, facts_json FROM reviews",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?)),
            )
        })
        .unwrap();
    assert_eq!(
        (acc, tier, provider.as_str(), partial),
        (None, None, "off", 0)
    );
    assert_eq!(facts, serde_json::to_string(&r).unwrap());

    // A rerun that no longer fires R-02 removes the finding.
    let m2 = meta(match_json([5001, 6000, 7000], "Normal", 1800));
    let r2 = build_stats_review(&m2, ME, &rules).unwrap();
    persist_stats_review(&s, &m2, ME, &r2, "2026-10-30").unwrap();
    assert_eq!(count(&s, "review_findings"), 0);
    assert_eq!(count(&s, "reviews"), 1);

    assert!(s.delete_match(777).unwrap());
    assert_eq!(count(&s, "reviews"), 0);
    assert_eq!(count(&s, "review_findings"), 0);
}

#[test]
fn three_runs_give_identical_json() {
    let m = meta(match_json([5000, 6000, 7000], "Normal", 1800));
    let rules = rules();
    let runs: Vec<String> = (0..3)
        .map(|_| serde_json::to_string(&build_stats_review(&m, ME, &rules).unwrap()).unwrap())
        .collect();
    assert_eq!(runs[0], runs[1]);
    assert_eq!(runs[1], runs[2]);
}
