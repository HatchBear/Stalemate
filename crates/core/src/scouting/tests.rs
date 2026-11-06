//! TC-08-1 to 08-3, TST-021 and TST-022 style checks. Synthetic data only.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::*;

fn rules() -> Rules {
    let p = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
    Rules::load(&p).unwrap()
}

fn entry(id: u32, hero: u32, team: u8) -> RosterEntry {
    RosterEntry {
        account_id: id,
        name: format!("p{id}"),
        team,
        hero_id: hero,
    }
}

fn hist(t: u64, mode: i32, win: bool, delta: Option<i32>) -> MatchHistoryEntry {
    serde_json::from_value(serde_json::json!({
        "account_id": 1, "match_id": t, "hero_id": 1, "hero_level": 1, "start_time": t,
        "match_duration_s": 1, "match_result": if win { 1 } else { 0 }, "player_team": 1,
        "player_match_outcome": 1, "game_mode": mode, "match_mode": 1, "player_kills": 0,
        "player_deaths": 0, "player_assists": 0, "last_hits": 0, "denies": 0, "net_worth": 0,
        "objectives_mask_team0": 0, "objectives_mask_team1": 0, "ranked_delta": delta
    }))
    .unwrap()
}

fn hs(id: u32, hero: u32, matches: u32, wins: u32) -> PlayerHeroStats {
    serde_json::from_value(serde_json::json!({
        "account_id": id, "hero_id": hero, "matches_played": matches, "wins": wins
    }))
    .unwrap()
}

#[derive(Default)]
struct Mock {
    latency: Duration,
    opted_out: Vec<u32>,
    busy: Vec<u32>,
    history: HashMap<u32, Vec<MatchHistoryEntry>>,
    stats: Vec<PlayerHeroStats>,
    ranks: Vec<AccountRank>,
    history_calls: std::sync::Mutex<HashMap<u32, u32>>,
    batch_calls: AtomicU32,
}

#[async_trait]
impl ScoutingSource for Mock {
    async fn match_history(&self, id: u32) -> Result<Vec<MatchHistoryEntry>, ApiError> {
        *self.history_calls.lock().unwrap().entry(id).or_default() += 1;
        tokio::time::sleep(self.latency).await;
        if self.opted_out.contains(&id) {
            return Err(ApiError::NoPublicData);
        }
        if self.busy.contains(&id) {
            return Err(ApiError::Busy {
                retry_after_s: 7,
                timeout: false,
            });
        }
        Ok(self.history.get(&id).cloned().unwrap_or_default())
    }
    async fn player_ranks(&self, _: &[u32]) -> Result<Vec<AccountRank>, ApiError> {
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.latency).await;
        Ok(self.ranks.clone())
    }
    async fn player_hero_stats(&self, _: &[u32]) -> Result<Vec<PlayerHeroStats>, ApiError> {
        self.batch_calls.fetch_add(1, Ordering::SeqCst);
        tokio::time::sleep(self.latency).await;
        Ok(self.stats.clone())
    }
}

async fn run(
    mock: Mock,
    roster: &[RosterEntry],
    me: u32,
) -> (HashMap<u32, ScoutingCard>, Arc<Mock>) {
    let mock = Arc::new(mock);
    let (tx, mut rx) = mpsc::unbounded_channel();
    ScoutingService::new(mock.clone())
        .scout(roster, me, &rules(), tx)
        .await;
    let mut last = HashMap::new();
    let mut order = Vec::new();
    while let Ok(c) = rx.try_recv() {
        order.push(c.clone());
        last.insert(c.account_id, c);
    }
    // The first message for every real player is Loading (progressive fill).
    assert!(order.iter().take(1).all(|c| c.state != CardState::Ready));
    (last, mock)
}

fn eleven() -> Vec<RosterEntry> {
    (1..=12).map(|i| entry(i, 100 + i, (i % 2) as u8)).collect()
}

#[tokio::test(start_paused = true)]
async fn tc_08_1_eleven_cards_ready_fast() {
    let mock = Mock {
        latency: Duration::from_millis(200),
        ..Default::default()
    };
    let start = tokio::time::Instant::now();
    let (cards, mock) = run(mock, &eleven(), 1).await;
    assert!(start.elapsed() <= Duration::from_secs(30));
    assert_eq!(cards.len(), 11, "the user is skipped");
    assert!(!cards.contains_key(&1));
    assert!(cards.values().all(|c| c.state == CardState::Ready));
    // Batch calls happen once for all players.
    assert_eq!(mock.batch_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(start_paused = true)]
async fn tc_08_2_opt_out_is_shown_and_not_retried() {
    let mock = Mock {
        opted_out: vec![5],
        stats: vec![hs(5, 105, 50, 25), hs(6, 106, 10, 5)],
        ..Default::default()
    };
    let (cards, mock) = run(mock, &eleven(), 1).await;
    let c = &cards[&5];
    assert_eq!(c.state, CardState::NoPublicData);
    assert!(c.wl_overall.is_none() && c.mains.is_empty() && c.hero_matches.is_none());
    assert_eq!(mock.history_calls.lock().unwrap()[&5], 1);
    assert_eq!(cards[&6].state, CardState::Ready);
    assert_eq!(cards[&6].hero_matches, Some(10));
}

#[tokio::test(start_paused = true)]
async fn busy_shows_error_with_retry_time_and_others_fill() {
    let mock = Mock {
        busy: vec![3],
        ..Default::default()
    };
    let (cards, _) = run(mock, &eleven(), 1).await;
    assert_eq!(
        cards[&3].state,
        CardState::Error {
            source: "match-history".into(),
            retry_after_s: Some(7)
        }
    );
    assert_eq!(cards[&4].state, CardState::Ready);
}

#[test]
fn tc_08_3_slope() {
    let series: Vec<f64> = (0..20).map(|i| 1000.0 + 10.0 * i as f64).collect();
    assert!((slope_per_10(&series).unwrap() - 100.0).abs() < 1e-9);
    assert_eq!(slope_per_10(&[5.0]), None);
    assert_eq!(slope_per_10(&[]), None);
}

#[test]
fn mmr_trend_uses_recent_ranked_oldest_first() {
    // Ten matches of +10 each; the window keeps only the newest 4 (oldest dropped).
    let h: Vec<_> = (0..10).map(|i| hist(i, 1, true, Some(10))).collect();
    // Series 0,10,20,30,40: slope 10 per match -> 100 per 10.
    assert!((mmr_trend(&h, 4).unwrap() - 100.0).abs() < 1e-9);
    let none: Vec<_> = (0..5).map(|i| hist(i, 1, true, None)).collect();
    assert_eq!(mmr_trend(&none, 20), None);
}

#[test]
fn recent_wl_counts_standard_only_and_labels_n() {
    let r = rules();
    let mut h = vec![
        hist(1, 1, true, None),
        hist(2, 1, false, None),
        hist(3, 1, true, None),
    ];
    h.push(hist(4, 2, true, None)); // other mode: ignored
    let w = recent_wl(&h, r.scouting.history_window, &ScoutingModes::default());
    assert_eq!((w.wins, w.matches, w.window), (2, 3, 20));
    // Window cuts to the most recent matches.
    let w = recent_wl(&h, 2, &ScoutingModes::default());
    assert_eq!((w.wins, w.matches), (1, 2));
}

#[tokio::test(start_paused = true)]
async fn card_fields_match_hand_calculation() {
    let mut history = HashMap::new();
    history.insert(
        2,
        vec![
            hist(1, 1, true, Some(20)),
            hist(2, 1, false, Some(-10)),
            hist(3, 1, true, None),
        ],
    );
    let stats = vec![
        hs(2, 102, 30, 18), // roster hero
        hs(2, 7, 50, 20),
        hs(2, 8, 10, 5),
        hs(2, 9, 5, 5),
        hs(2, 10, 4, 1),
        hs(2, 11, 3, 1),
        hs(2, 12, 2, 0),
    ];
    let ranks = vec![AccountRank {
        account_id: 2,
        badge: 53,
        rank: 5,
        subrank: 3,
        last_match: None,
    }];
    let mock = Mock {
        history,
        stats,
        ranks,
        ..Default::default()
    };
    let (cards, _) = run(mock, &eleven(), 1).await;
    let c = &cards[&2];
    assert_eq!(c.state, CardState::Ready);
    assert_eq!(c.rank_badge, Some(53));
    assert_eq!(
        c.wl_overall,
        Some(WlOverall {
            wins: 50,
            matches: 104
        })
    );
    assert_eq!(c.hero_matches, Some(30));
    assert_eq!(c.mains.len(), 5);
    assert_eq!((c.mains[0].hero_id, c.mains[0].matches), (7, 50));
    assert!((c.mains[0].win_rate - 0.4).abs() < 1e-9);
    assert_eq!(c.mains[1].hero_id, 102);
    assert_eq!(
        c.wl_recent,
        Some(WlRecent {
            wins: 2,
            matches: 3,
            window: 20
        })
    );
    // Deltas oldest first: +20, -10 -> series 0,20,10 -> slope 5 per match.
    assert!((c.mmr_trend_per_10.unwrap() - 50.0).abs() < 1e-9);
    // A player with no rank row has no badge.
    assert_eq!(cards[&3].rank_badge, None);
}

#[tokio::test(start_paused = true)]
async fn bot_slot_is_hero_only_and_makes_no_calls() {
    let roster = vec![entry(1, 101, 0), entry(0, 150, 1)];
    let (cards, mock) = run(Mock::default(), &roster, 1).await;
    assert_eq!(cards[&0].state, CardState::HeroOnly);
    assert_eq!(cards[&0].hero_id, 150);
    assert!(mock.history_calls.lock().unwrap().is_empty());
    assert_eq!(mock.batch_calls.load(Ordering::SeqCst), 0);
}

#[test]
fn fp_10_hidden_at_zero_visible_at_one() {
    let r = rules();
    assert!(!scouting_visible(0, &r));
    assert!(scouting_visible(1, &r));
}
