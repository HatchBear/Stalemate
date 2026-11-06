//! M-01 Match Watcher (PRG-01). See 04-data-sources-and-ingestion.md.
//!
//! Tails `console.log` and turns lines into `MatchEvent`s (FR-DATA-001, FR-DATA-002). The patterns come
//! from DATA-001 `[match_watcher]`; they stay unconfigured (empty) until the author captures real log
//! lines in T-006 (OQ-03). An empty pattern is never compiled: `""` would match every line.

use crate::settings::Rules;
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use regex::Regex;
use std::{
    fmt,
    fs::File,
    io::{BufRead, BufReader, Seek, SeekFrom},
    path::{Path, PathBuf},
    time::Duration,
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
    loop {
        tokio::select! { _ = nrx.recv() => {}, _ = tick.tick() => {} }
        for line in tailer.read_new_lines() {
            if let Some(ev) = classify(&line, &p) {
                if tx.send(ev).await.is_err() {
                    return Ok(()); // receiver gone: shut down
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
        append(&path, "TEST_MATCH_START\n");
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
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
        assert_eq!(next(&mut rx).await, Some(MatchEvent::Started));
        assert!(
            timeout(Duration::from_millis(200), rx.recv())
                .await
                .is_err(),
            "exactly one event"
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
        append(&path, "TEST_MATCH_START\n");
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
        append(&path, "TEST_MATCH_END\n");
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
        bytes.extend_from_slice(b"TEST_MATCH_START\r\n");
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
        append(&path, "TEST_MATCH_START\n");
        assert!(timeout(WAIT, h)
            .await
            .expect("task should end")
            .unwrap()
            .is_ok());
    }

    // Patterns from DATA-001

    fn shipped_rules() -> Rules {
        Rules::load(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml")).unwrap()
    }

    #[test]
    fn shipped_empty_patterns_are_not_configured() {
        let err = Patterns::from_rules(&shipped_rules()).err().unwrap();
        assert_eq!(err.empty, vec!["start", "end", "match_id", "exit"]);
        assert!(err.to_string().contains("not configured"));
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
}
