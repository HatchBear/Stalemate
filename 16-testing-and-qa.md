# 16 — Testing and QA

## Purpose

Defines how every requirement is verified: test levels, the full test case list, fixtures, the platform matrix and patch-day procedure.

## Scope

- **Covers:** All TST IDs referenced in [01](01-overview.md), [02](02-requirements.md) and the system docs.
- **Elsewhere:** Unit tests per program (TC-xx) are in [12](12-programs-and-code.md).

## Details

### Test levels

| Level | What | Where | When |
|---|---|---|---|
| Unit | TC-xx cases in [12](12-programs-and-code.md) | `cargo test`, `dotnet test` | Every push (CI) |
| Fixture integration | Pipelines run against recorded matches (DATA-008) | `cargo test --features fixtures` | Every push (CI) |
| Compliance | Live-Safe and fair-play checks | CI and code review | Every push |
| Performance | CPU, memory and latency | Test machines | Before each release |
| Platform manual | Overlay, paths, install | Platform matrix below | Before each release |
| Beta field testing | Real matches by testers | Beta channel | During Beta |

### Test cases

| ID | Verifies | Method | Pass criteria |
|---|---|---|---|
| TST-010 | FR-DATA-001, SM-1 | Replay recorded `console.log` lines into a temp file with real timing | Start and end events ≤ 5 s after each line in 20/20 matches |
| TST-011 | FR-DATA-002 | Same as TST-010 | Correct match ID in 20/20 |
| TST-012 | FR-DATA-003, NFR-REL-001 | FX-BC through a local HTTP server, with drops injected | All fragments consumed; backoff 2/4/8/16/32 s; `BroadcastLost` after 5 failures |
| TST-013 | FR-DATA-004 | 1-hour soak against a mock with limit counters | Zero requests over any published limit |
| TST-014 | FR-DATA-005 | Full FX-BC match | Recording complete; Review falls back to FX-DEM when the recording is deleted |
| TST-015 | FR-DATA-006 | Steam config fixtures, plus manual entry | Correct Account ID in all cases |
| TST-016 | FR-DATA-007 | Broadcast disabled | Scouting fills from manual entry and the API |
| TST-017 | FR-DATA-008, NFR-MAINT-001 | Grep for hard-coded hero or item IDs; start against mock assets | No hard-coded lists; assets loaded and cached per patch |
| TST-018 | NFR-REL-002 | Force haste to fail on FX-DEM | PRG-21 produces the same events (TC-04-5) |
| TST-019 | FR-DATA-009 | Mock API-17 returning a replay URL, with the recording deleted | Replay downloaded and Review produced. A failed or non-compliant download falls back to a stats-only Review |
| TST-020 | FR-SCOUT-001, FR-SCOUT-015, SM-2 | 11 mocked players with 200 ms latency | Basic fields of all cards filled ≤ 30 s; W/L values match fixtures |
| TST-021 | FR-SCOUT-002 | MMR fixture | Rank and slope match hand calculation |
| TST-022 | FR-SCOUT-003, FR-SCOUT-004 | Hero-stats fixture | Top 5 and lifetime matches correct |
| TST-023 | FR-SCOUT-005 | API-06 fixture | Most-selected build and core items correct |
| TST-024 | FR-SCOUT-006 | FX-LABEL 30 profiles | ≥ 80% agreement with hand labels |
| TST-025 | FR-SCOUT-007 | Fixtures | Max-first ability and median item times correct |
| TST-026 | FR-SCOUT-008 | Fixtures | Percentiles match hand calculation within 1 point |
| TST-027 | FR-SCOUT-009 | Crafted players for each flag rule | Each flag fires only when its rule is met |
| TST-028 | FR-SCOUT-010, FR-REV-017 | Add, edit and delete notes | Notes persist and appear on the matching Scouting Card |
| TST-029 | FR-SCOUT-011, FR-SCOUT-012 | Fixtures plus local DB | Matchup and repeat-player records correct |
| TST-0291 | FR-SCOUT-013 | Simulated Session: 3 losses; and 3 h | Reminder in the main window; never in the overlay |
| TST-0292 | FR-SCOUT-016 | API-13 fixture | Ban rates shown for the rank range |
| TST-030 | NFR-FAIR-001 to 003, FP-4, SM-9 | Run 20 FX-BC matches in Live Mode; validate every overlay and window payload against the whitelist; inspect linked symbols | Zero non-Live-Safe keys; no memory, injection, hook, packet or input APIs linked |
| TST-031 | FR-LIVE-015 | Code review and payload check | No fight data in Live Mode |
| TST-032 | NFR-FAIR-004 | Mock 404 for one player | "No public data"; no retries in 10 min |
| TST-033 | NFR-FAIR-005 | Network capture during a full match | No `api.steampowered.com` calls about other players |
| TST-040 | FR-LIVE-001 | FX-BC | Enemy board matches the fixture at 5 checkpoints |
| TST-041 | FR-LIVE-002 | FX-BC, hand-checked | Every enemy purchase logged with correct `t_game` and action |
| TST-042 | FR-LIVE-003, SM-3 | FX-LABEL 100 matches at 15:00 | Top-1 accuracy ≥ 60% |
| TST-043 | FR-LIVE-004 | FX-BC | For each enemy, ≥ 1 of the projected 3 items is bought within the next 5 min in ≥ 50% of checks [ASSUMPTION target] |
| TST-044 | FR-LIVE-005, FR-LIVE-006 | FX-BC at 8:50 and 9:10 | Lane opponents only, then all enemies |
| TST-045 | FR-LIVE-007 | Mock API-07 and API-08 | Ranking equals the formula in [06](06-in-match-recommendations.md) |
| TST-046 | FR-LIVE-008 | Inject an enemy purchase | Recommendations refreshed ≤ 30 s later |
| TST-047 | FR-LIVE-009 | Crafted plan and scores | Swaps match TC-13 rules |
| TST-048 | FR-LIVE-010, FR-LIVE-011 | Fixtures | Pace and timing values correct |
| TST-049 | FR-LIVE-012 | DATA-002 rules plus a fixture | Flex slot requirements correct |
| TST-0491 | FR-LIVE-013 | DATA-002 timers | Reminders at the configured times |
| TST-0492 | FR-LIVE-014, FR-UI-006 | FX-BC | Every top-3 change recorded and listed in the window |
| TST-0493 | FR-LIVE-016 | Mock matchup, synergy and counter tables for 12 heroes | Predicted win rate matches the hand calculation. Nothing is shown before the roster arrives, and the overlay payload never contains it |
| TST-050 | FR-UI-001 | Manual walkthrough of every view and data state | All views reachable; all states render |
| TST-051 | FR-UI-002 | Payload inspection | Overlay shows only OV-1 to OV-5 |
| TST-052 | FR-UI-003 | Manual on each platform | Clicks pass through; hotkey toggles |
| TST-053 | FR-UI-007 | Mode changes | Overlay hidden outside Live Mode |
| TST-054 | FR-UI-008 | FX-BC with known delay | Delay shown within ±2 s |
| TST-055 | NFR-PERF-003, SM-4 | Timestamp data arrival and redraw | Latency ≤ 1 s; ≤ 1 redraw/s |
| TST-060 | FR-REV-001, NFR-PERF-004, SM-5 | 20 FX-BC matches end-to-end | Record saved for every match; rules Review ≤ 60 s after the Broadcast ends |
| TST-061 | SM-6 | PRG-26 holdout of 2,000 matches | Brier ≤ 0.20 |
| TST-062 | FR-REV-002, FR-REV-014 | Fixture Reviews | Three phases present; 1–3 fixes ranked by total EPL |
| TST-063 | FR-REV-003, FR-REV-004 | Fixtures, hand-checked | Lane grade and soul sources within 1% of hand values |
| TST-064 | FR-REV-005 | Fixture deaths | Every death listed; R-01 correct on 10 hand-labeled deaths |
| TST-065 | FR-REV-006, FR-REV-007 | Fixtures | Damage, mitigation, aim and parry values match hand checks |
| TST-066 | FR-REV-008 | Fixture with a known plan | Deviations and R-09 results correct |
| TST-067 | FR-REV-009 | Mock benchmarks | Comparisons correct |
| TST-068 | FR-REV-010 to FR-REV-012 | Run each fixture Review 3 times | Identical tiers, accuracy and Key Moments every run |
| TST-069 | FR-REV-013 | Mock LLM returning valid and invalid text | Invalid rejected; template used after 2 tries |
| TST-0691 | FR-REV-015 | LLM Off | Complete Review with template text, no network calls to any LLM |
| TST-0692 | FR-REV-016 | 60 synthetic matches over 6 weeks | Trends match hand calculation |
| TST-070 | NFR-PERF-001, SM-7 | Profile during 3 live matches | Average CPU ≤ 3% of one core |
| TST-071 | NFR-PERF-002, SM-8 | Same | RSS ≤ 400 MB |
| TST-080 | FR-BUILD-001 | API-16 fixtures | 20 builds for 5 heroes, all items resolved |
| TST-081 | FR-BUILD-002 | One fixture per adapter | Imports succeed; unresolved names listed |
| TST-082 | FR-BUILD-003 | Builder plus export/import | Round trip unchanged |
| TST-083 | FR-BUILD-004, FR-BUILD-005 | Fixture with a selected build; manual pick | Priority order from [08](08-builds-import-and-builder.md) |
| TST-084 | FR-BUILD-006 | Simulated patch | Affected builds flagged |
| TST-090 | FR-UI-004, NFR-PLAT-001 | KDE Plasma 6 Wayland, Deadlock borderless windowed, Flatpak | Overlay above the game, click-through, hotkey works |
| TST-091 | FR-UI-005, NFR-PLAT-003 | Windows 11 and 10, borderless windowed | Same as TST-090 |
| TST-092 | NFR-PLAT-002 | Native Steam; Flatpak Steam; library on a second drive granted by portal | Deadlock and `console.log` found in all three |
| TST-093 | NFR-PLAT-004 | GNOME Wayland and an X11 session | Main window works; overlay result recorded |
| TST-100 | FR-SET-001 | Each provider | Off, Local and Own key all work; keys only in the keyring |
| TST-101 | FR-SET-002 | Export, then delete | JSON export complete; database empty afterward |
| TST-102 | FR-SET-003 | Fresh install without `-condebug`, then with it | Setup reports fail, then pass |
| TST-103 | NFR-PRIV-001 | Network capture over a full session | No traffic except deadlock-api, the Broadcast, the configured LLM and the release check |
| TST-104 | NFR-PRIV-002 | Release checklist | Privacy policy URL resolves |
| TST-105 | [11](11-data-model.md) | Migrations on an empty and a populated database | Idempotent, no data loss |
| TST-106 | FR-SET-004 | Mock GitHub Releases response; toggle off | Newer version shown; no request when the toggle is off |
| TST-107 | FR-UI-009 | Navigate the main window; toggle mute; change volume | Sounds play by default and respect mute and volume. None play from the overlay |
| TST-110 | [13](13-art-and-asset-pipeline.md) | `assets-check` CI job | All AST files present; SVG rules met; no placeholders in v1 builds |

### Verified by review, not by test

| Requirement | Method |
|---|---|
| NFR-COST-001, NFR-COST-002 | Design review at each milestone exit: no developer-run server, and LLM features cost the developer nothing by default |
| NFR-MAINT-002 | Code review: no numeric thresholds in M-08 to M-14 outside DATA-001 |
| FR-SCOUT-014 | Won't build; nothing to verify |

FR-REV-011 is covered by TST-068, and NFR-FAIR-002 and NFR-FAIR-003 by TST-030 (listed as ranges in those rows).

### Platform matrix

| Platform | Steam | Overlay | Required for |
|---|---|---|---|
| KDE Plasma 6, Wayland | Native | Yes | Alpha (window), Beta (overlay) |
| KDE Plasma 6, Wayland | Flatpak | Yes | Alpha (window), Beta (overlay) |
| Windows 11 | Native | Yes | Beta |
| Windows 10 | Native | Yes | v1 |
| GNOME Wayland | Native | Best effort | v1 |
| KDE Plasma 6, X11 | Native | Best effort | v1 |

### Patch-day procedure

Within 24 h of a Deadlock patch [ASSUMPTION]:

1. Record one match with PRG-27 (FX-PATCH).
2. Run `just fixtures-verify`.
3. If haste fails: confirm PRG-21 works; open an issue upstream; ship a notice in the app.
4. Refresh the API-15 asset cache; re-validate builds (FR-BUILD-006).
5. Re-check the DATA-002 timers and flex slot rules against the patch (DEC-032).
6. Check the DATA-004 model's Brier score on the new patch's matches once enough are available. Rerun PRG-26 only if it exceeds 0.20.

### Beta field testing [ASSUMPTION]

- At least 20 testers: at least 10 on KDE Plasma and at least 5 on Windows.
- In-app "report a problem" opens a prefilled GitHub issue with logs attached by the user (never automatically).
- Severity: S1 crash or fair-play violation (fix before next release); S2 wrong recommendation or Review (fix within 2 releases); S3 cosmetic.

## Dependencies

- [01 Overview](01-overview.md): SM targets.
- [02 Requirements](02-requirements.md)
- [12 Programs](12-programs-and-code.md): TC cases.
- [14 Data assets](14-data-and-content-assets.md): DATA-008.
- [15 Tools](15-tools-build-and-deploy.md): CI.

## Open Questions

- [ASSUMPTION] The TST-043 projection target, the 24 h patch-day window, and the Beta tester counts.

## Acceptance Criteria

- [ ] Every requirement in [02](02-requirements.md) maps to at least one TST here.
- [ ] Every TST referenced anywhere in the doc set exists here.
- [ ] CI runs unit, fixture and compliance levels on every push.
