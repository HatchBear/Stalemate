# 02 — Requirements

## Purpose

Lists every functional and non-functional requirement with a unique ID, MoSCoW priority, target phase, source and test.

## Scope

- **Covers:** Requirements for the v1 release and its Alpha and Beta milestones.
- **Does not cover:** Design detail, which lives in the system docs linked from each section.

## Details

### Conventions

- **Priority:** MoSCoW relative to the v1 release (Must, Should, Could, Won't). The user asked for v1 to "ideally have everything" (DEC-012). Features the user specified are Must; features proposed in the Feature Breakdown and kept by the user are Should [ASSUMPTION].
- **Phase:** The milestone where the requirement must first pass ([17](17-roadmap.md)).
- **Source:**
    - **U:** The user stated it.
    - **FB:** Proposed in the Feature Breakdown and kept by the user.
    - **R:** Research finding.
    - **DEC-nnn:** Decision log entry.
- **Test:** QA test ID in [16](16-testing-and-qa.md).

### Data and detection (design: [04](04-data-sources-and-ingestion.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-DATA-001 | Detect match start and match end from the game console log within 5 s of the log line being written | Must | Alpha | DEC-004 | TST-010 |
| FR-DATA-002 | Obtain the match ID of the current match | Must | Alpha | DEC-004 | TST-011 |
| FR-DATA-003 | Connect to the Broadcast of the current match and keep consuming it until it ends, tolerating the Broadcast delay (about 25 s measured) | Must | Alpha | DEC-004, DEC-006, DEC-050 | TST-012 |
| FR-DATA-004 | Query deadlock-api with per-endpoint rate limiting and response caching | Must | Alpha | R | TST-013 |
| FR-DATA-005 | Save the full Broadcast of the user's matches locally for Review; fall back to a downloaded replay file | Must | Alpha | U | TST-014 |
| FR-DATA-006 | Identify the user's Account ID from local Steam files, or from manual entry if that fails | Must | Alpha | [ASSUMPTION] | TST-015 |
| FR-DATA-007 | Keep scouting from deadlock-api alone when the Broadcast is unavailable | Should | Beta | [ASSUMPTION] | TST-016 |
| FR-DATA-008 | Load hero and item data from deadlock-api assets at startup, cached per game patch | Must | Alpha | R | TST-017 |
| FR-DATA-009 | Download a replay file through deadlock-api's replay URL (API-17) when the Broadcast recording is missing or incomplete | Should | Beta | DEC-027 | TST-019 |

### Player scouting (design: [05](05-player-scouting.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-SCOUT-001 | Show each player's overall win/loss and win/loss over their last 20 matches | Must | Alpha | U | TST-020 |
| FR-SCOUT-002 | Show each player's rank and MMR trend over their last 20 ranked matches | Must | Alpha | FB | TST-021 |
| FR-SCOUT-003 | Show each player's mains: top 5 heroes by matches, with matches and win rate on each | Must | Alpha | U | TST-022 |
| FR-SCOUT-004 | Show each player's lifetime matches on the hero they're playing now | Should | Alpha | FB | TST-022 |
| FR-SCOUT-005 | Show each player's Build Profile for their current hero: most-selected build and core items | Must | Beta | U | TST-023 |
| FR-SCOUT-006 | Apply the Counter-buy filter so counter buys aren't counted as core items | Must | Beta | U | TST-024 |
| FR-SCOUT-007 | Show power spikes: the ability they max first and the median timing of their core items | Should | Beta | FB | TST-025 |
| FR-SCOUT-008 | Show a playstyle profile as percentiles of available stats | Must | Beta | U, DEC-014 | TST-026 |
| FR-SCOUT-009 | Flag possible smurfs, party stacks and players you've met before | Should | v1 | FB | TST-027 |
| FR-SCOUT-010 | Show the user's saved notes on each player | Should | v1 | FB | TST-028 |
| FR-SCOUT-011 | Show the user's record against each enemy hero and with each teammate's hero | Should | Beta | FB | TST-029 |
| FR-SCOUT-012 | Show the user's record in past matches with or against each specific player | Should | v1 | FB | TST-029 |
| FR-SCOUT-013 | Show a session reminder after 3 consecutive losses or 3 h of play in a Session | Should | v1 | FB | TST-0291 |
| FR-SCOUT-014 | Per-player ban habits | Won't | — | DEC-017 | — |
| FR-SCOUT-015 | Fill the basic Scouting Card fields (FR-SCOUT-001 to 004) within 30 s of the roster appearing in the Broadcast. Later fields fill in afterward | Must | Alpha | SM-2 | TST-020 |
| FR-SCOUT-016 | Show match-level hero ban rates for the current rank range | Could | v1 | DEC-017 [ASSUMPTION] | TST-0292 |

### In-match recommendations (design: [06](06-in-match-recommendations.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-LIVE-001 | Show an enemy board with each enemy's hero, items and the other Live-Safe fields | Must | Alpha | U | TST-040 |
| FR-LIVE-002 | Keep a Purchase Log of every item each enemy buys or sells, with game time | Must | Alpha | U (DEC-015) | TST-041 |
| FR-LIVE-003 | Detect each enemy's most likely build from their items, with a confidence value | Must | Beta | U | TST-042 |
| FR-LIVE-004 | Project each enemy's next 3 items | Must | Beta | U | TST-043 |
| FR-LIVE-005 | During the Lane phase, recommend counter items against the user's lane opponents | Must | Beta | U | TST-044 |
| FR-LIVE-006 | After the Lane phase, recommend counter items against the whole enemy team | Must | Beta | U | TST-044 |
| FR-LIVE-007 | Rank counter items by adjusted win rate | Must | Beta | R | TST-045 |
| FR-LIVE-008 | Use the Purchase Log as an input to counter-item recommendations | Must | Beta | U (DEC-015) | TST-046 |
| FR-LIVE-009 | Suggest which items in the user's Build plan to replace, and with what | Must | Beta | U | TST-047 |
| FR-LIVE-010 | Show the user's souls per minute against the benchmark for their hero and rank | Should | v1 | FB | TST-048 |
| FR-LIVE-011 | Show whether the user is on schedule for their next core item | Should | v1 | FB | TST-048 |
| FR-LIVE-012 | Show which objectives would unlock the user's next flex slot | Should | v1 | FB | TST-049 |
| FR-LIVE-013 | Show reminders based only on the match clock and the game's fixed schedule | Should | v1 | FB | TST-0491 |
| FR-LIVE-014 | Keep every past item recommendation from the match, time-stamped, in the main window | Must | Beta | U (DEC-010) | TST-0492 |
| FR-LIVE-015 | Do not track or analyze fights during a match | Must | Alpha | U (DEC-005) | TST-031 |
| FR-LIVE-016 | Show a predicted win rate for the user's team from both teams' hero compositions (lane matchups, synergy, counters), once the roster is known. Main window only | Should | Beta | U (DEC-041) | TST-0493 |

### Review (design: [07](07-post-match-review-engine.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-REV-001 | Save a record of every match the user plays automatically | Must | Alpha | U | TST-060 |
| FR-REV-002 | Break the match into Lane phase (0–9 min), mid game (9–20) and late game (20+), with findings for each | Must | Beta | U | TST-062 |
| FR-REV-003 | Show the user's soul lead or deficit against their lane opponents at 9:00 | Must | Alpha | FB | TST-063 |
| FR-REV-004 | Show where the user's souls came from: lane troopers, neutral camps, breakables, Urn, Sinner's Sacrifice, plus Other for the rest. Urn shows as not available until a data field for it is verified (DEC-052) | Must | Alpha | FB, DEC-052 | TST-063 |
| FR-REV-005 | For each death, show time, place, souls lost, and abilities or items off cooldown and unused | Must | Beta | FB | TST-064 |
| FR-REV-006 | Show damage taken split by heroes, troopers and neutrals, with damage mitigated by items | Should | Beta | FB | TST-065 |
| FR-REV-007 | Show headshot rate, accuracy, and melee and parry usage | Should | v1 | FB | TST-065 |
| FR-REV-008 | Show whether the user followed their Build plan and whether each counter buy paid off | Must | Beta | U | TST-066 |
| FR-REV-009 | Compare farm, item timings, ability order and build against top players of the user's hero at the user's rank | Should | v1 | FB | TST-067 |
| FR-REV-010 | Classify each evaluated decision into one of 9 Tiers from expected points lost | Must | Beta | U (DEC-011) | TST-068 |
| FR-REV-011 | Compute a 0–100 accuracy score per match | Must | Beta | U (DEC-011) | TST-068 |
| FR-REV-012 | List the 5 Key Moments | Must | Beta | U (DEC-011) | TST-068 |
| FR-REV-013 | Generate explanations with an LLM only where rules can't, using only facts the rules produced | Must | Beta | U (DEC-011) | TST-069 |
| FR-REV-014 | Give 1–3 prioritized fixes for the next match | Must | Beta | U | TST-062 |
| FR-REV-015 | Produce a complete rules-only Review when no LLM is configured | Must | Beta | NFR-COST-002 | TST-0691 |
| FR-REV-016 | Show long-term trends: per-hero progress over weeks, and win rate by time of day and Session length | Should | v1 | FB | TST-0692 |
| FR-REV-017 | Let the user save notes on specific players | Should | v1 | FB | TST-028 |

### Builds (design: [08](08-builds-import-and-builder.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-BUILD-001 | Import in-game community builds through deadlock-api | Must | Alpha | U | TST-080 |
| FR-BUILD-002 | Import builds from external websites (site list is OQ-06). JSON and share-code import ship first | Should | v1 | U, DEC-031 | TST-081 |
| FR-BUILD-003 | Provide a manual build builder | Must | Alpha | U | TST-082 |
| FR-BUILD-004 | Read the user's selected in-game build from the Broadcast and use it as the Build plan when the user hasn't picked one for that hero | Should | Beta | DEC-013, DEC-026 | TST-083 |
| FR-BUILD-005 | Let the user pick the active Build plan manually. An explicit pick always overrides the in-game build | Must | Alpha | DEC-013, DEC-026 | TST-083 |
| FR-BUILD-006 | Flag builds with items removed or changed by a game patch | Should | Beta | [ASSUMPTION] | TST-084 |

### UI (design: [09](09-ui-window-and-overlay.md))

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-UI-001 | Main window with Home, Scouting, Live, Reviews, Builds and Settings views in Alpha, plus a Players view in v1 | Must | Alpha | U (DEC-010) | TST-050 |
| FR-UI-002 | Overlay that shows only currently needed info | Must | Beta | U (DEC-010) | TST-051 |
| FR-UI-003 | Overlay is click-through and toggled with a user-set global hotkey | Must | Beta | [ASSUMPTION] | TST-052 |
| FR-UI-004 | Overlay works on KDE Plasma Wayland | Must | Beta | U (DEC-007) | TST-090 |
| FR-UI-005 | Overlay works on Windows with the game in borderless windowed mode | Must | Beta | DEC-007 | TST-091 |
| FR-UI-006 | Main window shows good-to-know info, including past item recommendations | Must | Beta | U (DEC-010) | TST-0492 |
| FR-UI-007 | Overlay hides automatically outside Live Mode | Should | Beta | [ASSUMPTION] | TST-053 |
| FR-UI-008 | Show the current Broadcast delay in the overlay and window | Must | Beta | DEC-006 | TST-054 |
| FR-UI-009 | Play UI navigation sounds (AUD-002) in the main window. On by default, with mute and volume (0–100%) in Settings | Could | v1 | DEC-039 | TST-107 |

### Settings and privacy

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| FR-SET-001 | LLM provider setting: Off, Local (OpenAI-compatible HTTP on localhost) or Own API key | Must | Beta | DEC-009, DEC-011 | TST-100 |
| FR-SET-002 | Export all local data as JSON, and delete all local data | Should | v1 | [ASSUMPTION] | TST-101 |
| FR-SET-003 | First-run setup that explains how to add `-condebug` to Deadlock's launch options and checks it worked | Must | Alpha | R | TST-102 |
| FR-SET-004 | Check GitHub Releases for a newer app version at startup. On by default, and can be turned off in Settings | Should | Beta | DEC-028 | TST-106 |

### Non-functional requirements

| ID | Requirement | Priority | Phase | Source | Test |
|---|---|---|---|---|---|
| NFR-PLAT-001 | Ships as a Flatpak and runs on KDE Plasma 6 under Wayland | Must | Alpha | U (DEC-007) | TST-090 |
| NFR-PLAT-002 | Finds Deadlock under both the native Steam install and the Flatpak Steam install | Must | Alpha | [ASSUMPTION] | TST-092 |
| NFR-PLAT-003 | Runs on Windows 10 and 11, x64 | Must | Beta | DEC-007 | TST-091 |
| NFR-PLAT-004 | Main window works on other Linux desktops; overlay is best effort there | Could | v1 | [ASSUMPTION] | TST-093 |
| NFR-PERF-001 | Average CPU ≤ 3% of one core over 60 s in Live Mode | Must | Beta | SM-7 | TST-070 |
| NFR-PERF-002 | Resident memory ≤ 400 MB in Live Mode | Must | Beta | SM-8 | TST-071 |
| NFR-PERF-003 | Overlay redraws only when its data changes, at most 1 time per second | Must | Beta | SM-4 | TST-055 |
| NFR-PERF-004 | Rules-based Review ready ≤ 60 s after the Broadcast ends; LLM text ≤ 120 s more on the local provider | Must | Beta | SM-5 | TST-060 |
| NFR-FAIR-001 | Never read or write game memory, inject code, hook graphics or capture network packets | Must | Alpha | DEC-003 | TST-030 |
| NFR-FAIR-002 | Never send input to the game | Must | Alpha | DEC-003 | TST-030 |
| NFR-FAIR-003 | In Live Mode, every payload sent to the overlay or window contains only Live-Safe fields, enforced in code | Must | Alpha | DEC-003 | TST-030 |
| NFR-FAIR-004 | Never try to recover data for players who opted out of deadlock-api | Must | Alpha | R | TST-032 |
| NFR-FAIR-005 | Never call the Steam Web API for any player other than the user | Must | Alpha | R | TST-033 |
| NFR-PRIV-001 | Store all user data locally; no telemetry unless the user opts in | Must | Alpha | [ASSUMPTION] | TST-103 |
| NFR-PRIV-002 | Publish a privacy policy before the first public release | Must | v1 | R | TST-104 |
| NFR-COST-001 | No developer-run server is needed for any Must or Should feature | Must | Alpha | DEC-009 | Design review |
| NFR-COST-002 | LLM features cost the developer nothing by default | Must | Beta | DEC-009 | Design review |
| NFR-REL-001 | After a Broadcast drop, retry with backoff 2, 4, 8, 16, 32 s, then show "Broadcast lost" | Must | Alpha | [ASSUMPTION] | TST-012 |
| NFR-REL-002 | Replay and Broadcast parsing falls back to the .NET parser when the primary parser fails | Should | Beta | DEC-008 | TST-018 |
| NFR-MAINT-001 | No hero or item lists hard-coded; all game data loaded at runtime | Must | Alpha | R | TST-017 |
| NFR-MAINT-002 | All thresholds in this doc set live in config files ([14](14-data-and-content-assets.md)) | Must | Alpha | [ASSUMPTION] | Code review |

## Dependencies

- [01 Overview](01-overview.md): goals and metrics.
- [03](03-fair-play-and-compliance.md)–[09](09-ui-window-and-overlay.md): designs that satisfy these requirements.
- [16 Testing](16-testing-and-qa.md): test IDs.
- [17 Roadmap](17-roadmap.md): phases.

## Open Questions

- The Must/Should split (Must = user-specified; Should = Feature Breakdown additions) was approved on 2026-11-05 (DEC-038).
- OQ-06: Which external build sites should FR-BUILD-002 import from? FR-BUILD-002 is now Should, and JSON and share-code import ship first (DEC-031).
- OQ-08 (resolved): Keep FR-SCOUT-016 (match-level ban rates) as a Could item (DEC-035).
- [ASSUMPTION] Telemetry is off by default (NFR-PRIV-001).

## Acceptance Criteria

- [ ] Every requirement has a unique ID, priority, phase, source and test.
- [ ] Every Must requirement maps to a design section and a task in [17](17-roadmap.md).
- [ ] No requirement contradicts [03](03-fair-play-and-compliance.md).
