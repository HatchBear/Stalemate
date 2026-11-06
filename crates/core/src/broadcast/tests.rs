//! PRG-04 tests: TC-04-2, TC-04-3, TC-04-4 against a mock fragment source with synthetic bytes,
//! the TC-04-1 golden test (needs a recorded fixture, `--features fixtures`) and one live smoke test.

use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
    time::Duration,
};

use async_trait::async_trait;
use bytes::Bytes;
use tokio::{sync::mpsc, time::Instant};

use super::*;
use crate::types::RosterEntry;

fn rules() -> BroadcastRules {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
    crate::settings::Rules::load(&p).unwrap().broadcast
}

// ---------------------------------------------------------------------------------------------
// Mock relay
// ---------------------------------------------------------------------------------------------

/// A locator that always answers the same URL and counts calls.
#[derive(Default)]
struct FixedLocator {
    located: Mutex<Vec<Instant>>,
    failures: Mutex<Vec<bool>>,
}

#[async_trait]
impl UrlLocator for FixedLocator {
    async fn locate(&self, _match_id: MatchId) -> Result<String, LocateError> {
        self.located.lock().unwrap().push(Instant::now());
        Ok("http://relay.test/tv/1_1".into())
    }
    fn report_failure(&self, _match_id: MatchId, wrong_match: bool) {
        self.failures.lock().unwrap().push(wrong_match);
    }
}

/// What one connection attempt does.
enum Attempt {
    /// Deliver fragments `from..=to` (synthetic), then end with this.
    Serve { from: u32, to: u32, then: Ending },
}

#[derive(Clone, Copy)]
enum Ending {
    Drop,
    Close,
    Finish,
}

/// Synthetic fragment `n`: `n` bytes long plus 3, so lengths differ.
fn fragment(n: u32) -> Bytes {
    Bytes::from(vec![n as u8; n as usize + 3])
}

fn roster() -> ParserEvent {
    ParserEvent::Roster {
        players: vec![RosterEntry {
            account_id: 1,
            name: "a".into(),
            team: 0,
            hero_id: 7,
        }],
    }
}

/// What the "parser" makes of synthetic fragment `n`: the full current state each time, like the
/// real one, so a reconnect re-sends things already sent.
fn events_of(n: u32, last: bool) -> Vec<ParserEvent> {
    let mut v = vec![
        roster(),
        ParserEvent::Kda {
            account_id: 1,
            kills: (n / 3) as u16,
            deaths: 0,
            assists: 0,
        },
        ParserEvent::Tick { t_game: n },
    ];
    if last {
        v.push(ParserEvent::End { winning_team: 0 });
    }
    v
}

struct MockSession {
    next: u32,
    to: u32,
    then: Ending,
}

#[async_trait]
impl Session for MockSession {
    async fn next_step(&mut self) -> Result<Option<Step>, SessionError> {
        if self.next > self.to {
            return match self.then {
                Ending::Drop => Err(SessionError::Stream("connection reset".into())),
                Ending::Close => Ok(None),
                Ending::Finish => unreachable!("Finish ends with an End event"),
            };
        }
        let n = self.next;
        self.next += 1;
        let last = n == self.to && matches!(self.then, Ending::Finish);
        Ok(Some(Step {
            fragments: vec![fragment(n)],
            events: events_of(n, last),
            delay_s: Some(25.0),
        }))
    }
}

struct MockConnector {
    script: Mutex<VecDeque<Attempt>>,
    /// Attempts refused until this long after the first served session ended, then the script.
    refuse_until: Mutex<Option<Duration>>,
    first_drop: Mutex<Option<Instant>>,
    connects: Mutex<Vec<Instant>>,
}

impl MockConnector {
    fn new(script: Vec<Attempt>) -> Self {
        Self {
            script: Mutex::new(script.into()),
            refuse_until: Mutex::new(None),
            first_drop: Mutex::new(None),
            connects: Mutex::new(Vec::new()),
        }
    }
    fn gaps(&self) -> Vec<u64> {
        let t = self.connects.lock().unwrap();
        t.windows(2).map(|w| (w[1] - w[0]).as_secs()).collect()
    }
}

#[async_trait]
impl Connector for MockConnector {
    async fn connect(&self, _url: &str, _id: MatchId) -> Result<Box<dyn Session>, SessionError> {
        let now = Instant::now();
        self.connects.lock().unwrap().push(now);
        if let (Some(window), Some(dropped)) = (
            *self.refuse_until.lock().unwrap(),
            *self.first_drop.lock().unwrap(),
        ) {
            if now - dropped < window {
                return Err(SessionError::Connect("relay unreachable".into()));
            }
        }
        match self.script.lock().unwrap().pop_front() {
            None => Err(SessionError::Connect("refused".into())),
            Some(Attempt::Serve { from, to, then }) => {
                if matches!(then, Ending::Drop) {
                    // The drop happens when this session ends, `to - from + 1` steps from now.
                    *self.first_drop.lock().unwrap() = Some(now);
                }
                Ok(Box::new(MockSession {
                    next: from,
                    to,
                    then,
                }))
            }
        }
    }
}

fn deps(
    connector: Arc<MockConnector>,
    locator: Arc<FixedLocator>,
    dir: Option<std::path::PathBuf>,
) -> BroadcastDeps {
    BroadcastDeps {
        locator,
        connector,
        rules: rules(),
        recordings_dir: dir,
    }
}

async fn collect(id: MatchId, deps: BroadcastDeps) -> (BroadcastOutcome, Vec<BroadcastEvent>) {
    let (tx, mut rx) = mpsc::channel(1024);
    let h = tokio::spawn(run(id, deps, tx));
    let mut out = Vec::new();
    while let Some(e) = rx.recv().await {
        out.push(e);
    }
    (h.await.unwrap(), out)
}

fn parser_events(evs: &[BroadcastEvent]) -> Vec<ParserEvent> {
    evs.iter()
        .filter_map(|e| match &e.kind {
            BroadcastKind::Parser(p) => Some(p.clone()),
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// TC-04-2, TC-04-3, TC-04-4
// ---------------------------------------------------------------------------------------------

/// TC-04-2 (TST-012): the server drops after 10 fragments and returns at 5 s. The client reconnects
/// and no event repeats.
#[tokio::test(start_paused = true)]
async fn tc_04_2_drop_after_10_fragments_reconnects_without_duplicates() {
    let conn = Arc::new(MockConnector::new(vec![
        Attempt::Serve {
            from: 1,
            to: 10,
            then: Ending::Drop,
        },
        // The relay comes back at the live edge a little behind what was already delivered, so
        // fragments 9 and 10 are read again.
        Attempt::Serve {
            from: 9,
            to: 14,
            then: Ending::Finish,
        },
    ]));
    *conn.refuse_until.lock().unwrap() = Some(Duration::from_secs(5));
    let loc = Arc::new(FixedLocator::default());
    let (outcome, evs) = collect(42, deps(conn.clone(), loc.clone(), None)).await;

    assert_eq!(outcome, BroadcastOutcome::Ended);
    // Attempts: 0 s ok, drop; 2 s refused; 6 s ok (the 4 s wait follows the second failure).
    assert_eq!(conn.gaps(), vec![2, 4]);
    let connected = evs
        .iter()
        .filter(|e| matches!(e.kind, BroadcastKind::Connected { .. }))
        .count();
    assert_eq!(connected, 2);

    let p = parser_events(&evs);
    // No duplicates at all.
    for (i, a) in p.iter().enumerate() {
        assert!(!p[..i].contains(a), "duplicate event {a:?}");
    }
    // Every game second 1..=14 appears exactly once, in order.
    let ticks: Vec<u32> = p
        .iter()
        .filter_map(|e| match e {
            ParserEvent::Tick { t_game } => Some(*t_game),
            _ => None,
        })
        .collect();
    assert_eq!(ticks, (1..=14).collect::<Vec<_>>());
    assert_eq!(
        p.iter()
            .filter(|e| matches!(e, ParserEvent::Roster { .. }))
            .count(),
        1
    );
    assert!(matches!(p.last(), Some(ParserEvent::End { .. })));
    assert!(matches!(evs.last().unwrap().kind, BroadcastKind::Ended));
    // The locator was told about the failure that preceded each retry.
    assert_eq!(loc.failures.lock().unwrap().len(), 2);
}

/// TC-04-3 (TST-012, NFR-REL-001): the server never returns. Retries at 2, 4, 8, 16 and 32 s, then
/// `Lost`.
#[tokio::test(start_paused = true)]
async fn tc_04_3_server_never_returns_retries_on_schedule_then_lost() {
    let conn = Arc::new(MockConnector::new(vec![]));
    let loc = Arc::new(FixedLocator::default());
    let t0 = Instant::now();
    let (outcome, evs) = collect(1, deps(conn.clone(), loc, None)).await;
    assert_eq!(outcome, BroadcastOutcome::Lost);
    assert_eq!(conn.gaps(), vec![2, 4, 8, 16, 32]);
    assert_eq!(conn.connects.lock().unwrap().len(), 6);
    assert_eq!(Instant::now() - t0, Duration::from_secs(62));
    assert_eq!(evs.len(), 1);
    assert!(matches!(evs[0].kind, BroadcastKind::Lost));
}

#[tokio::test(start_paused = true)]
async fn steps_reset_the_failure_count() {
    // Four short sessions, each dropping after one fragment: every drop is the first failure, so
    // the wait is always 2 s and the client never gives up.
    let mut script = Vec::new();
    for i in 0..4 {
        script.push(Attempt::Serve {
            from: i + 1,
            to: i + 1,
            then: Ending::Drop,
        });
    }
    script.push(Attempt::Serve {
        from: 5,
        to: 5,
        then: Ending::Finish,
    });
    let conn = Arc::new(MockConnector::new(script));
    let (outcome, _) = collect(
        1,
        deps(conn.clone(), Arc::new(FixedLocator::default()), None),
    )
    .await;
    assert_eq!(outcome, BroadcastOutcome::Ended);
    assert_eq!(conn.gaps(), vec![2, 2, 2, 2]);
}

/// TC-04-4 (TST-014, FR-DATA-005): the recording's length equals the sum of the fragment lengths,
/// and the bytes are the fragments in order, across a reconnect.
#[tokio::test(start_paused = true)]
async fn tc_04_4_recording_length_is_the_sum_of_fragment_lengths() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Arc::new(MockConnector::new(vec![
        Attempt::Serve {
            from: 1,
            to: 4,
            then: Ending::Close,
        },
        Attempt::Serve {
            from: 5,
            to: 8,
            then: Ending::Finish,
        },
    ]));
    let (outcome, _) = collect(
        4242,
        deps(
            conn,
            Arc::new(FixedLocator::default()),
            Some(dir.path().to_path_buf()),
        ),
    )
    .await;
    assert_eq!(outcome, BroadcastOutcome::Ended);
    let expected: Vec<u8> = (1..=8).flat_map(|n| fragment(n).to_vec()).collect();
    let got = std::fs::read(recording_path(dir.path(), 4242)).unwrap();
    assert_eq!(got.len(), (1..=8).map(|n| fragment(n).len()).sum::<usize>());
    assert_eq!(got, expected);
}

#[tokio::test(start_paused = true)]
async fn no_recordings_dir_means_nothing_is_written() {
    let dir = tempfile::tempdir().unwrap();
    let conn = Arc::new(MockConnector::new(vec![Attempt::Serve {
        from: 1,
        to: 3,
        then: Ending::Finish,
    }]));
    collect(7, deps(conn, Arc::new(FixedLocator::default()), None)).await;
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
}

#[tokio::test(start_paused = true)]
async fn every_event_carries_a_wall_time_and_delay_is_sent_once_per_value() {
    let conn = Arc::new(MockConnector::new(vec![Attempt::Serve {
        from: 1,
        to: 5,
        then: Ending::Finish,
    }]));
    let before = time::OffsetDateTime::now_utc();
    let (_, evs) = collect(1, deps(conn, Arc::new(FixedLocator::default()), None)).await;
    let after = time::OffsetDateTime::now_utc();
    assert!(evs
        .iter()
        .all(|e| e.received_at >= before && e.received_at <= after));
    // The mock reports 25 s every time: one Delay event, not five.
    let delays: Vec<f32> = evs
        .iter()
        .filter_map(|e| match e.kind {
            BroadcastKind::Delay { secs } => Some(secs),
            _ => None,
        })
        .collect();
    assert_eq!(delays, vec![25.0]);
}

#[tokio::test(start_paused = true)]
async fn a_wrong_match_is_reported_to_the_locator() {
    struct WrongMatch;
    #[async_trait]
    impl Connector for WrongMatch {
        async fn connect(&self, _: &str, _: MatchId) -> Result<Box<dyn Session>, SessionError> {
            Err(SessionError::WrongMatch)
        }
    }
    let loc = Arc::new(FixedLocator::default());
    let (outcome, _) = collect(
        1,
        BroadcastDeps {
            locator: loc.clone(),
            connector: Arc::new(WrongMatch),
            rules: rules(),
            recordings_dir: None,
        },
    )
    .await;
    assert_eq!(outcome, BroadcastOutcome::Lost);
    assert!(loc.failures.lock().unwrap().iter().all(|w| *w));
}

#[tokio::test(start_paused = true)]
async fn dropping_the_receiver_stops_the_client() {
    let conn = Arc::new(MockConnector::new(vec![]));
    let (tx, rx) = mpsc::channel(8);
    let h = spawn(1, deps(conn, Arc::new(FixedLocator::default()), None), tx);
    drop(rx);
    assert_eq!(h.await.unwrap(), BroadcastOutcome::Stopped);
}

/// Without the `broadcast` feature the connector is a stub: the client reports `Lost` at once
/// instead of waiting out the backoff.
#[cfg(not(feature = "broadcast"))]
#[tokio::test(start_paused = true)]
async fn without_the_feature_the_client_reports_lost_immediately() {
    let d = BroadcastDeps {
        locator: Arc::new(FixedLocator::default()),
        connector: Arc::new(HasteConnector::new(None, Duration::from_secs(1))),
        rules: rules(),
        recordings_dir: None,
    };
    let t0 = Instant::now();
    let (outcome, evs) = collect(1, d).await;
    assert_eq!(outcome, BroadcastOutcome::Lost);
    assert_eq!(Instant::now() - t0, Duration::ZERO);
    assert!(matches!(evs[0].kind, BroadcastKind::Lost));
}

// ---------------------------------------------------------------------------------------------
// Live smoke test
// ---------------------------------------------------------------------------------------------

/// Connects to a currently listed public Broadcast for about a minute and checks the invariants the
/// rest of the app relies on. Network required: `cargo test -p stalemate-core live_smoke --
/// --ignored --nocapture`.
///
/// It uses the free listing only and never calls the per-match locator. It prints counts, never
/// names or IDs, and stores nothing.
#[cfg(feature = "broadcast")]
#[tokio::test]
#[ignore = "needs the network and a live public match"]
async fn live_smoke_listed_broadcast() {
    use crate::api_client::{ApiClient, ApiConfig};
    use std::collections::{HashMap, HashSet};

    let rules_all = {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
        crate::settings::Rules::load(&p).unwrap()
    };
    let api = ApiClient::new(
        crate::storage::Storage::open_in_memory().unwrap(),
        &rules_all.api,
        ApiConfig::default(),
    )
    .unwrap();
    let catalog = api.load_catalog().await.expect("asset catalog");
    let mut listed = api.live_urls().await.expect("listing");
    listed.sort_by_key(|l| std::cmp::Reverse(l.updated_at));
    println!("listed broadcasts: {}", listed.len());

    // Take the first listed Broadcast that answers /sync (about a third are "not started").
    let connector = HasteConnector::new(
        None,
        Duration::from_secs(rules_all.broadcast.request_timeout_s),
    );
    let mut chosen = None;
    for l in &listed {
        if let Ok(s) = connector.connect(&l.broadcast_url, l.match_id).await {
            drop(s);
            chosen = Some(l.clone());
            break;
        }
    }
    let chosen = chosen.expect("at least one listed Broadcast answers /sync");

    struct Fixed(String);
    #[async_trait]
    impl UrlLocator for Fixed {
        async fn locate(&self, _: MatchId) -> Result<String, LocateError> {
            Ok(self.0.clone())
        }
        fn report_failure(&self, _: MatchId, _: bool) {}
    }
    let d = BroadcastDeps {
        locator: Arc::new(Fixed(chosen.broadcast_url.clone())),
        connector: Arc::new(connector),
        rules: rules_all.broadcast.clone(),
        recordings_dir: None, // not our match: no recording
    };
    let (tx, mut rx) = mpsc::channel(4096);
    let h = spawn(chosen.match_id, d, tx);
    let started = std::time::Instant::now();
    let deadline = tokio::time::sleep(Duration::from_secs(60));
    tokio::pin!(deadline);
    let mut evs: Vec<BroadcastEvent> = Vec::new();
    loop {
        tokio::select! {
            () = &mut deadline => break,
            e = rx.recv() => match e { Some(e) => evs.push(e), None => break },
        }
    }
    drop(rx);
    let _ = h.await;
    println!("observed for {:?}, {} events", started.elapsed(), evs.len());

    let p = parser_events(&evs);
    let rosters: Vec<&Vec<RosterEntry>> = p
        .iter()
        .filter_map(|e| match e {
            ParserEvent::Roster { players } => Some(players),
            _ => None,
        })
        .collect();
    assert!(!rosters.is_empty(), "no complete roster arrived");
    let r = rosters.last().unwrap();
    let teams: HashSet<u8> = r.iter().map(|x| x.team).collect();
    println!(
        "rosters sent: {}, players: {}, teams seen: {:?}",
        rosters.len(),
        r.len(),
        {
            let mut t: Vec<_> = teams.iter().collect();
            t.sort();
            t
        }
    );
    assert_eq!(r.len(), 12);
    assert!(teams.is_subset(&HashSet::from([0, 1])) && teams.len() == 2);
    let per_team = |t| r.iter().filter(|x| x.team == t).count();
    println!("team sizes: {} / {}", per_team(0), per_team(1));

    // Latest inventory per player, resolved against the asset catalog.
    let mut latest: HashMap<u32, &Vec<u32>> = HashMap::new();
    for e in &p {
        if let ParserEvent::Inventory { account_id, items } = e {
            latest.insert(*account_id, items);
        }
    }
    let total: usize = latest.values().map(|v| v.len()).sum();
    let hits: usize = latest
        .values()
        .flat_map(|v| v.iter())
        .filter(|id| catalog.item(**id).is_some())
        .count();
    println!(
        "inventory: {} players, {} items, {} resolve in the catalog ({:.1}%)",
        latest.len(),
        total,
        hits,
        if total == 0 {
            0.0
        } else {
            100.0 * hits as f64 / total as f64
        }
    );
    assert_eq!(latest.len(), 12);

    let ticks: Vec<u32> = p
        .iter()
        .filter_map(|e| match e {
            ParserEvent::Tick { t_game } => Some(*t_game),
            _ => None,
        })
        .collect();
    println!(
        "ticks: {} (first {:?}, last {:?})",
        ticks.len(),
        ticks.first(),
        ticks.last()
    );
    assert!(
        ticks.windows(2).all(|w| w[0] < w[1]),
        "game time only moves forward"
    );
    assert!(
        ticks.len() >= 2 && ticks.last() > ticks.first(),
        "game time increases"
    );

    let delays: Vec<f32> = evs
        .iter()
        .filter_map(|e| match e.kind {
            BroadcastKind::Delay { secs } => Some(secs),
            _ => None,
        })
        .collect();
    let (dmin, dmax) = delays
        .iter()
        .fold((f32::MAX, f32::MIN), |(a, b), x| (a.min(*x), b.max(*x)));
    println!(
        "delay events: {}, last {:?}, min {dmin:.1}, max {dmax:.1}",
        delays.len(),
        delays.last()
    );
    let count = |f: fn(&ParserEvent) -> bool| p.iter().filter(|e| f(e)).count();
    println!(
        "events: kda {}, level {}, networth {}, inventory {}, mode {}, end {}",
        count(|e| matches!(e, ParserEvent::Kda { .. })),
        count(|e| matches!(e, ParserEvent::Level { .. })),
        count(|e| matches!(e, ParserEvent::NetWorth { .. })),
        count(|e| matches!(e, ParserEvent::Inventory { .. })),
        count(|e| matches!(e, ParserEvent::Mode { .. })),
        count(|e| matches!(e, ParserEvent::End { .. })),
    );
    assert!(hits > 0);
}

// ---------------------------------------------------------------------------------------------
// TC-04-1 (TST-012): recorded fixture through a local HTTP server, compared with a golden file
// ---------------------------------------------------------------------------------------------

/// Needs a real recorded match, which only the author can make (T-013), so it is behind
/// `--features fixtures` and skips cleanly when none is present.
///
/// Layout, next to the `events.jsonl` and `meta.json` that doc 14 (FX-BC) describes:
///
/// ```text
/// tests/fixtures/fx-bc/<match>/relay/sync            the relay's /sync JSON
/// tests/fixtures/fx-bc/<match>/relay/<n>_start       fragment n, served at /<n>/start
/// tests/fixtures/fx-bc/<match>/relay/<n>_full        served at /<n>/full
/// tests/fixtures/fx-bc/<match>/relay/<n>_delta       served at /<n>/delta
/// ```
///
/// The raw `relay/` files are never committed (DEC-056); only the golden `events.jsonl` is. A
/// fixture without a `relay/` folder is skipped.
#[cfg(all(feature = "fixtures", feature = "broadcast"))]
#[tokio::test]
async fn tc_04_1_recorded_fixture_gives_the_golden_events() {
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/fx-bc");
    let Ok(dirs) = std::fs::read_dir(&root) else {
        eprintln!("TC-04-1 skipped: no tests/fixtures/fx-bc (blocked on T-013)");
        return;
    };
    let mut ran = 0;
    for dir in dirs.flatten().map(|d| d.path()) {
        let relay = dir.join("relay");
        let golden = dir.join("events.jsonl");
        if !relay.join("sync").is_file() || !golden.is_file() {
            continue;
        }
        let meta: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join("meta.json")).unwrap()).unwrap();
        let match_id = meta["match_id"].as_u64().unwrap();
        let me = meta["me"].as_u64().map(|a| a as u32);

        let server = MockServer::start().await;
        let base = format!("/tv/{match_id}_1");
        Mock::given(method("GET"))
            .and(path(format!("{base}/sync")))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_bytes(std::fs::read(relay.join("sync")).unwrap()),
            )
            .mount(&server)
            .await;
        for f in std::fs::read_dir(&relay).unwrap().flatten() {
            let name = f.file_name().to_string_lossy().into_owned();
            let Some((n, kind)) = name.split_once('_') else {
                continue;
            };
            Mock::given(method("GET"))
                .and(path(format!("{base}/{n}/{kind}")))
                .respond_with(
                    ResponseTemplate::new(200).set_body_bytes(std::fs::read(f.path()).unwrap()),
                )
                .mount(&server)
                .await;
        }

        struct Fixed(String);
        #[async_trait]
        impl UrlLocator for Fixed {
            async fn locate(&self, _: MatchId) -> Result<String, LocateError> {
                Ok(self.0.clone())
            }
            fn report_failure(&self, _: MatchId, _: bool) {}
        }
        let mut r = rules();
        r.backoff_s.clear(); // the replay ends by running out of fragments: no retries
        let d = BroadcastDeps {
            locator: Arc::new(Fixed(format!("{}{base}", server.uri()))),
            connector: Arc::new(HasteConnector::new(
                me,
                Duration::from_secs(r.request_timeout_s),
            )),
            rules: r,
            recordings_dir: None,
        };
        let (_, evs) = collect(match_id, d).await;
        let got: Vec<ParserEvent> = parser_events(&evs);
        let want: Vec<ParserEvent> = std::fs::read_to_string(&golden)
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(got, want, "fixture {}", dir.display());
        ran += 1;
    }
    if ran == 0 {
        eprintln!("TC-04-1 skipped: no fixture with a relay/ folder (blocked on T-013)");
    }
}
