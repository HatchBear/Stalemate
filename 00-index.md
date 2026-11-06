# 00 — Index

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-01 — Product name (used in window titles, the Flatpak app ID, the installer and store listings)
> Constraints from the project: The Flatpak app ID must be reverse-DNS (for example `io.github.<account>.<name>`), lowercase-safe, and stable after the first public release. The name must not imply Valve endorsement ([03](03-fair-play-and-compliance.md)).
> Options to consider (optional): —
> Decision: **Stalemate** (chosen and confirmed 2026-11-05, DEC-036). The GitHub repository is https://github.com/HatchBear/Stalemate, so the Flatpak app ID is `io.github.hatchbear.stalemate` (lowercase).

The product name is **Stalemate**, and the binary and path name `<app>` is now `stalemate`. These docs still say **the app** in prose. The Flatpak app ID is `io.github.hatchbear.stalemate`.

## Purpose

This is the entry point to the design doc set for a free, public companion app for Valve's Deadlock. It lists every doc, defines shared terms, tracks doc status, and registers every open creative decision.

## Scope

- **Covers:** All 20 documents in this folder.
- **Does not cover:** Visual design, which stays with the author (see the creative-decision register below).

## Details

### Project summary

The app helps a Deadlock player in three phases:

1. **Before the match (scouting):** Records, mains, core builds with counter buys filtered out, power spikes, playstyle and flags for every other player in the match.
2. **During the match (in-match recommendations):** An enemy purchase log, detected and projected enemy builds, phase-aware counter items, a team comp win prediction, and checks on your own build. An overlay shows what's needed right now; the main window shows everything good to know, including past item recommendations.
3. **After the match (review):** A chess.com-style review that scores your decisions from win-probability swings, with rules where possible and an LLM where rules can't explain.

It ships first as a Flatpak on KDE Plasma, then on Windows. It's built in Rust, with an optional .NET fallback parser. It uses only public data or data a player can already see ([03](03-fair-play-and-compliance.md)). It's free ([01](01-overview.md)).

### Table of contents

| # | Document | What it covers |
|---|---|---|
| 00 | [Index](00-index.md) | This page |
| 01 | [Overview and goals](01-overview.md) | Vision, audience, scope, non-goals, success metrics |
| 02 | [Requirements](02-requirements.md) | Functional and non-functional requirements with IDs, priority and phase |
| 03 | [Fair play and compliance](03-fair-play-and-compliance.md) | ToS rules, the Live-Safe whitelist, privacy, licensing |
| 04 | [Data sources and ingestion](04-data-sources-and-ingestion.md) | Console log, broadcast, deadlock-api, replays, rate limits, caching |
| 05 | [Player scouting](05-player-scouting.md) | Scouting Cards, the counter-buy filter, playstyle, flags |
| 06 | [In-match recommendations](06-in-match-recommendations.md) | Purchase Log, Build Matcher, Projector, counter items, Build Check, pace |
| 07 | [Post-match review engine](07-post-match-review-engine.md) | Win-probability model, tiers, accuracy, Key Moments, rules, LLM |
| 08 | [Builds: import and builder](08-builds-import-and-builder.md) | Build model, imports, manual builder, active build plan |
| 09 | [UI: window and overlay](09-ui-window-and-overlay.md) | Views, overlay panels, behavior, platform mechanics |
| 10 | [Technical architecture](10-technical-architecture.md) | Stack, modules, diagrams, IPC, repo layout |
| 11 | [Data model](11-data-model.md) | SQLite schema, JSON schemas, retention |
| 12 | [Programs and code](12-programs-and-code.md) | Every program: I/O, starter code, tests |
| 13 | [Art and asset pipeline](13-art-and-asset-pipeline.md) | Asset list, specs, naming, folders, placeholders |
| 14 | [Data and content assets](14-data-and-content-assets.md) | Config, model, prompt, string and audio assets |
| 15 | [Tools, build and deploy](15-tools-build-and-deploy.md) | Dev setup, Flatpak manifest, Windows installer, CI, releases |
| 16 | [Testing and QA](16-testing-and-qa.md) | Test levels, test cases, fixtures, platform matrix |
| 17 | [Roadmap](17-roadmap.md) | Alpha, beta and v1 milestones, tasks, effort |
| 18 | [Risks and open questions](18-risks-and-open-questions.md) | Risk register, consolidated open questions |
| 19 | [Decision log](19-decision-log.md) | Every decision from the design conversation |

### Glossary

These are internal working terms for the docs and code. They aren't user-facing labels; all UI wording is CD-13.

| Term | Definition |
|---|---|
| **Account ID** | A player's 32-bit Steam account ID (SteamID3 number), the key deadlock-api uses for players |
| **Adjusted win rate** | deadlock-api's item win rate re-weighted by net worth at time of purchase, so items bought by players who were already ahead aren't overrated |
| **Broadcast** | Valve's public spectator stream of a live match, running about 25 s behind real time (measured in T-008, DEC-050) |
| **Broadcast delay** | The gap of about 25 s between game time and Broadcast time (DEC-050). All live features are designed for it |
| **Build Check** | The in-match feature that suggests swapping items in your active build plan for counter items |
| **Build Matcher** | The component that guesses which build an enemy is following from their items |
| **Build plan** | Your intended build for the current match: imported, built manually, or read from your selected in-game build |
| **Build Profile** | A player's usual build on a hero: core items, situational items and counter buys |
| **Core item** | An item a player buys in most matches on a hero, regardless of the enemy team |
| **Counter buy** | An item a player buys mainly when specific enemy heroes are present |
| **Counter-buy filter** | The algorithm that separates core items from counter buys in a Build Profile ([05](05-player-scouting.md)) |
| **deadlock-api** | The community REST API at `api.deadlock-api.com`, not affiliated with Valve |
| **Expected points lost (EPL)** | The drop in your team's win probability attributed to one of your decisions (0.00–1.00) |
| **Key Moment** | One of the five largest win-probability swings involving the user in a match |
| **Lane phase** | Game time 0:00–9:00 |
| **Live Mode** | App state while a match is in progress. Only Live-Safe fields may be shown |
| **Live-Safe field** | A data field the player could already see in-game, which may be shown during a match ([03](03-fair-play-and-compliance.md)) |
| **Match Watcher** | The component that detects match start and end from the game's console log |
| **Overlay** | The click-through, always-on-top surface for currently needed info |
| **Main window** | The regular app window for good-to-know info, history and reviews |
| **Projection** | A prediction of an enemy's next three items |
| **Purchase Log** | The time-stamped record of every item bought or sold by each player in the current match |
| **Review** | The post-match report produced by the review engine ([07](07-post-match-review-engine.md)) |
| **Review Mode** | App state after a match. All data, including non-Live-Safe fields, may be used |
| **Scouting Card** | The per-player panel of scouting data ([05](05-player-scouting.md)) |
| **Session** | A run of matches with gaps of under 30 minutes between them |
| **Swing** | The change in team win probability across an event window |
| **Tier** | One of the nine review classifications for a decision ([07](07-post-match-review-engine.md)) |
| **Upkeep tier** | A paid tier that was considered and dropped from v1 scope (DEC-018). Any paid tier would be revisited after v1, with legal review |

### ID conventions

| Prefix | Meaning | Defined in |
|---|---|---|
| `G-` | Goal | [01](01-overview.md) |
| `SM-` | Success metric | [01](01-overview.md) |
| `FR-` | Functional requirement | [02](02-requirements.md) |
| `NFR-` | Non-functional requirement | [02](02-requirements.md) |
| `LS-` | Live-Safe field | [03](03-fair-play-and-compliance.md) |
| `API-` | deadlock-api endpoint | [04](04-data-sources-and-ingestion.md) |
| `R-` | Review rule | [07](07-post-match-review-engine.md) |
| `M-` | Software module | [10](10-technical-architecture.md) |
| `PRG-` | Program | [12](12-programs-and-code.md) |
| `TC-` | Program test case | [12](12-programs-and-code.md) |
| `AST-` | Art asset | [13](13-art-and-asset-pipeline.md) |
| `DATA-`, `AUD-` | Data or audio asset | [14](14-data-and-content-assets.md) |
| `TST-` | QA test | [16](16-testing-and-qa.md) |
| `T-` | Roadmap task | [17](17-roadmap.md) |
| `RSK-` | Risk | [18](18-risks-and-open-questions.md) |
| `OQ-` | Open question | [18](18-risks-and-open-questions.md) |
| `DEC-` | Decision | [19](19-decision-log.md) |
| `CD-` | Creative decision | This page |

### Creative-decision register

| ID | Topic | Where |
|---|---|---|
| CD-01 | Product name | [00](00-index.md) |
| CD-02 | Overall visual style of window and overlay | [09](09-ui-window-and-overlay.md) |
| CD-03 | Color palette | [09](09-ui-window-and-overlay.md) |
| CD-04 | Typography | [09](09-ui-window-and-overlay.md) |
| CD-05 | Overlay panel look and layout aesthetics | [09](09-ui-window-and-overlay.md) |
| CD-06 | Motion and transitions | [09](09-ui-window-and-overlay.md) |
| CD-07 | Review tier names and tier icon design | [07](07-post-match-review-engine.md) |
| CD-08 | Coach voice and tone of review text | [07](07-post-match-review-engine.md) |
| CD-09 | Optional playstyle tag names | [05](05-player-scouting.md) |
| CD-10 | App icon and icon style | [13](13-art-and-asset-pipeline.md) |
| CD-11 | Mascot appearance (concept is fixed: "monke with a Tesla cannon... from the moon") | [13](13-art-and-asset-pipeline.md) |
| CD-12 | Whether to use sound, and what it sounds like | [14](14-data-and-content-assets.md) |
| CD-13 | UI copy wording (labels, empty states, errors) | [14](14-data-and-content-assets.md) |

### Doc status

| Doc | Version | Status | Last updated |
|---|---|---|---|
| 00–19 (all) | 0.2 | Draft, author review round 1 applied | 2026-11-05 |

## Dependencies

All documents [01](01-overview.md)–[19](19-decision-log.md).

## Open Questions

- CD-01 to CD-13 (register above).
- All technical open questions are consolidated in [18](18-risks-and-open-questions.md).

## Acceptance Criteria

- [ ] Every link in the contents table opens an existing file.
- [ ] Every doc has the six sections: Purpose, Scope, Details, Dependencies, Open Questions, Acceptance Criteria.
- [ ] Every glossary term is used with the same meaning in all docs.
- [ ] Every creative-decision block in the set has an entry in the register.
