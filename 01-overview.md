# 01 — Overview and Goals

## Purpose

Defines what the app is, who it's for, what it will and won't do, and how success is measured.

## Scope

Product-level vision, audience, goals, scope boundaries, release model and success metrics. Detailed requirements are in [02](02-requirements.md).

## Details

### Vision

A free desktop companion for Deadlock that covers the whole match:

- **Scouting:** Who you're playing with and against, before you commit to a plan.
- **In-match recommendations:** What to buy against what the enemy is actually building.
- **Review:** A chess.com-style breakdown of what went well and wrong, with concrete fixes.

It uses only public data and information a player can already see. It stays within Valve's terms ([03](03-fair-play-and-compliance.md)).

### Audience

| Segment | Platform | Phase |
|---|---|---|
| Deadlock players on Linux, KDE Plasma (Wayland) | Flatpak | Alpha onward |
| Deadlock players on Windows 10/11 x64 | Native installer | Beta onward [ASSUMPTION] |
| Deadlock players on other Linux desktops | Flatpak, window works; overlay best effort | v1, best effort [ASSUMPTION] |

The release is public. It's aimed at players in Standard mode only; other game modes are recorded but not analyzed (DEC-019).

### Goals

| ID | Goal |
|---|---|
| G-1 | Show a Scouting Card for every other player in the match, built from public match history |
| G-2 | Recommend counter items during the match, against your lane opponents in the lane phase and against the whole enemy team after it |
| G-3 | Track enemy purchases during the match and use them to detect and project enemy builds |
| G-4 | Produce a post-match Review that classifies your decisions and gives 1–3 prioritized fixes |
| G-5 | Run as a Flatpak on KDE Plasma first, then on Windows, with a main window plus an overlay |
| G-6 | Cost the developer nothing to run |

### In scope

- The three phases above ([05](05-player-scouting.md), [06](06-in-match-recommendations.md), [07](07-post-match-review-engine.md)).
- Build import from websites and a manual builder ([08](08-builds-import-and-builder.md)).
- A main window and an overlay ([09](09-ui-window-and-overlay.md)).

### Non-goals

| Non-goal | Reason |
|---|---|
| Reading game memory, injecting code, hooking graphics, capturing packets | Steam Subscriber Agreement cheating clauses ([03](03-fair-play-and-compliance.md)) |
| Overwolf | Windows-only, and its live data method is undisclosed (DEC-002) |
| Automating any in-game action, including buying items | Steam Subscriber Agreement automation clause |
| Tracking fights live | The broadcast delay makes it useless, and it's the riskiest data to show (DEC-005) |
| Per-player ban habits | Ban data has no player attached (DEC-017) |
| Looking up opponents through the Steam Web API | Steam Web API terms ([03](03-fair-play-and-compliance.md)) |
| Showing non-Live-Safe data during a match | Fair play ([03](03-fair-play-and-compliance.md)) |
| macOS | Not discussed [ASSUMPTION] |
| Mobile app | Not discussed [ASSUMPTION] |

### Release model

- **Price:** Free. The paid Upkeep tier was dropped from v1 scope (DEC-009, DEC-018).
- **Licence:** GPL-3.0 (DEC-020).
- **Zero-cost design:**
    - Each install calls deadlock-api directly from the player's own IP, so there's no developer server.
    - LLM text runs on a local model or the user's own API key ([07](07-post-match-review-engine.md)).
- **Channels:** Alpha, beta and stable ([15](15-tools-build-and-deploy.md)).

### Success metrics

All targets are [ASSUMPTION] values proposed for author approval.

| ID | Metric | Target | How measured |
|---|---|---|---|
| SM-1 | Time from match-start log line to Match Watcher event | ≤ 5 s | TST-010 |
| SM-2 | Time from roster available in Broadcast to all Scouting Cards filled | ≤ 30 s for the basic fields (FR-SCOUT-001 to 004), for players with public data. Build Profile and Playstyle fill in afterward (DEC-022) | TST-020 |
| SM-3 | Build Matcher top-1 accuracy at game time 15:00 | ≥ 60% on the validation set | TST-042 |
| SM-4 | Overlay update latency after new Broadcast data | ≤ 1 s | TST-055 |
| SM-5 | Rules-based Review ready after the match's Broadcast ends | ≤ 60 s | TST-060 |
| SM-6 | Win-probability model calibration (Brier score) on held-out matches | ≤ 0.20 | TST-061 |
| SM-7 | App CPU use during a match, averaged over 60 s | ≤ 3% of one core | TST-070 |
| SM-8 | App memory use during a match | ≤ 400 MB | TST-071 |
| SM-9 | Live-Safe violations in overlay or window payloads during Live Mode | 0 | TST-030 |
| SM-10 | Developer running cost per month at launch | $0 | Billing review |

## Dependencies

- [02 Requirements](02-requirements.md)
- [03 Fair play and compliance](03-fair-play-and-compliance.md)
- [17 Roadmap](17-roadmap.md)
- [19 Decision log](19-decision-log.md)

## Open Questions

- OQ-15 (resolved): Standard mode only. Other modes are recorded but not analyzed (DEC-019).
- [ASSUMPTION] Windows support starts in Beta.
- [ASSUMPTION] macOS and mobile are out of scope.
- [ASSUMPTION] All success-metric targets above.

## Acceptance Criteria

- [x] The author has approved or edited every SM target (approved 2026-11-05, DEC-038).
- [ ] Every goal maps to at least one requirement in [02](02-requirements.md).
- [ ] Every non-goal is reflected in a requirement or rule (NFR-FAIR, FR-LIVE-015, FR-SCOUT-014).
