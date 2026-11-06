//! M-01 Match Watcher (PRG-01). See 04-data-sources-and-ingestion.md.
//!
//! Tails `console.log` and turns lines into `MatchEvent`s (FR-DATA-001, FR-DATA-002). The patterns come
//! from DATA-001 `[match_watcher]`, filled in T-006 from a real log (OQ-03). An empty pattern is never
//! compiled: `""` would match every line. Per match the log gives `Started` (ticket line) and then
//! `MatchId` (the next lines), then `Ended` at post-game; `GameExited` comes when the game quits.

use crate::settings::Rules;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};
use tokio::{sync::mpsc, task::JoinHandle};

#[derive(Debug, Clone, PartialEq)]
pub enum MatchEvent {
    Started,
    MatchId(u64),
    Ended,
    GameExited,
}

pub struct Patterns {
    pub start: Regex,
    pub end: Regex,
    pub match_id: Regex,
    pub exit: Regex,
}

/// One or more patterns in DATA-001 are empty or not valid. Match detection can't run; the app should
/// fall back to manual match entry and say why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotConfigured {
    /// Names of the empty patterns.
    pub empty: Vec<&'static str>,
    /// Pattern names that are not valid regexes, with the reason.
    pub invalid: Vec<(&'static str, String)>,
}

impl fmt::Display for NotConfigured {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "match watcher patterns in rules.toml [match_watcher]")?;
        if !self.empty.is_empty() {
            write!(f, " not configured: {}", self.empty.join(", "))?;
        }
        for (name, why) in &self.invalid {
            write!(f, "; {name} is invalid: {why}")?;
        }
        Ok(())
    }
}

impl std::error::Error for NotConfigured {}

impl Patterns {
    /// Build the patterns from DATA-001. Fails if any is empty or invalid, or if `match_id` has no
    /// capture group.
    pub fn from_rules(rules: &Rules) -> Result<Patterns, NotConfigured> {
        let w = &rules.match_watcher;
        let mut err = NotConfigured {
            empty: Vec::new(),
            invalid: Vec::new(),
        };
        let mut compile = |name: &'static str, src: &str, need_group: bool| -> Option<Regex> {
            if src.trim().is_empty() {
                err.empty.push(name);
                return None;
            }
            match Regex::new(src) {
                Ok(re) if need_group && re.captures_len() < 2 => {
                    err.invalid
                        .push((name, "needs one capture group for the match ID".into()));
                    None
                }
                Ok(re) => Some(re),
                Err(e) => {
                    err.invalid.push((name, e.to_string()));
                    None
                }
            }
        };
        let start = compile("start", &w.start, false);
        let end = compile("end", &w.end, false);
        let match_id = compile("match_id", &w.match_id, true);
        let exit = compile("exit", &w.exit, false);
        match (start, end, match_id, exit) {
            (Some(start), Some(end), Some(match_id), Some(exit)) => Ok(Patterns {
                start,
                end,
                match_id,
                exit,
            }),
            _ => Err(err),
        }
    }
}

pub fn classify(line: &str, p: &Patterns) -> Option<MatchEvent> {
    if let Some(c) = p.match_id.captures(line) {
        return c.get(1)?.as_str().parse().ok().map(MatchEvent::MatchId);
    }
    if p.start.is_match(line) {
        return Some(MatchEvent::Started);
    }
    if p.end.is_match(line) {
        return Some(MatchEvent::Ended);
    }
    if p.exit.is_match(line) {
        return Some(MatchEvent::GameExited);
    }
    None
}

/// Turns the raw `classify` events into a clean stream (DEC-062), so consumers never see a
/// half-finished match. The log writes `PostGame` only when a match is decided; leaving early writes
/// nothing, so the next `Started` or `GameExited` closes an unfinished match.
///
/// - `Started` is held until the next `MatchId`. A second `Started` while one is held replaces it.
/// - A `MatchId` equal to the open match's ID is a reconnect: both events are swallowed.
/// - Any other `MatchId` after a held `Started`: emit `Ended` for an open match, then `Started`, `MatchId`.
/// - A `MatchId` with no held `Started` is dropped (the pair is the only signal that a match began).
/// - `Ended` closes the open match; with none open it is dropped.
/// - `GameExited` emits `Ended` for an open match, then `GameExited`. It also clears a held `Started`.
#[derive(Debug, Default)]
pub struct Sequencer {
    held_start: bool,
    open: Option<u64>,
}

impl Sequencer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, ev: MatchEvent) -> Vec<MatchEvent> {
        match ev {
            MatchEvent::Started => {
                self.held_start = true;
                Vec::new()
            }
            MatchEvent::MatchId(id) => {
                if !std::mem::take(&mut self.held_start) {
                    return Vec::new();
                }
                if self.open == Some(id) {
                    return Vec::new();
                }
                let mut out = Vec::new();
                if self.open.is_some() {
                    out.push(MatchEvent::Ended);
                }
                self.open = Some(id);
                out.push(MatchEvent::Started);
                out.push(MatchEvent::MatchId(id));
                out
            }
            MatchEvent::Ended => {
                if self.open.take().is_some() {
                    vec![MatchEvent::Ended]
                } else {
                    Vec::new()
                }
            }
            MatchEvent::GameExited => {
                self.held_start = false;
                let mut out = Vec::new();
                if self.open.take().is_some() {
                    out.push(MatchEvent::Ended);
                }
                out.push(MatchEvent::GameExited);
                out
            }
        }
    }
}

/// Reads new complete lines from a file that may not exist yet and may be truncated.
struct Tailer {
    path: PathBuf,
    pos: u64,
}

impl Tailer {
    /// Start at the end of the file, so lines written before the app started produce no event.
    /// If the file doesn't exist yet, start at 0: everything in it will be new.
    fn at_end(path: PathBuf) -> Self {
        let pos = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        Self { path, pos }
    }

    /// New complete lines since the last call. A partial last line is kept for next time.
    /// I/O errors (file missing, locked) return no lines; the next poll tries again.
    fn read_new_lines(&mut self) -> Vec<String> {
        let Ok(meta) = std::fs::metadata(&self.path) else {
            self.pos = 0; // gone: the next file that appears is new
            return Vec::new();
        };
        let len = meta.len();
        if len < self.pos {
            self.pos = 0; // truncated: the game restarted
        }
        if len == self.pos {
            return Vec::new();
        }
        let Ok(mut f) = File::open(&self.path) else {
            return Vec::new();
        };
        if f.seek(SeekFrom::Start(self.pos)).is_err() {
            return Vec::new();
        }
        let mut r = BufReader::new(f);
        let mut out = Vec::new();
        let mut buf = Vec::new();
        loop {
            buf.clear();
            match r.read_until(b'\n', &mut buf) {
                Ok(0) | Err(_) => break,
                Ok(_) if !buf.ends_with(b"\n") => break, // partial line: read again next time
                Ok(n) => {
                    self.pos += n as u64;
                    // The log is not guaranteed to be UTF-8.
                    out.push(
                        String::from_utf8_lossy(&buf)
                            .trim_end_matches(['\r', '\n'])
                            .to_string(),
                    );
                }
            }
        }
        out
    }
}

/// Start watching. The start position is fixed before this returns, so a line written right after
/// the call is seen and anything already in the file is not. Stops when `tx`'s receiver is dropped.
pub fn spawn(
    path: PathBuf,
    p: Patterns,
    tx: mpsc::Sender<MatchEvent>,
    poll: Duration,
) -> JoinHandle<anyhow::Result<()>> {
    let tailer = Tailer::at_end(path);
    tokio::spawn(run_tailer(tailer, p, tx, poll))
}

/// `spawn` as an async fn (PRG-01 starter signature). `poll` is `rules.match_watcher.poll_s`.
pub async fn run(
    path: PathBuf,
    p: Patterns,
    tx: mpsc::Sender<MatchEvent>,
    poll: Duration,
) -> anyhow::Result<()> {
    run_tailer(Tailer::at_end(path), p, tx, poll).await
}

async fn run_tailer(
    mut tailer: Tailer,
    p: Patterns,
    tx: mpsc::Sender<MatchEvent>,
    poll: Duration,
) -> anyhow::Result<()> {
    let (ntx, mut nrx) = mpsc::channel::<notify::Result<Event>>(16);
    // Watch the parent folder so we also see the file being created. If the folder doesn't exist
    // yet, or the watcher can't start, the poll alone still works.
    let _watcher = start_watcher(&tailer.path, ntx);
    let mut tick = tokio::time::interval(poll);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut seq = Sequencer::new();
    loop {
        tokio::select! { _ = nrx.recv() => {}, _ = tick.tick() => {} }
        for line in tailer.read_new_lines() {
            if let Some(raw) = classify(&line, &p) {
                for ev in seq.push(raw) {
                    if tx.send(ev).await.is_err() {
                        return Ok(()); // receiver gone: shut down
                    }
                }
            }
        }
    }
}

/// A `MatchEvent` with timing for the detection-latency check (TST-010).
#[derive(Debug, Clone, PartialEq)]
pub struct Timed {
    pub event: MatchEvent,
    /// The time the console line itself carries. `None` for events the Sequencer made up (an implicit
    /// `Ended`), and for lines whose stamp can't be read.
    pub line_time: Option<SystemTime>,
    /// When this app read the line.
    pub read_at: SystemTime,
}

/// Parse the `MM/DD HH:MM:SS` stamp at the start of a console line (for example `11/06 00:10:18 `).
///
/// The stamp has no year or zone. It is read as local time at `utc_offset` (one fixed offset, so a
/// daylight-saving change during a session can be off by an hour; fine for a latency check) in the
/// year of `now`. If that lands more than a day after `now`, the previous year is used (a log line
/// from late December read just after New Year). `utc_offset` is a parameter because the `time`
/// crate's local-offset lookup is not enabled here; the app passes the system's offset, or
/// `UtcOffset::UTC` if it can't get one.
pub fn parse_line_time(
    line: &str,
    now: SystemTime,
    utc_offset: time::UtcOffset,
) -> Option<SystemTime> {
    let b = line.as_bytes();
    if b.len() < 14 || b[2] != b'/' || b[5] != b' ' || b[8] != b':' || b[11] != b':' {
        return None;
    }
    let num = |i: usize| -> Option<u8> {
        let (a, c) = (b[i], b[i + 1]);
        (a.is_ascii_digit() && c.is_ascii_digit()).then(|| (a - b'0') * 10 + (c - b'0'))
    };
    let (mo, d, h, mi, sec) = (num(0)?, num(3)?, num(6)?, num(9)?, num(12)?);
    let month = time::Month::try_from(mo).ok()?;
    let t = time::Time::from_hms(h, mi, sec).ok()?;
    let now_dt = time::OffsetDateTime::from(now).to_offset(utc_offset);
    let at = |year: i32| -> Option<time::OffsetDateTime> {
        let date = time::Date::from_calendar_date(year, month, d).ok()?;
        Some(time::PrimitiveDateTime::new(date, t).assume_offset(utc_offset))
    };
    let mut dt = at(now_dt.year());
    if dt.is_none_or(|x| x > now_dt + time::Duration::days(1)) {
        dt = at(now_dt.year() - 1);
    }
    Some(dt?.into())
}

/// Stamp the sequenced events for one raw event. `raw_time` is the raw line's stamp; `held_start` is the
/// stamp of the `Started` line that is waiting for its `MatchId`. An event that is the raw event gets
/// `raw_time`, a `Started` released by a `MatchId` gets `held_start`, anything else is synthetic: `None`.
fn stamp(
    raw: &MatchEvent,
    raw_time: Option<SystemTime>,
    held_start: Option<SystemTime>,
    out: Vec<MatchEvent>,
) -> Vec<(MatchEvent, Option<SystemTime>)> {
    out.into_iter()
        .map(|ev| {
            let t = if ev == *raw {
                raw_time
            } else if ev == MatchEvent::Started {
                held_start
            } else {
                None
            };
            (ev, t)
        })
        .collect()
}

/// Like `spawn`, but each event carries its timing (TST-010). `utc_offset` is the local offset used to
/// read the line stamps; see `parse_line_time`.
pub fn spawn_timed(
    path: PathBuf,
    p: Patterns,
    tx: mpsc::Sender<Timed>,
    poll: Duration,
    utc_offset: time::UtcOffset,
) -> JoinHandle<anyhow::Result<()>> {
    let tailer = Tailer::at_end(path);
    tokio::spawn(run_tailer_timed(tailer, p, tx, poll, utc_offset))
}

async fn run_tailer_timed(
    mut tailer: Tailer,
    p: Patterns,
    tx: mpsc::Sender<Timed>,
    poll: Duration,
    utc_offset: time::UtcOffset,
) -> anyhow::Result<()> {
    let (ntx, mut nrx) = mpsc::channel::<notify::Result<Event>>(16);
    let _watcher = start_watcher(&tailer.path, ntx);
    let mut tick = tokio::time::interval(poll);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut seq = Sequencer::new();
    let mut held_start: Option<SystemTime> = None;
    loop {
        tokio::select! { _ = nrx.recv() => {}, _ = tick.tick() => {} }
        for line in tailer.read_new_lines() {
            let read_at = SystemTime::now();
            if let Some(raw) = classify(&line, &p) {
                let raw_time = parse_line_time(&line, read_at, utc_offset);
                if raw == MatchEvent::Started {
                    held_start = raw_time;
                }
                for (event, line_time) in stamp(&raw, raw_time, held_start, seq.push(raw.clone())) {
                    let t = Timed {
                        event,
                        line_time,
                        read_at,
                    };
                    if tx.send(t).await.is_err() {
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn start_watcher(
    path: &Path,
    ntx: mpsc::Sender<notify::Result<Event>>,
) -> Option<RecommendedWatcher> {
    let parent = path.parent()?;
    if !parent.is_dir() {
        return None;
    }
    let mut w = RecommendedWatcher::new(
        move |res| {
            let _ = ntx.try_send(res); // a full queue already means "wake up"
        },
        notify::Config::default(),
    )
    .ok()?;
    w.watch(parent, RecursiveMode::NonRecursive).ok()?;
    Some(w)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{io::Write, time::Instant};
    use tokio::time::timeout;

    // Synthetic patterns for the tests. They are NOT real Deadlock log lines (OQ-03, T-006).
    fn test_patterns() -> Patterns {
        Patterns {
            start: Regex::new(r"^TEST_MATCH_START\b").unwrap(),
            end: Regex::new(r"^TEST_MATCH_END\b").unwrap(),
            match_id: Regex::new(r"^TEST_MATCH_ID=(\d+)").unwrap(),
            exit: Regex::new(r"^TEST_GAME_EXIT\b").unwrap(),
        }
    }

    /// `match_watcher.poll_s` from the shipped `config/rules.toml`.
    fn shipped_poll_s() -> u64 {
        let text = include_str!("../../../config/rules.toml");
        crate::settings::Rules::from_toml_str(text, Path::new("rules.toml"))
            .unwrap()
            .match_watcher
            .poll_s
    }

    const FAST: Duration = Duration::from_millis(30);
    const WAIT: Duration = Duration::from_secs(2);

    fn append(path: &Path, text: &str) {
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        f.write_all(text.as_bytes()).unwrap();
        f.flush().unwrap();
    }

    async fn next(rx: &mut mpsc::Receiver<MatchEvent>) -> Option<MatchEvent> {
        timeout(WAIT, rx.recv()).await.ok().flatten()
    }

    #[tokio::test]
    async fn tc_01_1_start_line_gives_started_within_2s() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "old noise\n").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        // The shipped 1 s poll on purpose: the 2 s budget in the test case must hold with it.
        let poll = Duration::from_secs(shipped_poll_s());
        let h = spawn(path.clone(), test_patterns(), tx, poll);
        let t = Instant::now();
        append(&path, "TEST_MATCH_START\nTEST_MATCH_ID=1\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        assert_eq!(next(&mut rx).await, Some(MatchEvent::MatchId(1)));
        assert!(t.elapsed() < Duration::from_secs(2));
        h.abort();
    }

    #[tokio::test]
    async fn tc_01_2_half_a_line_waits_for_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        append(&path, "TEST_MATCH_");
        assert!(
            timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err(),
            "no event on a partial line"
        );
        append(&path, "START\n");
        assert!(
            timeout(Duration::from_millis(300), rx.recv())
                .await
                .is_err(),
            "Started is held until the match ID"
        );
        append(&path, "TEST_MATCH_ID=1\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        assert_eq!(next(&mut rx).await, Some(MatchEvent::MatchId(1)));
        assert!(
            timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "exactly two events"
        );
        h.abort();
    }

    #[tokio::test]
    async fn tc_01_3_truncation_then_start_line() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(
            &path,
            "a long line of earlier output that makes the file big\n".repeat(20),
        )
        .unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        std::fs::write(&path, "").unwrap(); // truncate to 0
        tokio::time::sleep(Duration::from_millis(100)).await;
        append(&path, "TEST_MATCH_START\nTEST_MATCH_ID=1\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        h.abort();
    }

    #[tokio::test]
    async fn tc_01_4_old_start_line_gives_no_event() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "TEST_MATCH_START\nTEST_MATCH_ID=5\n").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        assert!(timeout(Duration::from_millis(400), rx.recv())
            .await
            .is_err());
        // And new lines still work afterwards.
        append(&path, "TEST_MATCH_START\nTEST_MATCH_ID=6\nTEST_MATCH_END\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        assert_eq!(next(&mut rx).await, Some(MatchEvent::MatchId(6)));
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Ended));
        h.abort();
    }

    #[test]
    fn tc_01_5_match_id_line() {
        let p = test_patterns();
        assert_eq!(
            classify("TEST_MATCH_ID=123456789", &p),
            Some(MatchEvent::MatchId(123456789))
        );
    }

    #[test]
    fn classify_covers_every_event_and_ignores_noise() {
        let p = test_patterns();
        assert_eq!(classify("TEST_MATCH_START", &p), Some(MatchEvent::Started));
        assert_eq!(classify("TEST_MATCH_END", &p), Some(MatchEvent::Ended));
        assert_eq!(classify("TEST_GAME_EXIT", &p), Some(MatchEvent::GameExited));
        assert_eq!(classify("unrelated", &p), None);
        // An ID too big for u64 gives no event rather than a wrong one.
        assert_eq!(classify("TEST_MATCH_ID=99999999999999999999999", &p), None);
    }

    #[tokio::test]
    async fn log_that_appears_later_is_read_from_the_top() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game").join("citadel").join("console.log");
        let (tx, mut rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        tokio::time::sleep(Duration::from_millis(100)).await;
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        append(&path, "TEST_MATCH_START\nTEST_MATCH_ID=42\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        assert_eq!(next(&mut rx).await, Some(MatchEvent::MatchId(42)));
        h.abort();
    }

    #[tokio::test]
    async fn crlf_and_non_utf8_lines_are_handled() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        let mut bytes = b"\xff\xfe garbage\r\n".to_vec();
        bytes.extend_from_slice(b"TEST_MATCH_START\r\nTEST_MATCH_ID=1\r\n");
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(&bytes).unwrap();
        drop(f);
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        h.abort();
    }

    #[tokio::test]
    async fn dropping_the_receiver_stops_the_watcher() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "").unwrap();
        let (tx, rx) = mpsc::channel(8);
        let h = spawn(path.clone(), test_patterns(), tx, FAST);
        drop(rx);
        append(&path, "TEST_MATCH_START\nTEST_MATCH_ID=1\n");
        assert!(timeout(WAIT, h)
            .await
            .expect("task should end")
            .unwrap()
            .is_ok());
    }

    // Sequencer (DEC-062)

    fn seq(evs: &[MatchEvent]) -> Vec<MatchEvent> {
        let mut s = Sequencer::new();
        evs.iter().flat_map(|e| s.push(e.clone())).collect()
    }
    use MatchEvent::{Ended, GameExited, MatchId, Started};

    #[test]
    fn sequencer_normal_match() {
        assert_eq!(
            seq(&[Started, MatchId(1), Ended, GameExited]),
            vec![Started, MatchId(1), Ended, GameExited]
        );
    }

    #[test]
    fn sequencer_holds_started_until_the_id() {
        let mut s = Sequencer::new();
        assert!(s.push(Started).is_empty());
        assert_eq!(s.push(MatchId(1)), vec![Started, MatchId(1)]);
    }

    #[test]
    fn sequencer_reconnect_is_swallowed() {
        assert_eq!(
            seq(&[Started, MatchId(1), Started, MatchId(1), Ended]),
            vec![Started, MatchId(1), Ended]
        );
    }

    #[test]
    fn sequencer_abandon_then_new_match() {
        assert_eq!(
            seq(&[Started, MatchId(1), Started, MatchId(2), Ended]),
            vec![Started, MatchId(1), Ended, Started, MatchId(2), Ended]
        );
    }

    #[test]
    fn sequencer_abandon_then_exit() {
        assert_eq!(
            seq(&[Started, MatchId(1), GameExited]),
            vec![Started, MatchId(1), Ended, GameExited]
        );
    }

    #[test]
    fn sequencer_double_ended_and_stray_ended_are_dropped() {
        assert_eq!(
            seq(&[Started, MatchId(1), Ended, Ended]),
            vec![Started, MatchId(1), Ended]
        );
        assert_eq!(seq(&[Ended]), vec![]);
    }

    #[test]
    fn sequencer_drops_match_id_without_started() {
        assert_eq!(seq(&[MatchId(9)]), vec![]);
        // And a stray ID does not disturb an open match.
        assert_eq!(
            seq(&[Started, MatchId(1), MatchId(1), Ended]),
            vec![Started, MatchId(1), Ended]
        );
    }

    #[test]
    fn sequencer_exit_with_no_match_and_exit_clears_held_start() {
        assert_eq!(seq(&[GameExited]), vec![GameExited]);
        assert_eq!(seq(&[Started, GameExited, MatchId(3)]), vec![GameExited]);
    }

    #[test]
    fn same_id_after_ended_is_a_new_match() {
        assert_eq!(
            seq(&[Started, MatchId(1), Ended, Started, MatchId(1)]),
            vec![Started, MatchId(1), Ended, Started, MatchId(1)]
        );
    }

    // Patterns from DATA-001

    fn shipped_rules() -> Rules {
        Rules::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml")).unwrap()
    }

    #[test]
    fn empty_pattern_is_never_compiled() {
        let mut r = shipped_rules();
        r.match_watcher.start = "^S".into();
        r.match_watcher.end = "^E".into();
        r.match_watcher.match_id = r"^I=(\d+)".into();
        r.match_watcher.exit = "   ".into(); // blank counts as empty
        let err = Patterns::from_rules(&r).err().unwrap();
        assert_eq!(err.empty, vec!["exit"]);
    }

    #[test]
    fn configured_patterns_compile() {
        let mut r = shipped_rules();
        r.match_watcher.start = "^S".into();
        r.match_watcher.end = "^E".into();
        r.match_watcher.match_id = r"^I=(\d+)".into();
        r.match_watcher.exit = "^X".into();
        let p = Patterns::from_rules(&r).unwrap();
        assert_eq!(classify("I=7", &p), Some(MatchEvent::MatchId(7)));
    }

    #[test]
    fn invalid_regex_and_missing_capture_group_are_reported() {
        let mut r = shipped_rules();
        r.match_watcher.start = "(".into();
        r.match_watcher.end = "^E".into();
        r.match_watcher.match_id = r"^I=\d+".into(); // no capture group
        r.match_watcher.exit = "^X".into();
        let err = Patterns::from_rules(&r).err().unwrap();
        assert!(err.empty.is_empty());
        let names: Vec<_> = err.invalid.iter().map(|i| i.0).collect();
        assert_eq!(names, vec!["start", "match_id"]);
    }

    // Shipped patterns against synthetic lines (T-006). The formats copy a real 2026-11-06 console.log;
    // every Steam ID, server ID and name here is made up. Real match IDs are used.

    fn shipped_patterns() -> Patterns {
        Patterns::from_rules(&shipped_rules()).expect("shipped patterns are configured")
    }

    const T: &str = "11/06 00:10:18";

    fn hideout_session() -> Vec<String> {
        vec![
            format!("{T} [Client] CL:  CCreateGameClientJob creating client connection to 'loopback'"),
            format!("{T} [Networking] Connected loopback client=aaaa1111@loopback:1 <-> server=bbbb2222@loopback:0"),
            format!("{T} [Client] CL:  Connected to 'loopback:1'"),
            format!("{T} Loading map \"dl_hideout\""),
            format!("{T} [Client] CL:  CCreateGameClientJob creating client connection to '127.0.0.1'"),
            format!("{T} [Client] CL:  Connected to '127.0.0.1:27015'"),
            format!("{T} ChangeGameState: MatchIntro (4)"),
            format!("{T} OnGameStateChanged: HeroSelection (3)"),
            format!("{T} OnGameStateChanged: GameInProgress (7)"),
            format!("{T} [SteamNetSockets] Received Steam datagram ticket for server steamid:90000000000000001 vport -1. hideout_lobby=170000000000000001"),
            format!("{T} [Client] Disconnecting from server: NETWORK_DISCONNECT_LOOPDEACTIVATE"),
            format!("{T} [Client] CL:  Connected to '=[A:1:111111111:52354]'"),
        ]
    }

    /// One public match as the log writes it, followed by the post-match hideout connect.
    fn match_session(id: u64, lobby: u64, how_left: &str) -> Vec<String> {
        vec![
            format!("{T} [GCClient] Recv msg 9100 (k_EMsgGCToClientSDRTicket), 254 bytes"),
            format!("{T} [SteamNetSockets] Received Steam datagram ticket for server steamid:90000000000000002 vport -1. match_id={id}"),
            format!("{T} Lobby {lobby} for Match {id} created"),
            format!("{T} [Citadel Play Controller] CCitadel_PlayController::OnMatchFormed"),
            format!("{T} [Client] CL:  Connected to '=[A:1:222222222:52354]'"),
            format!("{T} OnGameStateChanged: MatchIntro (4)"),
            format!("{T} OnGameStateChanged: GameInProgress (7)"),
            format!("{T} OnGameStateChanged: PostGame (8)"),
            format!("{T} Lobby {lobby} for Match {id} destroyed"),
            format!("{T} [SteamNetSockets] Received Steam datagram ticket for server steamid:90000000000000003 vport -1. hideout_lobby=170000000000000001"),
            format!("{T} [Client] Disconnected from server: {how_left}"),
            format!("{T} [Client] CL:  CCreateGameClientJob creating client connection to 'loopback'"),
            format!("{T} [Client] CL:  Connected to 'loopback:1'"),
            format!("{T} Loading map \"dl_hideout\""),
            format!("{T} [Client] CL:  Connected to '=[A:1:333333333:52354]'"),
            format!("{T} OnGameStateChanged: GameInProgress (7)"),
        ]
    }

    fn exit_lines() -> Vec<String> {
        vec![
            format!("{T} [HostStateManager] HostStateRequest::Start(HSR_QUIT)"),
            format!("{T} [Client] Disconnecting from server: NETWORK_DISCONNECT_EXITING"),
            format!("{T} Source2Shutdown"),
        ]
    }

    fn events(lines: &[String], p: &Patterns) -> Vec<MatchEvent> {
        lines.iter().filter_map(|l| classify(l, p)).collect()
    }

    #[test]
    fn hideout_and_loopback_sessions_give_no_events() {
        assert_eq!(events(&hideout_session(), &shipped_patterns()), vec![]);
    }

    #[test]
    fn three_matches_give_start_id_end_each_and_one_exit() {
        let p = shipped_patterns();
        let mut log = hideout_session();
        let ids = [(132128593, 1), (132142323, 2), (132156547, 3)];
        for (id, lobby) in ids {
            log.extend(match_session(
                id,
                lobby,
                "NETWORK_DISCONNECT_DISCONNECT_BY_USER",
            ));
            log.extend(hideout_session()); // back in the menu between matches
        }
        log.extend(exit_lines());
        let mut want = Vec::new();
        for (id, _) in ids {
            want.extend([
                MatchEvent::Started,
                MatchEvent::MatchId(id),
                MatchEvent::Ended,
            ]);
        }
        want.push(MatchEvent::GameExited);
        assert_eq!(events(&log, &p), want);
    }

    #[test]
    fn post_match_connect_is_not_a_new_match() {
        let p = shipped_patterns();
        let log = match_session(132128593, 1, "NETWORK_DISCONNECT_DISCONNECT_BY_USER");
        let n_started = events(&log, &p)
            .iter()
            .filter(|e| **e == MatchEvent::Started)
            .count();
        assert_eq!(n_started, 1);
    }

    #[test]
    fn exit_is_detected_once() {
        let p = shipped_patterns();
        let got = events(&exit_lines(), &p);
        assert_eq!(got, vec![MatchEvent::GameExited]);
    }

    /// TST-010 / TST-011 style: lines appended with real timing, events within 5 s with the right ID.
    #[tokio::test]
    async fn tst_010_011_events_within_5s_with_correct_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "old noise\n").unwrap();
        let (tx, mut rx) = mpsc::channel(16);
        let poll = Duration::from_secs(shipped_poll_s());
        let h = spawn(path.clone(), shipped_patterns(), tx, poll);
        let limit = Duration::from_secs(5);
        let p = shipped_patterns();
        let mut seq = Sequencer::new();
        for (id, lobby) in [(132128593u64, 1u64), (132142323, 2)] {
            let session = match_session(id, lobby, "NETWORK_DISCONNECT_DISCONNECT_BY_USER");
            for line in &session {
                let t = Instant::now();
                append(&path, &format!("{line}\n"));
                let Some(raw) = classify(line, &p) else {
                    continue;
                };
                for ev in seq.push(raw) {
                    let got = timeout(limit, rx.recv()).await.expect("event in time");
                    assert_eq!(got, Some(ev));
                    assert!(t.elapsed() < limit);
                }
            }
        }
        h.abort();
    }

    #[test]
    fn line_time_parses_local_stamp() {
        use time::macros::{datetime, offset};
        let now: SystemTime = datetime!(2026-11-06 12:00 +2).into();
        let got = parse_line_time("11/06 00:10:18 [Client] x", now, offset!(+2)).unwrap();
        assert_eq!(got, SystemTime::from(datetime!(2026-11-06 00:10:18 +2)));
        assert!(parse_line_time("no stamp here at all", now, offset!(+2)).is_none());
        assert!(parse_line_time("13/40 00:10:18 x", now, offset!(+2)).is_none());
        assert!(parse_line_time("11/06 00:10", now, offset!(+2)).is_none());
    }

    #[test]
    fn line_time_year_rolls_back_when_in_the_future() {
        use time::macros::{datetime, offset};
        // Read on 2 Jan; the line is from 31 Dec.
        let now: SystemTime = datetime!(2027-01-02 00:00:05 UTC).into();
        let got = parse_line_time("12/31 23:59:59 x", now, offset!(UTC)).unwrap();
        assert_eq!(got, SystemTime::from(datetime!(2026-12-31 23:59:59 UTC)));
        // Within a day ahead (clock skew) stays in the current year.
        let got = parse_line_time("01/02 12:00:00 x", now, offset!(UTC)).unwrap();
        assert_eq!(got, SystemTime::from(datetime!(2027-01-02 12:00:00 UTC)));
    }

    #[test]
    fn stamp_marks_sequencer_events_without_a_line_time() {
        let (t1, t2, t3) = (
            SystemTime::UNIX_EPOCH + Duration::from_secs(1),
            SystemTime::UNIX_EPOCH + Duration::from_secs(2),
            SystemTime::UNIX_EPOCH + Duration::from_secs(3),
        );
        let mut seq = Sequencer::new();
        let mut out = Vec::new();
        for (raw, t, held) in [
            (MatchEvent::Started, t1, t1),
            (MatchEvent::MatchId(5), t2, t1),
            (MatchEvent::Started, t3, t3),
            (MatchEvent::MatchId(6), t3, t3),
        ] {
            out.extend(stamp(&raw, Some(t), Some(held), seq.push(raw.clone())));
        }
        assert_eq!(
            out,
            vec![
                (MatchEvent::Started, Some(t1)),
                (MatchEvent::MatchId(5), Some(t2)),
                (MatchEvent::Ended, None), // implicit: match 5 was abandoned
                (MatchEvent::Started, Some(t3)),
                (MatchEvent::MatchId(6), Some(t3)),
            ]
        );
    }

    #[tokio::test]
    async fn spawn_timed_carries_line_and_read_times() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("console.log");
        std::fs::write(&path, "").unwrap();
        let (tx, mut rx) = mpsc::channel(8);
        let _h = spawn_timed(
            path.clone(),
            shipped_patterns(),
            tx,
            Duration::from_millis(30),
            time::UtcOffset::UTC,
        );
        let now = time::OffsetDateTime::now_utc();
        let stamp = format!(
            "{:02}/{:02} {:02}:{:02}:{:02}",
            u8::from(now.month()),
            now.day(),
            now.hour(),
            now.minute(),
            now.second()
        );
        let lines = match_session(77, 1, "x");
        let text: String = lines
            .iter()
            .map(|l| format!("{}\n", l.replacen(T, &stamp, 1)))
            .collect();
        append(&path, &text);
        let mut got = Vec::new();
        while got.len() < 2 {
            got.push(
                timeout(Duration::from_secs(3), rx.recv())
                    .await
                    .unwrap()
                    .unwrap(),
            );
        }
        assert_eq!(got[0].event, MatchEvent::Started);
        assert_eq!(got[1].event, MatchEvent::MatchId(77));
        for g in &got {
            let lt = g.line_time.expect("line time");
            let d = g.read_at.duration_since(lt).unwrap_or_default();
            assert!(d < Duration::from_secs(5), "{d:?}");
        }
    }
}
