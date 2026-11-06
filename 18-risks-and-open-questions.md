# 18 — Risks and Open Questions

## Purpose

The risk register, plus every open question and assumption from the doc set in one place.

## Scope

- **Covers:** Technical, platform, policy, cost and data risks; all OQ IDs; assumptions awaiting author approval.

## Details

### Risk register

Likelihood (L) and Impact (I): H = high, M = medium, L = low.

| ID | Risk | L | I | Mitigation | Linked |
|---|---|---|---|---|---|
| RSK-01 | Valve restricts or rate-limits the Broadcast, or changes its format | M | H | Degrade to API-only scouting (FR-DATA-007); keep the parser and locator isolated in M-04 | [04](04-data-sources-and-ingestion.md) |
| RSK-02 | deadlock-api changes endpoints, limits or terms, or goes offline | M | H | One client module (M-03); cache aggressively; verify paths against `/openapi.json` in CI weekly [ASSUMPTION] | [04](04-data-sources-and-ingestion.md) |
| RSK-03 | A game patch breaks haste | H | M | .NET fallback parser (M-21); patch-day procedure | [16](16-testing-and-qa.md) |
| RSK-04 | Valve treats scouting or overlays as unfair (as it did in Dota 2) | M | H | Strict Live-Safe rules; scouting only after the swap window; no automation; follow Valve statements and adapt | [03](03-fair-play-and-compliance.md) |
| RSK-05 | `layer-shika` is early-stage, or KWin doesn't keep a layer-shell overlay above the game | M | H | Fall back to `smithay-client-toolkit` directly; test in T-027; fall back to window-only on Linux if needed | [09](09-ui-window-and-overlay.md) |
| RSK-06 | Steam libraries outside default paths aren't readable in the Flatpak sandbox | M | M | FileChooser portal to grant a library folder; clear setup check | [04](04-data-sources-and-ingestion.md) |
| RSK-07 | LLM text states wrong facts | M | M | Numbers validated against facts; template fallback; LLM never computes values | [07](07-post-match-review-engine.md) |
| RSK-08 | Hosted LLM or live-events costs break the free model | M | M | Default Off/Local/Own key; no hosted LLM or live-events service in v1 (DEC-018) | [01](01-overview.md) |
| RSK-09 | A paid Upkeep tier conflicts with Valve's non-commercial content license | M | H | No paid tier in v1 (DEC-018); legal review (OQ-10) before ever charging; never sell Valve content | [03](03-fair-play-and-compliance.md) |
| RSK-10 | Unsigned Windows builds trigger SmartScreen warnings and hurt adoption | H | M | Free open-source signing programme; unsigned with a README note until approved (DEC-033) | [15](15-tools-build-and-deploy.md) |
| RSK-11 | Locating the Broadcast requires the app's own Valve game-coordinator client, which SSA §2.G may forbid | M | H | Spike T-008 is a hard go/no-go gate (DEC-021); prefer a third-party locator; don't implement a GC client without consent; the fallback is API scouting plus a post-match Review | [03](03-fair-play-and-compliance.md) |
| RSK-12 | The win-probability model is poorly calibrated, so tiers feel wrong | M | M | Brier gate (SM-6); retrain when the Brier score degrades; label models trained on earlier patches | [07](07-post-match-review-engine.md) |
| RSK-13 | Small samples make Build Profiles and playstyle noisy | H | L | Sample guards and "Low sample" labels; global fallbacks | [05](05-player-scouting.md) |
| RSK-14 | Players object to being profiled | L | M | Only public data; respect opt-outs; notes stay local | [03](03-fair-play-and-compliance.md) |
| RSK-15 | Flathub rejects the Steam filesystem permissions | L | M | Read-only, narrowly scoped paths with justification; portal for the rest | [15](15-tools-build-and-deploy.md) |
| RSK-16 | The app's CPU use hurts game performance | L | M | NFR-PERF limits; overlay redraws ≤ 1/s; parsing on a background thread | [02](02-requirements.md) |
| RSK-17 | Required log lines aren't present in `console.log` | L | H | Alternative: detect a match from the Broadcast and Steam logs [ASSUMPTION]; resolve in T-006 | [04](04-data-sources-and-ingestion.md) |

**Risk acceptance (DEC-044):** The author accepts RSK-01, RSK-02, RSK-04, RSK-05 and RSK-11 as the baseline risks of the project, with the mitigations listed.

### Open questions

| ID | Question | Blocks | Raised in |
|---|---|---|---|
| OQ-01 | Are there target dates for Alpha, Beta and v1? **Resolved (DEC-034): none, effort only.** | Scheduling | [17](17-roadmap.md) |
| OQ-02 | How can the app get the Broadcast URL for a match without its own Valve protocol client? **Hard go/no-go gate (DEC-021).** | FR-DATA-003 (Alpha) | [04](04-data-sources-and-ingestion.md) |
| OQ-03 | What's the exact `console.log` path, and which lines mark match start, end and match ID? | FR-DATA-001 (Alpha) | [04](04-data-sources-and-ingestion.md) |
| OQ-04 | Which scoreboard fields are visible in-game (LS-05 to LS-09, LS-11, LS-13)? | Live-Safe whitelist | [03](03-fair-play-and-compliance.md) |
| OQ-05 | Which fields does API-05 return (purchases, selected build, pre-swap hero, lane, party, per-interval stats), and what's the batch limit? Does the Broadcast expose the user's selected build? | Scouting, Review, FR-BUILD-004 | [04](04-data-sources-and-ingestion.md) |
| OQ-06 | Which external build sites should be supported? **FR-BUILD-002 is now Should; JSON and share-code import ship first (DEC-031).** | FR-BUILD-002 | [08](08-builds-import-and-builder.md) |
| OQ-07 | What's the community builds endpoint path (API-16)? | FR-BUILD-001 | [04](04-data-sources-and-ingestion.md) |
| OQ-08 | Keep match-level ban rates (FR-SCOUT-016), or drop ban data entirely? **Resolved: keep as a Could item (DEC-035).** | FR-SCOUT-016 | [05](05-player-scouting.md) |
| OQ-09 | What are the current timer schedules and flex slot rules? **Plan: hand-fill from the current patch before v1 (DEC-032).** | FR-LIVE-012, FR-LIVE-013 | [06](06-in-match-recommendations.md) |
| OQ-10 | Is a paid Upkeep tier compatible with Valve's terms? **Deferred past v1 (DEC-018).** | DEC-009 | [03](03-fair-play-and-compliance.md) |
| OQ-11 | Should the LLM provider default to Off or Local? **Resolved: Off (DEC-030).** | FR-SET-001 | [07](07-post-match-review-engine.md) |
| OQ-12 | Do Slint's, layer-shika's and DemoFile's licenses fit the release model? **Slint is used under GPLv3 (DEC-020); layer-shika and DemoFile still need a GPL-3.0 compatibility check.** | Stack | [10](10-technical-architecture.md) |
| OQ-13 | What are the exact haste broadcast API and DemoFile's Deadlock API? | PRG-04, PRG-21 | [12](12-programs-and-code.md) |
| OQ-14 | How will Windows builds be signed at zero or low cost? **Decided: free open-source signing programme (DEC-033).** | Windows release | [15](15-tools-build-and-deploy.md) |
| OQ-15 | Should Street Brawl be supported (scouting, recommendations, Review)? **Resolved: no, Standard only (DEC-019).** | Scope | [01](01-overview.md) |
| OQ-16 | What are Flathub's current icon and screenshot requirements? | AST-STORE-001 | [13](13-art-and-asset-pipeline.md) |
| OQ-17 | Which `org.kde.Platform` branch and rust-stable extension branch should be used? | Flatpak build | [15](15-tools-build-and-deploy.md) |
| OQ-18 | What's the path of the replay URL endpoint (API-17), and does it need Valve-side data? | FR-DATA-009 | [04](04-data-sources-and-ingestion.md) |
| OQ-20 | What endpoint and data shape does the team comp prediction (API-18) use? | FR-LIVE-016 | [04](04-data-sources-and-ingestion.md) |
| OQ-19 | How long is Deadlock's pre-game hero-swap phase, and does it close before the game clock starts? **Deferred to a ranked-access Beta tester (DEC-047). FP-10 is safe without it.** | FP-10 | [03](03-fair-play-and-compliance.md) |

### Creative decisions pending

CD-01 is decided (Stalemate) and CD-09 is decided (no tags). CD-07 and CD-08 are decided, and CD-13 is drafted during implementation. CD-02 to CD-06 and CD-12 are now decided. Only CD-10 and CD-11 (app icon and mascot art) are open, and placeholders cover them until v1. All are listed in [00](00-index.md).

### Assumptions (approved as starting values on 2026-11-05, DEC-038)

| Assumption | Where |
|---|---|
| Windows support starts in Beta; macOS and mobile are out of scope | [01](01-overview.md) |
| All success-metric targets SM-1 to SM-10 | [01](01-overview.md) |
| Must = user-specified features; Should = Feature Breakdown additions | [02](02-requirements.md) |
| Telemetry off by default; data stored locally only | [02](02-requirements.md) |
| Token buckets at 80% of published limits; no bundled API key | [04](04-data-sources-and-ingestion.md) |
| Recording the Broadcast is the primary Review source; the replay file is the fallback | [04](04-data-sources-and-ingestion.md) |
| All scouting, matcher, projector, counter, Build Check, pace and review thresholds | [14](14-data-and-content-assets.md) DATA-001 |
| Win-probability features and objective weights | [07](07-post-match-review-engine.md) |
| Standard mode only for Reviews in v1 | [07](07-post-match-review-engine.md) |
| Separate overlay process; Slint for UI; `layer-shika` on Linux | [10](10-technical-architecture.md) |
| Retention: 50 matches or 30 days of recordings | [11](11-data-model.md) |
| No default overlay hotkey; adjustable overlay opacity; accessibility rules | [09](09-ui-window-and-overlay.md) |
| GitHub hosting and CI; version ranges per channel | [15](15-tools-build-and-deploy.md) |
| One-developer effort estimates | [17](17-roadmap.md) |

## Dependencies

All documents.

## Open Questions

This document is the register. Resolve OQ-02 to OQ-05 before the end of Alpha (exit criteria in [17](17-roadmap.md)).

## Acceptance Criteria

- [ ] Every OQ and RSK referenced anywhere in the set appears here.
- [ ] Every High-impact risk has a mitigation task in [17](17-roadmap.md) or a decision.
- [x] The author has approved or replaced every assumption above (approved 2026-11-05).
