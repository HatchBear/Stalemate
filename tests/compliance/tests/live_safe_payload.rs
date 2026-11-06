//! TST-030 payload check (NFR-FAIR-001 to 003, FP-4, SM-9).
//!
//! Replays FX-BC fixtures through `GameState` and the Live-Safe filter, then validates every
//! payload with `live_safe::check_payload`. Run with `--features fixtures`.
//!
//! Fixture layout (assumed here; T-013 and the fixture recorder must produce it, see
//! tests/fixtures/README.md):
//!
//! ```text
//! tests/fixtures/fx-bc/<match>/meta.json     {"match_id": 1, "me": 100, "started_unix": 1700000000}
//! tests/fixtures/fx-bc/<match>/events.jsonl  one normalized `ParserEvent` per line, chat stripped
//! ```
//!
//! With no fixtures the replay test prints a message and passes without checking anything.
//! Set `STALEMATE_REQUIRE_FX_BC=<n>` to fail unless at least `n` matches were replayed (TST-030
//! asks for 20).

use stalemate_core::live_safe::{check_payload, Whitelist};

/// Negative control: the checker must reject keys that are never Live-Safe. Always runs, so a
/// broken checker can't make the fixture replay pass by accident.
#[test]
fn check_payload_rejects_non_live_safe_keys() {
    let wl = Whitelist::default();
    let clean = serde_json::json!({
        "match_id": 1, "t_game_s": 10, "delay_s": 0,
        "players": [{"account_id": 1, "team": 0, "hero_id": 7, "items": []}],
        "self": {"souls": 0, "items": [], "cooldowns": {}}
    });
    assert!(check_payload(&clean, &wl).is_ok());

    for bad_key in ["position", "health", "enemy_souls", "chat"] {
        let mut p = clean.clone();
        p["players"][0][bad_key] = serde_json::json!(1);
        let err = check_payload(&p, &wl).expect_err(bad_key);
        assert!(
            err.iter().any(|k| k.contains(bad_key)),
            "{bad_key} not reported: {err:?}"
        );
    }
}

#[cfg(feature = "fixtures")]
mod replay {
    use std::path::{Path, PathBuf};
    use std::time::{Duration, SystemTime};

    use stalemate_core::game_state::GameState;
    use stalemate_core::live_safe::{check_payload, project_state, Ls, Whitelist};
    use stalemate_core::types::ParserEvent;

    fn repo_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
    }

    fn match_dirs() -> Vec<PathBuf> {
        let dir = repo_root().join("tests/fixtures/fx-bc");
        let Ok(rd) = std::fs::read_dir(&dir) else {
            return Vec::new();
        };
        let mut v: Vec<PathBuf> = rd
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.join("events.jsonl").is_file() && p.join("meta.json").is_file())
            .collect();
        v.sort();
        v
    }

    /// Replay one match; check the payload at every Tick. Returns the number of payloads checked.
    fn replay(dir: &Path, wl: &Whitelist) -> usize {
        let meta: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(dir.join("meta.json")).unwrap())
                .unwrap_or_else(|e| panic!("{}: bad meta.json: {e}", dir.display()));
        let field = |k: &str| {
            meta[k]
                .as_u64()
                .unwrap_or_else(|| panic!("{}: meta.json needs a number '{k}'", dir.display()))
        };
        let started = SystemTime::UNIX_EPOCH + Duration::from_secs(field("started_unix"));
        let mut gs = GameState::new(
            field("match_id"),
            field("me").try_into().expect("me fits AccountId"),
            started,
        );
        let mut checked = 0;
        let text = std::fs::read_to_string(dir.join("events.jsonl")).unwrap();
        for (i, line) in text
            .lines()
            .enumerate()
            .filter(|(_, l)| !l.trim().is_empty())
        {
            let ev: ParserEvent = serde_json::from_str(line)
                .unwrap_or_else(|e| panic!("{}:{}: bad event: {e}", dir.display(), i + 1));
            let is_tick = matches!(ev, ParserEvent::Tick { .. });
            // Zero delay: the payload content, not the timing, is what is checked here.
            let received = match &ev {
                ParserEvent::Tick { t_game } => started + Duration::from_secs(u64::from(*t_game)),
                _ => started + Duration::from_secs(u64::from(gs.t_game)),
            };
            gs.apply(&ev, received);
            if is_tick {
                let payload = serde_json::to_value(project_state(&gs, wl)).unwrap();
                if let Err(bad) = check_payload(&payload, wl) {
                    panic!("{}:{}: non-Live-Safe keys: {bad:?}", dir.display(), i + 1);
                }
                checked += 1;
            }
        }
        checked
    }

    #[test]
    fn fx_bc_payloads_contain_only_live_safe_keys() {
        let dirs = match_dirs();
        let required: usize = std::env::var("STALEMATE_REQUIRE_FX_BC")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);
        if dirs.is_empty() {
            eprintln!(
                "SKIPPED: no FX-BC fixtures in tests/fixtures/fx-bc/ (blocked on T-013, \
                 the author records them). Nothing was checked."
            );
            assert!(
                required == 0,
                "STALEMATE_REQUIRE_FX_BC={required} but no fixtures found"
            );
            return;
        }
        let configured = Whitelist::load(&repo_root().join("config/live_safe.toml"))
            .expect("config/live_safe.toml");
        // The filter must also hold if every pending field were enabled.
        let widest = Whitelist(Ls::ALL.into_iter().collect());
        let mut payloads = 0;
        for d in &dirs {
            payloads += replay(d, &configured) + replay(d, &widest);
        }
        eprintln!(
            "checked {payloads} payloads over {} FX-BC matches",
            dirs.len()
        );
        if dirs.len() < 20 {
            eprintln!(
                "WARN: TST-030 asks for 20 FX-BC matches, only {} found",
                dirs.len()
            );
        }
        assert!(
            dirs.len() >= required,
            "need {required} FX-BC matches, found {}",
            dirs.len()
        );
    }
}
