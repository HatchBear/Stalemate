use super::*;
use std::io::Read;

fn rules() -> DiagnosticsRules {
    DiagnosticsRules {
        enabled_default: true,
        keep_days: 14,
        perf_sample_s: 10,
        max_file_mb: 20,
    }
}

struct Env {
    _tmp: tempfile::TempDir,
    data: PathBuf,
    logs: PathBuf,
    out: PathBuf,
}

fn env() -> Env {
    let tmp = tempfile::tempdir().unwrap();
    let root = tmp.path().to_path_buf();
    let e = Env {
        data: root.join("diagnostics"),
        logs: root.join("logs"),
        out: root.join("out"),
        _tmp: tmp,
    };
    std::fs::create_dir_all(&e.logs).unwrap();
    e
}

fn opts(e: &Env) -> ExportOptions {
    ExportOptions {
        note: "it froze".into(),
        log_dir: e.logs.clone(),
        about: About {
            version: "0.1.0".into(),
            ..Default::default()
        },
        ..Default::default()
    }
}

fn entries(path: &Path) -> Vec<String> {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut v: Vec<String> = (0..z.len())
        .map(|i| z.by_index(i).unwrap().name().to_string())
        .collect();
    v.sort();
    v
}

fn read_entry(path: &Path, name: &str) -> String {
    let mut z = zip::ZipArchive::new(std::fs::File::open(path).unwrap()).unwrap();
    let mut s = String::new();
    z.by_name(name).unwrap().read_to_string(&mut s).unwrap();
    s
}

#[test]
fn round_trip_of_each_record_type() {
    let e = env();
    let d = Diagnostics::open(&e.data, &rules(), true);
    d.check("steam", false, "not found");
    d.check("steam", true, "found");
    d.match_record(&MatchDiag {
        match_id: 7,
        detect_latency_ms: Some(120),
        ..Default::default()
    });
    d.match_record(&MatchDiag {
        match_id: 7,
        review_result: Some("ok".into()),
        ..Default::default()
    });
    d.perf(PerfSample {
        cpu_pct: 3.5,
        rss_mb: 210,
    });
    d.usage("view.home");
    d.usage("view.home");
    d.usage("try_again");
    d.marker("odd roster");
    let s = d.summary();
    assert_eq!(
        (
            s.checks,
            s.matches,
            s.perf_samples,
            s.usage_actions,
            s.markers
        ),
        (1, 1, 1, 3, 1)
    );
    let p = d.export(&e.out, &opts(&e)).unwrap();
    assert!(p
        .file_name()
        .unwrap()
        .to_string_lossy()
        .starts_with("stalemate-diagnostics-"));
    let checks: serde_json::Value = serde_json::from_str(&read_entry(&p, "checks.json")).unwrap();
    assert_eq!(checks[0]["ok"], true);
    assert_eq!(checks[0]["detail"], "found");
    let m: serde_json::Value =
        serde_json::from_str(read_entry(&p, "matches.jsonl").trim()).unwrap();
    assert_eq!(m["match_id"], 7);
    assert_eq!(m["detect_latency_ms"], 120);
    assert_eq!(m["review_result"], "ok");
    let perf: serde_json::Value =
        serde_json::from_str(read_entry(&p, "perf.jsonl").trim()).unwrap();
    assert_eq!(perf["rss_mb"], 210);
    let u: serde_json::Value = serde_json::from_str(&read_entry(&p, "usage.json")).unwrap();
    assert_eq!(u["view.home"], 2);
    assert!(read_entry(&p, "markers.jsonl").contains("odd roster"));
    assert!(read_entry(&p, "note.txt").contains("it froze"));
}

#[test]
fn disabled_writes_nothing() {
    let e = env();
    let d = Diagnostics::open(&e.data, &rules(), false);
    d.check("a", true, "");
    d.usage("x");
    d.marker("m");
    assert!(!e.data.exists());
    d.set_enabled(true);
    d.usage("x");
    assert!(e.data.exists());
    d.set_enabled(false);
    let before = std::fs::read_dir(&e.data).unwrap().count();
    d.usage("y");
    assert_eq!(std::fs::read_dir(&e.data).unwrap().count(), before);
    assert_eq!(d.summary().usage_actions, 1);
}

#[test]
fn old_days_roll_off() {
    let e = env();
    std::fs::create_dir_all(&e.data).unwrap();
    let today = time::OffsetDateTime::now_utc().date();
    let name = |n: i64| {
        format!(
            "diag-{}.jsonl",
            store::date_str(today - time::Duration::days(n))
        )
    };
    for n in [0, 14, 15, 40] {
        std::fs::write(e.data.join(name(n)), "").unwrap();
    }
    std::fs::write(e.data.join("keep-me.txt"), "").unwrap();
    let _d = Diagnostics::open(&e.data, &rules(), true);
    let mut left: Vec<String> = std::fs::read_dir(&e.data)
        .unwrap()
        .map(|f| f.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    left.sort();
    let mut want = vec![name(0), name(14), "keep-me.txt".to_string()];
    want.sort();
    assert_eq!(left, want);
}

#[test]
fn zip_holds_exactly_the_listed_entries() {
    let e = env();
    std::fs::write(e.logs.join("stalemate.2026-11-05.log"), "x INFO a\n").unwrap();
    std::fs::write(e.logs.join("stalemate.2026-11-06.log"), "x INFO b\n").unwrap();
    let d = Diagnostics::open(&e.data, &rules(), true);
    let p = d.export(&e.out, &opts(&e)).unwrap();
    assert_eq!(
        entries(&p),
        [
            "about.json",
            "checks.json",
            "log/stalemate.2026-11-05.log",
            "log/stalemate.2026-11-06.log",
            "markers.jsonl",
            "matches.jsonl",
            "note.txt",
            "perf.jsonl",
            "usage.json"
        ]
    );
}

#[test]
fn log_keeps_info_and_above_and_rewrites_paths_and_server_ids() {
    let e = env();
    std::fs::write(
        e.logs.join("a.log"),
        "t DEBUG noisy\nt INFO open /home/jo/.local/x\nt WARN on [A:1:1:2] steamid:123\n  backtrace line\n",
    )
    .unwrap();
    let d = Diagnostics::open(&e.data, &rules(), true);
    let mut o = opts(&e);
    o.home_dir = Some("/home/jo".into());
    let p = d.export(&e.out, &o).unwrap();
    let log = read_entry(&p, "log/a.log");
    assert!(!log.contains("noisy"));
    assert!(log.contains("open ~/.local/x"));
    assert!(log.contains("[server]") && log.contains("steamid:[server]"));
    assert!(log.contains("backtrace line"));
}

#[test]
fn size_cap_drops_oldest_log_lines_first() {
    let e = env();
    let mut r = rules();
    r.max_file_mb = 1;
    // Incompressible-ish lines so the cap bites.
    let mut s = String::new();
    let mut x: u64 = 88172645463325252;
    for i in 0..60_000 {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        s.push_str(&format!(
            "t INFO line{i:06} {x:016x}{:016x}\n",
            x.rotate_left(17)
        ));
    }
    std::fs::write(e.logs.join("big.log"), &s).unwrap();
    let d = Diagnostics::open(&e.data, &r, true);
    let p = d.export(&e.out, &opts(&e)).unwrap();
    assert!(std::fs::metadata(&p).unwrap().len() <= 1024 * 1024);
    let log = read_entry(&p, "log/big.log");
    assert!(log.contains("line059999"), "newest kept");
    assert!(!log.contains("line000000"), "oldest dropped");
    assert!(log.len() < s.len());
}

fn leak_kind(r: Result<PathBuf, ExportError>, out: &Path) -> String {
    let err = r.unwrap_err();
    assert!(
        !out.exists() || std::fs::read_dir(out).unwrap().count() == 0,
        "nothing written"
    );
    match err {
        ExportError::Leak { kind, .. } => kind,
        other => panic!("expected Leak, got {other}"),
    }
}

#[test]
fn leak_scan_fails_closed() {
    let cases: [(&str, &str); 4] = [
        ("key=sk-abc123", "API key"),
        ("saw Wraith_Main there", "another player's name"),
        ("player 99887766 left", "another player's Account ID"),
        ("steam 76561198012345678", "SteamID64"),
    ];
    for (line, kind) in &cases {
        let e = env();
        std::fs::write(e.logs.join("a.log"), format!("t INFO {line}\n")).unwrap();
        let d = Diagnostics::open(&e.data, &rules(), true);
        d.redact_also(99887766, "Wraith_Main");
        let mut o = opts(&e);
        o.api_key = Some("sk-abc123".into());
        o.home_dir = Some("/home/jo".into());
        assert_eq!(&leak_kind(d.export(&e.out, &o), &e.out), kind, "{line}");
    }
}

#[test]
fn leak_in_the_note_and_in_markers_is_caught() {
    let e = env();
    let d = Diagnostics::open(&e.data, &rules(), true);
    d.marker("my key is sk-abc123");
    let mut o = opts(&e);
    o.api_key = Some("sk-abc123".into());
    assert_eq!(leak_kind(d.export(&e.out, &o), &e.out), "API key");
    let d2 = Diagnostics::open(&env().data, &rules(), true);
    o.note = "ask Wraith_Main".into();
    d2.redact_also(987_654_321, "Wraith_Main");
    assert_eq!(
        leak_kind(d2.export(&e.out, &o), &e.out),
        "another player's name"
    );
}

#[test]
fn home_path_is_redacted_not_leaked() {
    let e = env();
    let d = Diagnostics::open(&e.data, &rules(), true);
    d.check("steam", true, "/home/jo/.steam/steam");
    let mut o = opts(&e);
    o.home_dir = Some("/home/jo".into());
    let p = d.export(&e.out, &o).unwrap();
    let c = read_entry(&p, "checks.json");
    assert!(c.contains("~/.steam/steam") && !c.contains("/home/jo"));
}

#[test]
fn own_account_id_only_when_chosen() {
    for include in [false, true] {
        let e = env();
        std::fs::write(e.logs.join("a.log"), "t INFO account 4242 ready\n").unwrap();
        let d = Diagnostics::open(&e.data, &rules(), true);
        let mut o = opts(&e);
        o.me = Some(4242);
        o.include_account_id = include;
        let p = d.export(&e.out, &o).unwrap();
        let log = read_entry(&p, "log/a.log");
        let about = read_entry(&p, "about.json");
        if include {
            assert!(log.contains("4242"));
            assert!(about.contains("\"account_id\": 4242"));
        } else {
            assert!(!log.contains("4242") && log.contains("[me]"));
            assert!(!about.contains("account_id"));
        }
    }
}

#[test]
fn other_players_are_never_written_to_the_store() {
    let e = env();
    let d = Diagnostics::open(&e.data, &rules(), true);
    d.redact_also(99887766, "Wraith_Main");
    d.usage("x");
    for f in std::fs::read_dir(&e.data).unwrap() {
        let t = std::fs::read_to_string(f.unwrap().path()).unwrap();
        assert!(!t.contains("Wraith_Main") && !t.contains("99887766"));
    }
}
