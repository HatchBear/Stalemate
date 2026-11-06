# 12 — Programs and Code

## Purpose

Specifies every program and module: purpose, inputs, outputs, dependencies, starter code or pseudocode, and test cases.

## Scope

- **Covers:** PRG-01 to PRG-27 (runtime modules, binaries and developer tools).
- **Code status:** Starter code is Rust 2021 unless noted. It's a starting point, not a finished implementation. Lines marked `// VERIFY` depend on open questions.

## Details

### Program index

| ID | Program | Module | Inputs | Outputs | Phase |
|---|---|---|---|---|---|
| PRG-01 | Match Watcher | M-01 | `console.log`, DATA-001 patterns | `MatchEvent` | Alpha |
| PRG-02 | Account resolver | M-02 | Steam files | Steam roots, libraries, Account ID | Alpha |
| PRG-03 | API client | M-03 | Requests | Typed responses, cache rows | Alpha |
| PRG-04 | Broadcast client | M-04 | Match ID | `ParserEvent` stream, recording file | Alpha |
| PRG-05 | Game state actor | M-05 | `ParserEvent` | `GameState` | Alpha |
| PRG-06 | Live-Safe filter | M-06 | `GameState`, DATA-003 | `LiveSafeState` | Alpha |
| PRG-07 | Purchase Log | M-07 | `LiveSafeState` diffs, item tree | `PurchaseEvent` | Alpha |
| PRG-08 | Scouting service | M-08 | Roster, API client | `ScoutingCard` ×11 | Alpha |
| PRG-09 | Counter-buy filter | M-08 | Player matches, lift table | Item classes | Beta |
| PRG-10 | Build Matcher | M-09 | Items, candidate builds, priors | Posterior per build | Beta |
| PRG-11 | Projector | M-10 | Top build, transitions | Next 3 items | Beta |
| PRG-12 | Counter recommender | M-11 | Live-Safe state, Purchase Log, API data | Ranked items | Beta |
| PRG-13 | Build Check | M-12 | Build plan, scores | Swap suggestions | Beta |
| PRG-14 | Pace and objectives | M-13 | Own state, benchmarks, DATA-002 | Pace, timing, flex slots, timers | v1 |
| PRG-15 | Win-probability model | M-14 | State samples, DATA-004 | P(win) timeline | Beta |
| PRG-16 | Review evaluator | M-14 | Events, P timeline, DATA-001 | Decisions, tiers, accuracy, findings, fixes | Beta |
| PRG-17 | LLM narrator | M-14 | `ReviewFacts`, DATA-005 | Validated text | Beta |
| PRG-18 | Build library and builder | M-15 | User edits, API-15 | `Build` rows | Alpha |
| PRG-19 | Build importers | M-15 | API-16, URLs, DATA-009 | `Build` | Alpha (in-game), v1 (external) |
| PRG-20 | Main window app | M-18 | Core state | UI | Alpha |
| PRG-21 | .NET fallback parser | M-21 | Recording or `.dem` | `ParserEvent` JSON lines | Beta |
| PRG-22 | Overlay app | M-19 | `OverlayMessage` | Overlay surface | Beta |
| PRG-23 | Storage | M-16 | SQL | Rows | Alpha |
| PRG-24 | IPC | M-17 | Messages | Framed JSON | Beta |
| PRG-25 | Settings and secrets | M-20 | TOML, keyring | `Settings` | Alpha |
| PRG-26 | Model trainer (dev only) | — | API-05 bulk data | DATA-004 coefficients | Beta |
| PRG-27 | Fixture recorder (dev only) | — | Live matches | DATA-008 fixtures | Alpha |

### Shared types

```rust
pub type AccountId = u32;
pub type HeroId = u32;
pub type ItemId = u32;
pub type MatchId = u64;
pub type GameSecs = u32;

/// Normalized parser output. Identical for haste (PRG-04) and the .NET fallback (PRG-21).
#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
#[serde(tag = "kind")]
pub enum ParserEvent {
    Roster { players: Vec<RosterEntry> },
    Tick { t_game: GameSecs },
    Inventory { account_id: AccountId, items: Vec<ItemId> },
    Kda { account_id: AccountId, kills: u16, deaths: u16, assists: u16 },
    Level { account_id: AccountId, level: u8 },
    Objective { team: u8, kind: String, t_game: GameSecs },
    SelfState { souls: u32, cooldowns: Vec<(u32, f32)>, flex_slots: u8 },
    // Review Mode only. M-06 never forwards these:
    Position { account_id: AccountId, x: f32, y: f32, z: f32 },
    Damage { attacker: Option<AccountId>, victim: AccountId, amount: u32, source: String },
    Death { account_id: AccountId, t_game: GameSecs },
    Chat { account_id: AccountId, team_only: bool, text: String }, // parsed but never displayed (DEC-025)
    End { winning_team: u8 },
}

#[derive(serde::Serialize, serde::Deserialize, Clone, Debug)]
pub struct RosterEntry { pub account_id: AccountId, pub name: String, pub team: u8, pub hero_id: HeroId }
```

---

### PRG-01 Match Watcher

- **Purpose:** Detect match start, match ID and match end from `console.log` (FR-DATA-001, FR-DATA-002).
- **Inputs:** Log path (PRG-02); regexes from DATA-001 `[match_watcher]` (OQ-03).
- **Outputs:** `MatchEvent` on an `mpsc` channel.
- **Dependencies:** `notify`, `regex`, `tokio`.

```rust
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::{fs::File, io::{BufRead, BufReader, Seek, SeekFrom}, path::PathBuf, time::Duration};
use tokio::sync::mpsc;

#[derive(Debug, Clone, PartialEq)]
pub enum MatchEvent { Started, MatchId(u64), Ended, GameExited }

pub struct Patterns { pub start: Regex, pub end: Regex, pub match_id: Regex, pub exit: Regex } // DATA-001, VERIFY

pub fn classify(line: &str, p: &Patterns) -> Option<MatchEvent> {
    if let Some(c) = p.match_id.captures(line) {
        return c.get(1)?.as_str().parse().ok().map(MatchEvent::MatchId);
    }
    if p.start.is_match(line) { return Some(MatchEvent::Started); }
    if p.end.is_match(line) { return Some(MatchEvent::Ended); }
    if p.exit.is_match(line) { return Some(MatchEvent::GameExited); }
    None
}

pub async fn run(path: PathBuf, p: Patterns, tx: mpsc::Sender<MatchEvent>) -> anyhow::Result<()> {
    // Start at end of file: lines from before the app started are ignored.
    let mut pos = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    let (ntx, mut nrx) = mpsc::channel::<notify::Result<Event>>(16);
    let mut watcher = RecommendedWatcher::new(
        move |res| { let _ = ntx.blocking_send(res); },
        notify::Config::default(),
    )?;
    watcher.watch(path.parent().unwrap(), RecursiveMode::NonRecursive)?;
    let mut poll = tokio::time::interval(Duration::from_secs(1)); // fallback if notify misses events

    loop {
        tokio::select! { _ = nrx.recv() => {}, _ = poll.tick() => {} }
        let len = match std::fs::metadata(&path) { Ok(m) => m.len(), Err(_) => continue };
        if len < pos { pos = 0; }            // truncated: game restarted
        if len == pos { continue; }
        let mut f = File::open(&path)?;
        f.seek(SeekFrom::Start(pos))?;
        let mut r = BufReader::new(f);
        let mut line = String::new();
        while r.read_line(&mut line)? > 0 {
            if !line.ends_with('\n') { break; } // partial line: read again next time
            pos += line.len() as u64;
            if let Some(ev) = classify(&line, &p) { tx.send(ev).await?; }
            line.clear();
        }
    }
}
```

| Test | Case | Expected |
|---|---|---|
| TC-01-1 | Append a start line to a temp file | `Started` within 2 s |
| TC-01-2 | Append half a line, then the rest 1 s later | One event, after the second write |
| TC-01-3 | Truncate the file to 0, then append a start line | `Started` emitted |
| TC-01-4 | File already holds an old start line when `run` begins | No event |
| TC-01-5 | Line matching `match_id` with value `123456789` | `MatchId(123456789)` |

### PRG-02 Account resolver

- **Purpose:** Find Steam roots, Steam libraries, the Deadlock install and the user's Account ID (FR-DATA-006, NFR-PLAT-002).
- **Inputs:** Candidate roots from [04](04-data-sources-and-ingestion.md); `loginusers.vdf`; `libraryfolders.vdf`.
- **Outputs:** `SteamInfo { roots, libraries, deadlock_dir, console_log, account_id }`.
- **Dependencies:** A VDF parser crate ([10](10-technical-architecture.md)).

```rust
pub const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

pub fn account_id_from_steamid64(id64: u64) -> Option<u32> {
    id64.checked_sub(STEAMID64_BASE).and_then(|v| u32::try_from(v).ok())
}

pub fn candidate_roots(home: &std::path::Path) -> Vec<std::path::PathBuf> {
    #[cfg(target_os = "linux")]
    return vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    #[cfg(windows)]
    return vec![std::path::PathBuf::from(r"C:\Program Files (x86)\Steam")];
}
// Pseudocode:
// for root in candidate_roots: if root/config/loginusers.vdf exists:
//     pick the user with MostRecent == "1" -> account_id_from_steamid64
//     read root/steamapps/libraryfolders.vdf -> library paths
// for lib in libraries: if lib/steamapps/common/Deadlock exists -> deadlock_dir
// console_log = deadlock_dir/game/citadel/console.log   // verified on native Steam (DEC-062)
```

| Test | Case | Expected |
|---|---|---|
| TC-02-1 | `account_id_from_steamid64(76561197960287930)` | `Some(22202)` |
| TC-02-2 | `account_id_from_steamid64(1)` | `None` |
| TC-02-3 | Fixture with two users, one `MostRecent = 1` | That user's Account ID |
| TC-02-4 | Fixture with Deadlock in a second library | `deadlock_dir` points to the second library |
| TC-02-5 | Only the Flatpak Steam root exists | Resolved from the Flatpak path |

### PRG-03 API client

- **Purpose:** All deadlock-api calls, with rate limiting, caching and error mapping (FR-DATA-004).
- **Inputs:** Endpoint ID, path, query, TTL.
- **Outputs:** Deserialized `T`, or `ApiError`.
- **Dependencies:** `reqwest`, `governor`, PRG-23.

```rust
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use std::{num::NonZeroU32, sync::Arc};

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("no public data")] NoPublicData,                       // 404: never retried (NFR-FAIR-004)
    #[error("busy, retry after {retry_after_s}s")] Busy { retry_after_s: u64, timeout: bool },
    #[error("http {0}")] Http(u16),
    #[error(transparent)] Net(#[from] reqwest::Error),
    #[error(transparent)] Decode(#[from] serde_json::Error),
}

pub enum Scope { Analytics, Players, Other }

pub struct ApiClient {
    http: reqwest::Client,
    analytics: Arc<DefaultDirectRateLimiter>, // 160/min = 80% of 200/min per IP
    players: Arc<DefaultDirectRateLimiter>,   // 80/s = 80% of 100/s
    other: Arc<DefaultDirectRateLimiter>,     // 60/min [ASSUMPTION: unpublished]
    db: crate::storage::Storage,
}

impl ApiClient {
    pub fn new(db: crate::storage::Storage, user_agent: &str) -> Self {
        let q = |n, per_min: bool| {
            let n = NonZeroU32::new(n).unwrap();
            Arc::new(RateLimiter::direct(if per_min { Quota::per_minute(n) } else { Quota::per_second(n) }))
        };
        Self {
            http: reqwest::Client::builder().user_agent(user_agent).build().unwrap(),
            analytics: q(160, true), players: q(80, false), other: q(60, true), db,
        }
    }

    pub async fn get<T: serde::de::DeserializeOwned>(
        &self, scope: Scope, endpoint_id: &str, path: &str, query: &[(&str, String)], ttl_s: i64,
    ) -> Result<T, ApiError> {
        let key = crate::storage::cache_key("GET", path, query);
        if let Some(hit) = self.db.cache_get(&key).ok().flatten() {
            if hit.is_fresh(ttl_s) { return Ok(serde_json::from_slice(&hit.body)?); }
        }
        match scope {
            Scope::Analytics => self.analytics.until_ready().await,
            Scope::Players => self.players.until_ready().await,
            Scope::Other => self.other.until_ready().await,
        }
        let resp = self.http.get(format!("https://api.deadlock-api.com{path}")).query(query).send().await?;
        let status = resp.status().as_u16();
        let retry = resp.headers().get("retry-after")
            .and_then(|v| v.to_str().ok()).and_then(|s| s.parse().ok()).unwrap_or(30);
        match status {
            200 => {
                let body = resp.bytes().await?;
                let _ = self.db.cache_put(&key, endpoint_id, ttl_s, 200, &body);
                Ok(serde_json::from_slice(&body)?)
            }
            404 => Err(ApiError::NoPublicData),
            503 => Err(ApiError::Busy { retry_after_s: retry, timeout: false }),
            504 => Err(ApiError::Busy { retry_after_s: retry.max(300), timeout: true }), // no same-query retry for 5 min
            s => Err(ApiError::Http(s)),
        }
    }
}
```

| Test | Case | Expected |
|---|---|---|
| TC-03-1 | Second identical call within TTL | No network request (mock server sees 1 call) |
| TC-03-2 | Mock returns 503 with `Retry-After: 7` | `Busy { retry_after_s: 7 }` |
| TC-03-3 | Mock returns 504 | `retry_after_s ≥ 300` |
| TC-03-4 | 400 analytics calls queued | ≤ 160 sent in any 60 s window |
| TC-03-5 | Mock returns 404 | `NoPublicData`, and no retry within the test window |

### PRG-04 Broadcast client

- **Purpose:** Locate, consume, parse and record the Broadcast (FR-DATA-003, FR-DATA-005, NFR-REL-001).
- **Inputs:** Match ID; a broadcast locator (OQ-02).
- **Outputs:** `ParserEvent` stream; `recordings/<match_id>.bcast`.
- **Dependencies:** `haste` with the `broadcast` feature, `tokio`.
- **Replay fallback:** If the recording is missing or incomplete, Review downloads a replay through API-17 (FR-DATA-009, [VERIFY]).

```text
fn run(match_id, tx):
    backoff = [2, 4, 8, 16, 32]           # seconds (NFR-REL-001)
    failures = 0
    loop:
        url = locate_broadcast(match_id)   # OQ-02, VERIFY
        if url is None: goto retry
        rec = open_append(recordings/<match_id>.bcast)
        stream = haste::broadcast::open(url)   # VERIFY haste API
        for fragment in stream:
            rec.append(fragment.bytes)
            for ev in normalize(haste_events(fragment)):   # maps to ParserEvent
                tx.send(ev)
            failures = 0
            if ev is End: return Ok
        retry:
        if failures == len(backoff): tx.send(BroadcastLost); return
        sleep(backoff[failures]); failures += 1
```

| Test | Case | Expected |
|---|---|---|
| TC-04-1 | Recorded fixture replayed through a local HTTP server | Same `ParserEvent` sequence as a golden file |
| TC-04-2 | Server drops after 10 fragments, returns at 5 s | Reconnects; no duplicate events |
| TC-04-3 | Server never returns | Retries at 2, 4, 8, 16, 32 s, then `BroadcastLost` |
| TC-04-4 | Recording file after a full fixture | Byte length equals the sum of fragment lengths |
| TC-04-5 | Parity: same fixture through PRG-21 | Identical normalized events, except tick timestamps within 1 tick |

### PRG-05 Game state actor

- **Purpose:** Keep one authoritative `GameState` per match.
- **Inputs:** `ParserEvent`.
- **Outputs:** `GameState` snapshots on a `watch` channel; the full event log kept for Review.

```rust
pub struct GameState {
    pub match_id: u64, pub t_game: GameSecs, pub delay_s: u32,
    pub players: Vec<PlayerState>, pub objectives: Vec<(u8, String, GameSecs)>,
    pub me: AccountId, pub self_state: Option<SelfState>,
}
pub struct PlayerState {
    pub account_id: AccountId, pub name: String, pub team: u8, pub hero_id: HeroId,
    pub items: Vec<ItemId>, pub kda: (u16, u16, u16), pub level: u8, pub lane: Option<u8>,
    pub position: Option<[f32; 3]>, // Review only; M-06 never copies it
}
// apply(event) mutates state; delay_s = now_wall - wall_time_of(t_game) estimated from fragment timestamps.
```

| Test | Case | Expected |
|---|---|---|
| TC-05-1 | Roster then 12 Inventory events | 12 players with items |
| TC-05-2 | Tick events with wall-clock gaps of 75 s | `delay_s` within 75 ± 2 |

### PRG-06 Live-Safe filter

- **Purpose:** The only path from `GameState` to the UI in Live Mode (NFR-FAIR-003, FP-4).
- **Inputs:** `GameState`; the whitelist from DATA-003.
- **Outputs:** `LiveSafeState` ([11](11-data-model.md)).
- **Design rule:** `LiveSafeState` has no field for positions, enemy cooldowns, enemy health, chat or selected builds. They can't be sent by mistake.

```rust
use std::collections::HashSet;

#[derive(Hash, Eq, PartialEq, Clone, Copy, serde::Deserialize)]
pub enum Ls { Ls02Name, Ls05AllyItems, Ls06Kda, Ls07Level, Ls09Lane, Ls11Objectives, Ls13Flex }

pub struct Whitelist(pub HashSet<Ls>); // DATA-003: only fields verified in-game

#[derive(serde::Serialize)]
pub struct PlayerLs {
    pub account_id: AccountId, pub team: u8, pub hero_id: HeroId,          // LS-01, LS-03
    #[serde(skip_serializing_if = "Option::is_none")] pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")] pub items: Option<Vec<ItemId>>,
    #[serde(skip_serializing_if = "Option::is_none")] pub kda: Option<(u16, u16, u16)>,
    #[serde(skip_serializing_if = "Option::is_none")] pub level: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")] pub lane: Option<u8>,
}

pub fn project(gs: &GameState, wl: &Whitelist) -> Vec<PlayerLs> {
    let my_team = gs.players.iter().find(|p| p.account_id == gs.me).map(|p| p.team);
    gs.players.iter().map(|p| {
        let is_enemy = Some(p.team) != my_team;
        PlayerLs {
            account_id: p.account_id, team: p.team, hero_id: p.hero_id,
            name: wl.0.contains(&Ls::Ls02Name).then(|| p.name.clone()),
            // LS-04: enemy items always allowed; LS-05: ally items only if verified
            items: (is_enemy || wl.0.contains(&Ls::Ls05AllyItems)).then(|| p.items.clone()),
            kda: wl.0.contains(&Ls::Ls06Kda).then_some(p.kda),
            level: wl.0.contains(&Ls::Ls07Level).then_some(p.level),
            lane: if wl.0.contains(&Ls::Ls09Lane) { p.lane } else { None },
        }
    }).collect()
}
```

LS-02 (player names) is confirmed in [03](03-fair-play-and-compliance.md), so DATA-003 ships it enabled by default.

| Test | Case | Expected |
|---|---|---|
| TC-06-1 | Empty whitelist | Each player serializes only `account_id`, `team`, `hero_id`, plus `items` for enemies |
| TC-06-2 | State with positions set | No `position` key anywhere in the JSON |
| TC-06-3 | Whitelist with `Ls06Kda` | `kda` present for all players |
| TC-06-4 | Property test: random `GameState` × random whitelist | The JSON key set is always a subset of the allowed keys |

### PRG-07 Purchase Log

- **Purpose:** Turn inventory changes into `PurchaseEvent`s (FR-LIVE-002).
- **Inputs:** Previous and next item lists per player; the item tree from API-15.
- **Outputs:** `Vec<PurchaseEvent>`.

```rust
#[derive(Debug, PartialEq, Clone, Copy, serde::Serialize)]
pub enum Action { Buy, Sell, Upgrade }

#[derive(Debug, PartialEq, serde::Serialize)]
pub struct PurchaseEvent { pub account_id: AccountId, pub t_game_s: GameSecs, pub item_id: ItemId, pub action: Action, pub cost: u32 }

pub trait ItemTree { fn is_component_of(&self, component: ItemId, item: ItemId) -> bool; fn cost(&self, item: ItemId) -> u32; }

pub fn diff(prev: &[ItemId], next: &[ItemId], tree: &impl ItemTree, t: GameSecs, acct: AccountId) -> Vec<PurchaseEvent> {
    let removed: Vec<ItemId> = prev.iter().copied().filter(|i| !next.contains(i)).collect();
    let added: Vec<ItemId> = next.iter().copied().filter(|i| !prev.contains(i)).collect();
    let mut consumed = std::collections::HashSet::new();
    let mut out = Vec::new();
    for a in added {
        let comps: Vec<ItemId> = removed.iter().copied().filter(|r| tree.is_component_of(*r, a)).collect();
        let action = if comps.is_empty() { Action::Buy } else { consumed.extend(comps); Action::Upgrade };
        out.push(PurchaseEvent { account_id: acct, t_game_s: t, item_id: a, action, cost: tree.cost(a) });
    }
    for r in removed.into_iter().filter(|r| !consumed.contains(r)) {
        out.push(PurchaseEvent { account_id: acct, t_game_s: t, item_id: r, action: Action::Sell, cost: tree.cost(r) });
    }
    out
}
```

| Test | Case | Expected |
|---|---|---|
| TC-07-1 | `[]` → `[A]` | `Buy A` |
| TC-07-2 | `[A]` → `[B]`, A is a component of B | `Upgrade B` only |
| TC-07-3 | `[A]` → `[]` | `Sell A` |
| TC-07-4 | `[A, C]` → `[B]`, A is a component of B, C isn't | `Upgrade B`, `Sell C` |

### PRG-08 Scouting service

- **Purpose:** Build 11 `ScoutingCard`s (FR-SCOUT-001 to FR-SCOUT-013).
- **Inputs:** Roster; PRG-03.
- **Outputs:** `ScoutingCard` per player, stored and pushed to the main window.

```text
on Roster(players):
    # basic fields first; Beta fields follow as the analytics bucket allows (DEC-022)
    for p in players where p != me, in parallel (bounded by PRG-03 limiters):
        hist  = API-01(p)                  # last 20
        mmr   = API-02(p)
        hstat = API-03(p)
        if any returns NoPublicData: card(p) = NoPublicData; continue
        card.wl_overall = sum(hstat.wins) / sum(hstat.matches)
        card.wl_last20  = wins(hist[0..20])
        card.mmr_slope  = least_squares(mmr.last(20)) * 10
        card.mains      = top5(hstat by matches)
        card.hero_exp   = hstat[p.hero].matches
        card.profile    = PRG-09(p, p.hero)          # Beta
        card.spikes     = API-11(p) + median core-item times   # Beta
        card.playstyle  = percentiles(API-05 sample, API-12 baseline)   # Beta
        card.flags      = flags(p)                   # v1
        card.notes      = db.notes(p)                # v1
        push(card)
    me_panel = API-09(me), API-10(me), db.repeat_players(roster), session_check()
```

| Test | Case | Expected |
|---|---|---|
| TC-08-1 | 11 mocked players, mock latency 200 ms | All cards within 30 s |
| TC-08-2 | One player returns 404 | That card shows "No public data"; the others fill |
| TC-08-3 | MMR series `[1000, 1010, …, 1190]` (20 points, +10 each) | Slope reported as +100 per 10 matches |

### PRG-09 Counter-buy filter

- **Purpose:** Classify a player's items as Core, Counter buy, Situational or Ignored, and tag Off-plan items (FR-SCOUT-006).
- **Inputs:** `MatchSample` list; global lift `L(i, e)`; selected-build item set `S`; thresholds from DATA-001.
- **Outputs:** `HashMap<ItemId, (Class, off_plan: bool)>`.

```rust
use std::collections::{HashMap, HashSet};

pub struct MatchSample { pub items: HashSet<ItemId>, pub enemies: HashSet<HeroId> }
pub struct Th { pub core_rate: f64, pub core_max_cond_gap: f64, pub counter_mult: f64, pub counter_lift: f64,
                pub situational_rate: f64, pub min_enemy_matches: usize }
#[derive(Debug, PartialEq, Clone, Copy)] pub enum Class { Core, Counter, Situational, Ignored }

pub fn classify(ms: &[MatchSample], lift: &impl Fn(ItemId, HeroId) -> f64,
                selected: &HashSet<ItemId>, th: &Th) -> HashMap<ItemId, (Class, bool)> {
    let n = ms.len() as f64;
    let all_items: HashSet<ItemId> = ms.iter().flat_map(|m| m.items.iter().copied()).collect();
    let all_enemies: HashSet<HeroId> = ms.iter().flat_map(|m| m.enemies.iter().copied()).collect();
    let mut out = HashMap::new();
    for i in all_items {
        let r = ms.iter().filter(|m| m.items.contains(&i)).count() as f64 / n;
        let mut max_gap = 0.0f64;
        let mut is_counter = false;
        for &e in &all_enemies {
            let with_e: Vec<_> = ms.iter().filter(|m| m.enemies.contains(&e)).collect();
            if with_e.len() < th.min_enemy_matches { continue; }
            let r_e = with_e.iter().filter(|m| m.items.contains(&i)).count() as f64 / with_e.len() as f64;
            max_gap = max_gap.max(r_e - r);
            if r_e >= th.counter_mult * r && lift(i, e) >= th.counter_lift { is_counter = true; }
        }
        let class = if r >= th.core_rate && max_gap < th.core_max_cond_gap { Class::Core }
            else if r < th.core_rate && is_counter { Class::Counter }
            else if r >= th.situational_rate { Class::Situational }
            else { Class::Ignored };
        out.insert(i, (class, !selected.contains(&i)));
    }
    out
}
// Defaults (DATA-001): core_rate 0.60, core_max_cond_gap 0.20, counter_mult 2.0,
// counter_lift 1.3, situational_rate 0.15, min_enemy_matches 3.
```

| Test | Case | Expected |
|---|---|---|
| TC-09-1 | Item in 25 of 30 matches, no enemy dependence | Core |
| TC-09-2 | Item in 6 of 30 overall, but in 5 of 6 matches with hero E, and lift 1.6 | Counter |
| TC-09-3 | Same as TC-09-2, but lift 1.1 | Situational |
| TC-09-4 | Item in 2 of 30, no pattern | Ignored |
| TC-09-5 | Core item not in the selected build | `(Core, true)` |

### PRG-10 Build Matcher

- **Purpose:** Posterior over candidate builds for one enemy (FR-LIVE-003).
- **Inputs:** Owned items (expanded with components); candidates with priors.
- **Outputs:** Sorted `(build_id, posterior)`; `Unclear` if top < 0.40.

```rust
pub struct Candidate { pub build_id: String, pub items: HashSet<ItemId>, pub prior: f64 }

pub fn match_builds(owned: &HashSet<ItemId>, cands: &[Candidate], p_in: f64, p_out: f64) -> Vec<(String, f64)> {
    let mut logs: Vec<(String, f64)> = cands.iter().map(|c| {
        let ll: f64 = owned.iter().map(|i| if c.items.contains(i) { p_in.ln() } else { p_out.ln() }).sum();
        (c.build_id.clone(), c.prior.max(1e-9).ln() + ll)
    }).collect();
    let m = logs.iter().map(|x| x.1).fold(f64::NEG_INFINITY, f64::max);
    let z: f64 = logs.iter().map(|x| (x.1 - m).exp()).sum();
    for x in logs.iter_mut() { x.1 = (x.1 - m).exp() / z; }
    logs.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    logs
}
// Defaults (DATA-001): p_in 0.90, p_out 0.05, unclear_below 0.40, prior_player_weight 0.7.
```

| Test | Case | Expected |
|---|---|---|
| TC-10-1 | Owned = 4 items, all in build A, none in B, equal priors | A > 0.99 |
| TC-10-2 | No items owned | Posterior equals the normalized priors |
| TC-10-3 | Posteriors sum | 1.0 ± 1e-9 |

### PRG-11 Projector

- **Purpose:** Next 3 items per enemy (FR-LIVE-004).
- **Formula:** `score(j) = 0.6 × order_score(j) + 0.4 × T(j | last item, phase)`, as in [06](06-in-match-recommendations.md).

```rust
pub fn project(build_order: &[ItemId], owned: &HashSet<ItemId>, last: Option<ItemId>,
               trans: &impl Fn(Option<ItemId>, ItemId) -> f64) -> Vec<ItemId> {
    let remaining: Vec<ItemId> = build_order.iter().copied().filter(|i| !owned.contains(i)).collect();
    let mut scored: Vec<(ItemId, f64)> = remaining.iter().enumerate().map(|(pos, &j)| {
        let order = (1.0 - 0.2 * pos as f64).max(0.0);
        (j, 0.6 * order + 0.4 * trans(last, j))
    }).collect();
    scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    scored.into_iter().take(3).map(|x| x.0).collect()
}
```

| Test | Case | Expected |
|---|---|---|
| TC-11-1 | Order `[A,B,C,D]`, A owned, transitions all 0 | `[B,C,D]` |
| TC-11-2 | Fewer than 3 remaining | Returns only those |

### PRG-12 Counter recommender

- **Purpose:** Rank counter items (FR-LIVE-005 to FR-LIVE-008). Also computes the team comp prediction (FR-LIVE-016) from API-18 or its fallback.
- **Inputs:** Phase; target set E; threat weights from the Purchase Log and projections; lift and adjusted win rates from API-07 and API-08.
- **Outputs:** Top 5 `(item_id, score, delta_pp, drivers)`.

```rust
pub struct Enemy { pub account_id: AccountId, pub hero_id: HeroId, pub owned_value: u32, pub projected_value: u32 }

pub fn threat_weights(es: &[Enemy]) -> Vec<f64> {
    let v: Vec<f64> = es.iter().map(|e| e.owned_value as f64 + 0.5 * e.projected_value as f64).collect();
    let s: f64 = v.iter().sum();
    if s == 0.0 { vec![1.0 / es.len() as f64; es.len()] } else { v.iter().map(|x| x / s).collect() }
}

/// lift(i, e) = Some(WR(i | h, e) - WR(i | h)) or None if n < min_matches (200).
pub fn score_items(cands: &[ItemId], es: &[Enemy], w: &[f64],
                   adj_wr: &impl Fn(ItemId) -> Option<f64>,
                   lift: &impl Fn(ItemId, HeroId) -> Option<f64>) -> Vec<(ItemId, f64, f64)> {
    let mut out = Vec::new();
    'item: for &i in cands {
        let Some(base) = adj_wr(i) else { continue };
        let mut counter = 0.0;
        for (e, we) in es.iter().zip(w) {
            match lift(i, e.hero_id) { Some(l) => counter += we * l, None => continue 'item }
        }
        out.push((i, base + counter, counter * 100.0)); // delta_pp shown to user
    }
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    out.truncate(5);
    out
}
```

| Test | Case | Expected |
|---|---|---|
| TC-12-1 | Equal owned values for 6 enemies | Each weight is 1/6 |
| TC-12-2 | An item with one enemy lift under the 200-match minimum | Excluded |
| TC-12-3 | At 8:59 the target set is lane opponents; at 9:00 it's all enemies | Phase switches exactly at 540 s |
| TC-12-4 | A new enemy purchase raises their value | Ranking changes in the predicted direction |
| TC-12-5 | Two teams of 6 heroes with mocked matchup, synergy and counter tables | Predicted win rate equals the hand calculation, and is computed once per match |

### PRG-13 Build Check

- **Purpose:** Swap suggestions for the Build plan (FR-LIVE-009).

```rust
pub fn swaps(plan_unowned: &[(ItemId, Category, f64)], recs: &[(ItemId, Category, f64)], margin: f64)
    -> Vec<(ItemId, ItemId)> {
    let mut out = Vec::new();
    for (k, kc, ks) in plan_unowned {
        if let Some((i, _, _)) = recs.iter()
            .filter(|(_, c, s)| c == kc && *s - ks >= margin)
            .max_by(|a, b| a.2.partial_cmp(&b.2).unwrap()) {
            out.push((*k, *i));
        }
        if out.len() == 2 { break; }
    }
    out
}
// margin = 0.02 (DATA-001)
```

| Test | Case | Expected |
|---|---|---|
| TC-13-1 | Rec beats the plan item in the same category by 0.03 | One swap |
| TC-13-2 | Rec beats it by 0.01 | No swap |
| TC-13-3 | Rec is in a different category | No swap |

### PRG-14 Pace and objectives

- **Purpose:** FR-LIVE-010 to FR-LIVE-013.
- **Logic:**
    - `spm_delta_pct = 100 × (my_spm − bench_spm) / bench_spm`.
    - Timing is "ahead" if more than 60 s before the median, "behind" if more than 60 s after, otherwise "on time".
    - Flex slot and timer rules are read from DATA-002.

| Test | Case | Expected |
|---|---|---|
| TC-14-1 | my 900 spm, bench 1000 | −10% |
| TC-14-2 | Core item median at 600 s, now 700 s and not bought | "behind" |
| TC-14-3 | Timer entry at 300 s | Reminder shown from 270 s to 300 s [ASSUMPTION 30 s lead] |

### PRG-15 Win-probability model

- **Purpose:** P(win) for any state sample ([07](07-post-match-review-engine.md)).
- **Inputs:** Feature vector `[1, t, nw, nw_t, obj, alive]`; coefficients from DATA-004. The feature vector shrinks to the features the model file lists (DEC-042).

```rust
#[derive(serde::Deserialize)]
pub struct Model { pub patch: String, pub beta: [f64; 6], pub brier: f64 }

impl Model {
    pub fn p_win(&self, t_min: f64, nw: f64, obj: f64, alive: f64) -> f64 {
        let x = [1.0, t_min, nw, nw * t_min, obj, alive];
        let z: f64 = self.beta.iter().zip(x).map(|(b, xi)| b * xi).sum();
        1.0 / (1.0 + (-z).exp())
    }
    /// dP/dnw, used for D-FARM
    pub fn dp_dnw(&self, t_min: f64, nw: f64, obj: f64, alive: f64) -> f64 {
        let p = self.p_win(t_min, nw, obj, alive);
        p * (1.0 - p) * (self.beta[2] + self.beta[3] * t_min)
    }
}
```

| Test | Case | Expected |
|---|---|---|
| TC-15-1 | All betas 0 | P = 0.5 |
| TC-15-2 | `dp_dnw` against a finite difference (h = 1e-6) | Equal within 1e-6 |

### PRG-16 Review evaluator

- **Purpose:** Decisions, EPL, tiers, accuracy, Key Moments, rule findings and fixes (FR-REV-002 to FR-REV-014).

```rust
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum Tier { T1, T2, T3, T4, T5, T6, T7, T8, T9 }

pub struct Decision { pub epl: f64, pub was_best: bool, pub p_before: f64, pub p_after: f64,
                      pub user_share: f64, pub user_died: bool, pub missed_window: bool }

pub fn tier(d: &Decision) -> Tier {
    let critical = (d.p_before < 0.40 && d.p_after >= 0.50)
        || ((0.40..=0.60).contains(&d.p_before) && d.p_after >= 0.65);
    if critical && d.user_share >= 0.33 { return Tier::T7; }
    if d.user_died && d.p_after - d.p_before >= 0.10 { return Tier::T8; }
    if d.missed_window { return Tier::T9; }
    match d.epl {
        e if e == 0.0 && d.was_best => Tier::T1,
        e if e <= 0.02 => Tier::T2,
        e if e <= 0.05 => Tier::T3,
        e if e <= 0.10 => Tier::T4,
        e if e <= 0.20 => Tier::T5,
        _ => Tier::T6,
    }
}

/// `epls_by_type`: one slice of EPLs per decision type (D-DEATH, D-ITEM, D-OBJ, D-FARM, D-FIGHT),
/// after overlap removal (DEC-029). Types with no decisions are omitted.
pub fn accuracy(epls_by_type: &[&[f64]], k: f64) -> Option<u8> {
    let means: Vec<f64> = epls_by_type.iter().filter(|t| !t.is_empty())
        .map(|t| t.iter().sum::<f64>() / t.len() as f64).collect();
    if means.is_empty() { return None; }
    let mean = means.iter().sum::<f64>() / means.len() as f64; // each type counts equally
    Some((100.0 * (-k * mean).exp()).round() as u8) // k = 10 (DATA-001)
}
```

Note: an EPL of exactly 0.00 on a decision that wasn't the best available falls to T2.

| Test | Case | Expected |
|---|---|---|
| TC-16-1 | `accuracy(&[&[0.02]], 10)` | 82 |
| TC-16-2 | `accuracy(&[&[0.05]], 10)` | 61 |
| TC-16-3 | `accuracy(&[&[0.10]], 10)` | 37 |
| TC-16-4 | EPL 0.15, no special rule | T5 |
| TC-16-5 | p 0.35 → 0.55, share 0.5 | T7 |
| TC-16-6 | Same input twice | Identical output (determinism, TST-068) |
| TC-16-7 | Death with escape item off cooldown 5 s before death | R-01 fires |
| TC-16-8 | `accuracy(&[&[0.0, 0.0, 0.0], &[0.10]], 10)` | 61 (each type counts equally) |
| TC-16-9 | A death window and a fight window overlap on the same swing | The overlap is counted once, under D-DEATH |

### PRG-17 LLM narrator

- **Purpose:** Plain-language text from `ReviewFacts`, with number validation (FR-REV-013, FR-REV-015).

```rust
#[async_trait::async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, system: &str, user: &str, max_tokens: u32) -> anyhow::Result<String>;
}
// Implementations: OffProvider (always Err), LocalOpenAiCompat { base_url },
// OwnKeyOpenAiCompat { base_url, key }, OwnKeyAnthropic { key }.

pub fn numbers_in(s: &str) -> Vec<String> {
    let re = regex::Regex::new(r"-?\d+(?:[.,]\d+)?").unwrap();
    re.find_iter(s).map(|m| m.as_str().replace(',', "")).collect()
}

/// Every number in `text` must appear in `facts_json`, after normalizing (strip trailing ".0").
pub fn validate(text: &str, facts_json: &str) -> bool {
    let norm = |x: &str| x.trim_end_matches(".0").to_string();
    let allowed: std::collections::HashSet<String> = numbers_in(facts_json).iter().map(|x| norm(x)).collect();
    numbers_in(text).iter().all(|n| allowed.contains(&norm(n)))
}

pub async fn narrate(p: &dyn LlmProvider, prompt: &str, facts: &str, fallback: String) -> String {
    for _ in 0..2 {
        if let Ok(t) = p.complete(prompt, facts, 400).await {
            if validate(&t, facts) { return t; }
        }
    }
    fallback // template text from DATA-006
}
```

| Test | Case | Expected |
|---|---|---|
| TC-17-1 | Text "You died at 12 minutes", facts contain 12 | Valid |
| TC-17-2 | Text mentions 47%, facts don't | Invalid; fallback used |
| TC-17-3 | `OffProvider` | Template text, no network |

### PRG-18 Build library and builder

- **Purpose:** CRUD, validation, active-plan selection and patch re-validation (FR-BUILD-003, FR-BUILD-005, FR-BUILD-006).
- **Rules:** No duplicate items in a build; every item exists in the current API-15 data; on patch change, set `needs_update = 1` when items were removed.

| Test | Case | Expected |
|---|---|---|
| TC-18-1 | Add the same item twice | Rejected |
| TC-18-2 | Export a build, then import it | Byte-identical JSON after normalization |
| TC-18-3 | Item removed in a new patch | `needs_update = 1` |

### PRG-19 Build importers

```rust
pub trait BuildImporter {
    fn id(&self) -> &'static str;
    fn can_handle(&self, url: &url::Url) -> bool;
    fn import(&self, url: &url::Url) -> anyhow::Result<ImportedBuild>; // unresolved item names listed, never dropped
}
// Implementations: InGameCommunity (API-16, VERIFY path), one per external site (OQ-06).
```

| Test | Case | Expected |
|---|---|---|
| TC-19-1 | In-game build fixture | All items resolved |
| TC-19-2 | Fixture with one unknown item name | Import succeeds; one unresolved name listed |

### PRG-20 Main window app

- **Purpose:** The `stalemate` binary: starts the core, renders views ([09](09-ui-window-and-overlay.md)), and launches PRG-22 in Live Mode.

```rust
slint::include_modules!(); // ui/*.slint

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let settings = core::settings::load()?;
    let core = core::Core::start(settings).await?;        // PRG-01..PRG-17, PRG-23..PRG-25
    let ui = MainWindow::new()?;
    let weak = ui.as_weak();
    core.on_ui_update(move |view_model| {
        let weak = weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(ui) = weak.upgrade() { ui.set_model(view_model.into()); }
        });
    });
    core.on_mode_change(|mode| core::overlay_process::set_running(mode.is_live()));
    ui.run()?;
    Ok(())
}
```

| Test | Case | Expected |
|---|---|---|
| TC-20-1 | Launch with no Steam found | Home view shows a failed setup check |
| TC-20-2 | Simulated Live Mode | Overlay process started; stopped on Review Mode |

### PRG-21 .NET fallback parser

- **Purpose:** Parse a recording or `.dem` with DemoFile when haste fails (NFR-REL-002).
- **Interface:** `stalemate-parser-fallback --input <path> --format bcast|dem`. Writes `ParserEvent` JSON lines to stdout; exit code 0 on success.

```csharp
// C# (.NET); DemoFile API names are placeholders — VERIFY against the DemoFile package.
using System.Text.Json;
var input = args[Array.IndexOf(args, "--input") + 1];
await using var stream = File.OpenRead(input);
var parser = new DeadlockDemoParser();             // VERIFY type name
parser.OnInventoryChanged += (acct, items, t) =>
    Console.WriteLine(JsonSerializer.Serialize(new { kind = "Inventory", account_id = acct, items }));
// ... map every other ParserEvent kind the same way ...
await parser.ReadAllAsync(stream);                 // VERIFY method name
```

| Test | Case | Expected |
|---|---|---|
| TC-21-1 | Parity with PRG-04 on 5 fixtures | Same events (TC-04-5) |
| TC-21-2 | Corrupt input | Non-zero exit and an error on stderr |

### PRG-22 Overlay app

- **Purpose:** The `stalemate-overlay` binary: renders OV-1 to OV-5 from `OverlayMessage` ([09](09-ui-window-and-overlay.md)).
- **Linux (KDE Plasma Wayland):**

```text
surface = layer_shell.create(layer = Overlay, namespace = "io.github.hatchbear.stalemate.overlay")
surface.set_keyboard_interactivity(None)
surface.set_exclusive_zone(-1)
surface.set_anchor(settings.anchor)            # one of 9 anchors
surface.set_margin(settings.offset)
wl_surface.set_input_region(empty_region)      # click-through
render Slint component via layer-shika         # or smithay-client-toolkit fallback (RSK-05)
```

- **Windows:**

```rust
use windows::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

pub unsafe fn make_click_through_topmost(hwnd: HWND) -> windows::core::Result<()> {
    let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
    let add = (WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_TOPMOST | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW).0 as isize;
    SetWindowLongPtrW(hwnd, GWL_EXSTYLE, ex | add);
    SetWindowPos(hwnd, HWND_TOPMOST, 0, 0, 0, 0, SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE)
}
```

| Test | Case | Expected |
|---|---|---|
| TC-22-1 | A click on the overlay area | Received by the window underneath (manual test, TST-090 and TST-091) |
| TC-22-2 | 10 updates in 1 s | ≤ 1 redraw per second |

### PRG-23 Storage

- **Purpose:** Open SQLite, run migrations from [11](11-data-model.md), and provide typed queries and the API cache.
- **Rule:** Migrations are numbered SQL files in `crates/core/src/storage/migrations/NNN.sql`, applied in order inside a transaction.

| Test | Case | Expected |
|---|---|---|
| TC-23-1 | Fresh database | Version 1 |
| TC-23-2 | Run migrations twice | No error, no change |

### PRG-24 IPC

```rust
#[derive(serde::Serialize, serde::Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OverlayMessage {
    Update { seq: u64, payload: OverlayPayload }, // built only from PRG-06 output
    Show, Hide, Config { settings: OverlaySettings },
    Ack { seq: u64 }, Hotkey, Error { message: String },
}
// Framing: one JSON object per line, UTF-8, max 64 KiB per line.
```

| Test | Case | Expected |
|---|---|---|
| TC-24-1 | Round-trip every variant | Equal after decode |
| TC-24-2 | A line over 64 KiB | Rejected, connection kept |

### PRG-25 Settings and secrets

- **Files:** `settings.toml` in the config directory ([10](10-technical-architecture.md)); defaults in DATA-001.
- **Secrets:** Own LLM keys and own deadlock-api key are stored in the keyring, never in TOML.

| Test | Case | Expected |
|---|---|---|
| TC-25-1 | Save a key, then read `settings.toml` | Key not present in the file |

### PRG-26 Model trainer (developer tool)

- **Purpose:** Fit DATA-004 on the last few patches, and refit when its Brier score degrades ([07](07-post-match-review-engine.md), DEC-042).
- **Steps:**
    1. Pull ≥ 20,000 Standard matches from the last 3 patches [ASSUMPTION] from API-05, within rate limits.
    2. Sample every 30 s and build features.
    3. Split matches 90/10.
    4. Fit with L2-regularized logistic regression (λ = 1e-3 [ASSUMPTION]).
    5. Report the Brier score on the 10% split.
    6. Write `models/model.json` only if Brier ≤ 0.20. It lists the patches it was trained on.

| Test | Case | Expected |
|---|---|---|
| TC-26-1 | Synthetic data from known betas | Recovered betas within 5% |

### PRG-27 Fixture recorder (developer tool)

- **Purpose:** Record the developer's own matches (Broadcast fragments and API responses) as test fixtures (DATA-008).
- **Rule:** Only the developer's own matches; strip chat text before committing fixtures [ASSUMPTION].

## Dependencies

- [04](04-data-sources-and-ingestion.md)–[09](09-ui-window-and-overlay.md): behavior.
- [10 Architecture](10-technical-architecture.md): modules.
- [11 Data model](11-data-model.md): types.
- [14 Data assets](14-data-and-content-assets.md): DATA-001 to DATA-009.
- [16 Testing](16-testing-and-qa.md)

## Open Questions

- OQ-02, OQ-03: The broadcast locator and log patterns (PRG-01, PRG-04).
- OQ-13: The exact haste broadcast API and DemoFile Deadlock API (PRG-04, PRG-21).
- [ASSUMPTION] The rate for unpublished limit scopes (60 req/min), the 30 s timer lead, λ = 1e-3, stripping chat from fixtures.

## Acceptance Criteria

- [ ] Every PRG has passing unit tests for all listed TC cases.
- [ ] `cargo clippy -- -D warnings` and `cargo fmt --check` pass.
- [ ] All `// VERIFY` markers are resolved before Beta.
