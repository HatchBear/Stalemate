# 14 — Data and Content Assets

## Purpose

Lists every non-art asset: config files, the model, prompt templates, UI strings, test fixtures and optional audio.

## Scope

- **Covers:** DATA-001 to DATA-009, AUD-001 and AUD-002.
- **Note:** All thresholds referenced elsewhere in this doc set live here (NFR-MAINT-002).

## Details

### Asset list

| ID | Name | Format | Path | Usage | Priority | Status |
|---|---|---|---|---|---|---|
| DATA-001 | Rules and thresholds | TOML | `config/rules.toml` | All tunable constants and log patterns | Alpha | Not started |
| DATA-002 | Objective schedule | TOML | `config/objectives.toml` | Timers and flex slot rules (FR-LIVE-012, FR-LIVE-013) | v1 | Blocked on OQ-09 |
| DATA-003 | Live-Safe whitelist | TOML | `config/live_safe.toml` | Fields enabled after in-game verification | Alpha | Not started |
| DATA-004 | Win-probability model | JSON, one model for all patches | `models/model.json` | PRG-15 | Beta | Not started |
| DATA-005 | LLM prompt templates | Text with `{placeholders}` | `prompts/*.txt` | PRG-17 | Beta | Not started |
| DATA-006 | UI strings and template text | Fluent `.ftl` [ASSUMPTION] | `i18n/en-US/*.ftl` | All UI text; rules-only Review text | Alpha | Not started |
| DATA-007 | Default settings | TOML | `config/settings.default.toml` | First-run settings | Alpha | Not started |
| DATA-008 | Test fixtures | Recorded fragments, `.dem`, JSON | `tests/fixtures/` | [16](16-testing-and-qa.md) | Alpha | Not started |
| DATA-009 | Build importer registry | TOML | `config/importers.toml` | PRG-19 site adapters | v1 (Should) | Not started. Adapters are added as sites are chosen (OQ-06, DEC-031) |
| AUD-001 | Overlay alert sound (optional) | OGG Vorbis | `assets/audio/` | New swap suggestion | Dropped | Dropped (DEC-045) |
| AUD-002 | UI navigation sounds | OGG Vorbis | `assets/audio/ui/` | Ticks and mechanical noises for menu navigation in the main window | v1 (Could) | Not started. Exact sounds open (CD-12) |

### DATA-001 `rules.toml`

Every value marked [ASSUMPTION] in docs 04–09 appears here with that value.

```toml
[match_watcher]           # OQ-03: fill from real logs (T-006)
start    = ""             # regex
end      = ""             # regex
match_id = ""             # regex with one capture group
exit     = ""             # regex

[api]
analytics_per_min   = 160     # 80% of 200
players_per_sec     = 80      # 80% of 100
other_per_min       = 60
history_ttl_s       = 21600
analytics_ttl_s     = 86400

[broadcast]
backoff_s           = [2, 4, 8, 16, 32]
stale_delay_s       = 120

[scouting]
swap_closed_after_s     = 0      # VERIFY (T-007): scouting stays hidden until game time passes this
history_window          = 20
mmr_window              = 20
build_sample_matches    = 30
low_sample_below        = 10
core_rate               = 0.60
core_max_cond_gap       = 0.20
counter_mult            = 2.0
counter_lift            = 1.3
situational_rate        = 0.15
min_enemy_matches       = 3
smurf_max_lifetime      = 100
smurf_min_last20_wr     = 0.70
smurf_mmr_slope_pct     = 95
session_gap_min         = 30
session_loss_streak     = 3
session_hours           = 3

[build_matcher]
candidates              = 50
p_in                    = 0.90
p_out                   = 0.05
unclear_below           = 0.40
prior_player_weight     = 0.7
min_player_matches      = 10

[projector]
order_weight            = 0.6
transition_weight       = 0.4
order_step              = 0.2

[counter]
lane_phase_end_s        = 540
min_matches             = 200
refresh_min_s           = 30
projected_value_weight  = 0.5
affordability_income_s  = 120
top_window              = 5
top_overlay             = 3

[build_check]
margin                  = 0.02
max_swaps               = 2

[pace]
timing_tolerance_s      = 60
timer_lead_s            = 30

[review]
min_match_s             = 600
sample_every_s          = 30
accuracy_k              = 10
tiers                   = [0.00, 0.02, 0.05, 0.10, 0.20]   # T1..T5 upper bounds; above = T6
t7_low                  = 0.40
t7_even                 = [0.40, 0.60]
t7_high                 = 0.65
t7_min_share            = 0.33
t8_min_gain             = 0.10
t9_enemies_dead         = 2
t9_window_s             = 30
key_moments             = 5
dedupe_priority         = ["D-DEATH", "D-FIGHT", "D-OBJ"]
obj_weights             = { guardian = 1, walker = 2, base = 3 }
r01_unused_s            = 3
r02_lane_deficit        = 1500
r03_camp_gap_pp         = 15
r04_late_s              = 180
r05_min_score           = 0.03
r05_window_s            = 180
r06_creep_share         = 0.40
r07_hs_gap_pp           = 10
r08_offplan_count       = 3
r09_window_s            = 120
r10_absent_count        = 2
r11_parry_gap_pp        = 20

[overlay]
max_redraw_hz           = 1

[retention]
recordings_keep_matches = 50
recordings_keep_days    = 30

[updates]
check_on_start          = true   # FR-SET-004
```

### DATA-002 `objectives.toml`

The structure is fixed. All values wait on OQ-09, so none are filled in here. The author will hand-fill them from the current patch before v1 and re-check them on patch day (DEC-032).

```toml
[[timer]]
id          = ""      # e.g. a neutral or boss spawn; VERIFY
first_s     = 0       # VERIFY
interval_s  = 0       # VERIFY; 0 = once

[[flex_slot]]
slot        = 1
requires    = []      # objective IDs; VERIFY against the current patch
```

### DATA-003 `live_safe.toml`

```toml
# Fields from 03-fair-play-and-compliance.md. Add a field only after it's verified in-game (T-007).
enabled = ["Ls02Name"]   # LS-01, LS-03, LS-04, LS-10, LS-12 and LS-14 are always on (confirmed)
# Pending verification: Ls05AllyItems, Ls06Kda, Ls07Level, Ls09Lane, Ls11Objectives, Ls13Flex
# LS-08 enemy souls stays off until verified.
```

### DATA-004 model file

```json
{ "patches": ["string"], "trained_at": "RFC 3339", "matches": 20000,
  "features": ["bias", "t", "nw", "nw_t", "obj", "alive"],
  "beta": [0, 0, 0, 0, 0, 0], "brier": 0.0 }
```

### DATA-005 prompt templates

| File | Variables | Output limit |
|---|---|---|
| `prompts/key_moment.txt` | `{facts}`, `{voice}` | 2 sentences |
| `prompts/fix.txt` | `{facts}`, `{voice}` | 2 sentences |
| `prompts/phase_summary.txt` | `{facts}`, `{voice}` | 3 sentences |

Fixed technical instructions in every template:

- Use only facts in `{facts}`.
- Don't state any number that isn't in `{facts}`.
- Don't recommend items or actions that aren't in `{facts}`.

`{voice}` is filled from the CD-08 decision.

### DATA-006 strings

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-13 — Wording of all UI copy: labels, empty states, errors, setup instructions, and rules-only Review templates
> Constraints from the project: en-US first [ASSUMPTION]. Overlay rows must fit OV-1 to OV-5 limits ([09](09-ui-window-and-overlay.md)). Template text must work without an LLM (FR-REV-015). Must include the "not affiliated with Valve" notice (FP-9).
> Options to consider (optional): —
> Decision: Claude drafts the en-US strings during implementation, and the author reviews them (2026-11-05).

### DATA-008 fixtures

| Set | Contents | Minimum | Source |
|---|---|---|---|
| FX-BC | Broadcast recordings of the developer's own matches | 20 matches | PRG-27 |
| FX-DEM | Replay `.dem` files of the same matches | 5 matches | Developer |
| FX-API | Recorded deadlock-api responses for those matches | All calls made | PRG-27 |
| FX-LABEL | Hand-labeled Build Profiles (TST-024) and build matches at 15:00 (TST-042) | 30 profiles, 100 matches | Developer |
| FX-PATCH | One recording per new patch | 1 per patch | PRG-27 |

Chat text is stripped from committed fixtures [ASSUMPTION].

### AUD-001 (optional)

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-12 — Whether the overlay plays a sound, and what it sounds like
> Constraints from the project: If used: OGG Vorbis, 48 kHz, ≤ 1.0 s, peak ≤ −1 dBFS, off by default, volume setting 0–100%. Must not resemble Deadlock's own sounds.
> Options to consider (optional): No sound; one sound for swap suggestions.
> Decision: Partly decided (2026-11-05, DEC-039). The app has UI navigation sounds, on by default, as their own asset (AUD-002). The overlay alert sound (AUD-001) is dropped: the overlay stays silent (DEC-045).

### AUD-002 (UI navigation sounds)

An asset of its own, separate from AUD-001 (DEC-039). Small ticks and mechanical noises for menu navigation in the main window.

- **Format:** OGG Vorbis, 48 kHz, ≤ 0.3 s each, peak ≤ −1 dBFS [ASSUMPTION]. Must not resemble Deadlock's own sounds.
- **Behavior:** On by default. Settings has a mute toggle and a volume setting from 0 to 100% (FR-UI-009). Sounds play only for interactions in the main window, never in the overlay [ASSUMPTION].
- **Set:** One sound each for move, select and back, plus one for an error [ASSUMPTION count]. The exact sounds are still open.

## Dependencies

- [03 Fair play](03-fair-play-and-compliance.md): DATA-003.
- [04](04-data-sources-and-ingestion.md)–[09](09-ui-window-and-overlay.md): thresholds.
- [07 Review](07-post-match-review-engine.md): DATA-004, DATA-005, CD-08.
- [12 Programs](12-programs-and-code.md)
- [16 Testing](16-testing-and-qa.md): DATA-008.

## Open Questions

- OQ-09: What are the timer and flex slot values?
- OQ-06: Which sites go in DATA-009?
- [ASSUMPTION] Fluent for strings, en-US only for v1, stripping chat from fixtures.
- [ASSUMPTION] AUD-002 clip count and length, and main-window-only playback.

## Acceptance Criteria

- [ ] Every threshold quoted in docs 04–09 appears in DATA-001 with the same value.
- [ ] The app starts with only DATA-001, DATA-003, DATA-006 and DATA-007 present.
- [ ] No model file ships with Brier > 0.20.
