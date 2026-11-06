# Stalemate

A free, open-source companion app for Valve's *Deadlock*. It covers the whole match: scouting before it, item recommendations during it, and a review after it.

> **Status: pre-alpha.** The building blocks exist and are tested, but the app isn't wired end to end yet and there's no release. See [Current state](#current-state) for what works today and [Roadmap](#roadmap) for what's next.

- **Targets:** Flatpak on KDE Plasma 6 (Wayland) first, then Windows 10/11.
- **Stack:** Rust, [Slint](https://slint.dev) UI, SQLite storage.
- **Licence:** GPL-3.0.
- **Cost to run:** nothing. There's no developer server. Every install talks to public APIs from the player's own machine.
- **Flatpak app ID:** `io.github.hatchbear.stalemate`

Stalemate isn't affiliated with or endorsed by Valve.

---

## What it's for

| Phase | What Stalemate does |
|---|---|
| **Before and early in a match** | Shows a *Scouting Card* for every other player: win/loss record, rank trend, most-played heroes, hero experience. Later: build profile and playstyle |
| **During a match** | Tracks what the enemy team has bought, matches their items to known builds, projects where those builds are heading, and recommends counter items. Compares your build to what you planned |
| **After a match** | A chess.com-style review: it grades your decisions, finds key moments, gives 1–3 prioritized fixes, and can narrate them with a local model or your own API key |

Everything runs on public match history and on information a player can already see in the game's own UI.

## Fair play

Stalemate follows Valve's terms strictly. This is a hard rule for the project, not a preference. The full reasoning is in [`03-fair-play-and-compliance.md`](03-fair-play-and-compliance.md).

**It never:**

- reads or writes game memory, injects code, hooks graphics, captures packets or sends input to the game;
- automates anything in game, including buying items;
- calls the Steam Web API to look up other players;
- recovers data for players who opted out of deadlock-api;
- implements a Valve game-coordinator client;
- tracks fights live, or displays chat.

**During a live match**, only fields marked *Live-Safe* in [`config/live_safe.toml`](config/live_safe.toml) can reach the UI, and only through the `live_safe` module. Every field on that list was checked by hand in the real game. Anything not verified stays blocked.

**Where its data comes from:**

1. The game's own `console.log` (via the `-condebug` launch option) to detect that a match started.
2. [deadlock-api.com](https://deadlock-api.com), for match history, player stats and a locator for the match's spectator broadcast.
3. The spectator broadcast, which is public and delayed by about 25 seconds. A pinned, patched fork of the [haste](https://github.com/blukai/haste) parser (in [`third_party/haste`](third_party/haste)) reads it.

Compliance is checked in CI by [`scripts/fair-play-check.sh`](scripts/fair-play-check.sh) and the [`tests/compliance`](tests/compliance) crate.

---

## Current state

Most of the core library is written and unit-tested. The app shell exists. The runtime that connects them is the main piece still missing.

### Built

| Area | Where | Notes |
|---|---|---|
| Workspace, CI skeleton, task runner | `Cargo.toml`, `justfile`, `.github/` | Toolchain pinned to Rust 1.98.1 |
| Storage and migrations | `crates/core/src/storage/` | SQLite, migration 001 |
| Settings and secrets | `crates/core/src/settings.rs`, `config/` | All thresholds and constants live in config, not code |
| Flatpak packaging | `packaging/flatpak/` | Builds and launches; about 30 MB |
| Account resolver | `crates/core/src/account.rs` | Finds your account from the Steam install; includes `-condebug` checks |
| deadlock-api client | `crates/core/src/api_client/` | Rate-limit aware, plus an asset cache |
| Broadcast client | `crates/core/src/broadcast/` | Reads the delayed spectator stream, optional raw recording. Smoke-tested against a live match: 12 players resolved, all items resolved |
| Game state and Live-Safe filter | `game_state.rs`, `live_safe.rs` | Logic done and tested |
| Purchase Log | `purchase_log.rs` | Logic done; persistence comes with the runtime |
| Scouting basics | `crates/core/src/scouting/` | W/L, MMR trend, mains, hero experience |
| Build library and importer | `crates/core/src/builds/` | In-game community builds and a manual builder |
| Stats-only Review | `crates/core/src/review/` | Match record, lane grade, soul sources |
| Main window shell | `ui/`, `crates/app/` | Slint views for Home, Scouting, Live, Reviews, Builds, Settings, running on demo data |
| Hero and item data | runtime | Loaded from the API at runtime and never hard-coded. Art is never bundled |

### Early logic that is ahead of the roadmap

Some Beta modules already exist as code, but nothing consumes them yet: `build_matcher.rs`, `projector.rs`, `counter.rs`, `build_check.rs`, `pace.rs`. Treat them as drafts until the Beta tasks that cover them are done.

### Not done

- **The runtime wiring.** The state machine that goes Idle → Match detected → Live → Awaiting end → Reviewing → Idle, and feeds real data into the window, hasn't been written. The UI still runs on demo data.
- **Match detection.** The code is written, but the `console.log` line patterns in `config/rules.toml` are empty until they are captured from real matches. Until then the watcher reports "not configured". A manual "watch this match ID" entry is planned as a stopgap.
- **Fixture recordings and the fixture recorder.** Needed for the review tests and for Review work in general.
- **The overlay.** `crates/overlay` is a stub.
- **Windows.** Nothing has been built or tested on Windows yet.
- **Final art.** Placeholders only.

### Known limits and open items

- The match-end signal (winning team, team numbering) hasn't been seen in a live match yet.
- The pre-game swap phase only exists in ranked, so it can't be tested by the author. It will be left to Beta testers.
- The per-match broadcast locator endpoint is limited to 6 requests per hour per IP, and 300 per hour across all users. That is a scaling risk to revisit before a wide release.
- deadlock-api has no MMR history endpoint. The MMR trend is rebuilt from per-match rank deltas, so it needs at least two ranked matches.
- One dependency (`mars`) has no licence file yet. An upstream issue is open. A scoped exception is in `deny.toml`, and the release check blocks public builds until it's resolved.

---

## Roadmap

The full plan is in [`17-roadmap.md`](17-roadmap.md). Effort is in person-days for one developer and is an estimate. No dates are set.

| Milestone | Goal | Platforms | Effort |
|---|---|---|---|
| **Alpha** | Prove the data pipeline end to end | Flatpak, KDE Plasma 6 Wayland | 61 |
| **Beta** | All the core intelligence and the overlay | Plus Windows 11 | 81 |
| **v1** | Everything else, final art, privacy policy, Flathub stable | Plus Windows 10, best effort elsewhere | 38 |

About 180 person-days in total, plus a 30% contingency.

### Alpha: prove the pipeline

Match detected, broadcast consumed and recorded, basic scouting, enemy board and Purchase Log, a stats-only Review, build import and a manual builder. Main window only.

- [x] Workspace, CI, storage, settings, Flatpak
- [x] Account resolver, API client, Broadcast client
- [x] Game state, Live-Safe filter, Purchase Log
- [x] Scouting basics, builds, stats-only Review
- [x] Main window shell
- [ ] Runtime wiring and first-run `-condebug` setup
- [ ] Match Watcher log patterns (needs real captures)
- [ ] Fixture recorder and recorded matches
- [ ] Alpha QA and release

**Exit when:** all Alpha Must requirements pass on KDE Plasma (native and Flatpak Steam), the fair-play check passes on 20 fixtures, and the open data questions are resolved.

### Beta: the intelligence

- **Scouting:** build profiles with a counter-buy filter, power spikes, playstyle, your own matchups, manual roster entry when the broadcast is unavailable
- **Live:** Build Matcher, Projector, phase-aware counter-item recommender with history, Build Check against your selected build
- **Overlay** on KDE Wayland (layer-shell), then the **Windows port** (Win32 overlay, MSI)
- **Review:** win-probability model and trainer, decision evaluator with tiers and accuracy, key moments, rule-based findings, LLM narrator (local model or your own key), report UI
- **Also:** .NET fallback parser, asset pipeline, patch-change flagging for builds, replay-download fallback, update check against GitHub Releases, team composition prediction

**Exit when:** all Beta Must requirements pass, the success metrics are met, the overlay passes its tests, and there are two weeks of field testing with no open severity-1 bugs.

### v1: the rest

- Flags, notes, repeat-player detection, session check, Players view
- Pace and item timing, flex slots and timers
- Aim and melee review rules, benchmarks, long-term trends
- External build importers, match-level ban rates
- Data export and delete
- Privacy policy, About and disclaimer
- Final art, UI sounds
- Windows 10 and other-desktop testing, Flathub stable submission

**Exit when:** every Must and Should requirement passes, no placeholders remain, the privacy policy is live and Flathub stable is published.

### Not planned

macOS, a mobile app, Overwolf, a paid tier, per-player ban habits, live fight tracking, and anything that automates the game. The reasons are in [`01-overview.md`](01-overview.md).

---

## Building

You need the pinned Rust toolchain (set by `rust-toolchain.toml`; install with [rustup](https://rustup.rs)) and [`just`](https://github.com/casey/just).

```sh
just build        # cargo build --workspace
just test         # unit tests, fixture tests, and .NET tests if a project exists
just lint         # cargo fmt --check and cargo clippy -D warnings
just compliance   # fair-play, hard-coded-ID and asset checks
just flatpak      # build and install the Flatpak (needs flatpak-builder, KDE 6.11 SDK)
```

`just win-installer`, `just export-assets` and `just fixtures-verify` are placeholders for tasks that haven't been built yet.

### Using the app today

To use Stalemate you must launch Deadlock with the `-condebug` option so the game writes `console.log`. The first-run setup flow for this is still being wired up. Until then the app runs in demo mode.

## Repository layout

| Path | What's there |
|---|---|
| `crates/core` | The library (`stalemate-core`): data, game state, scouting, builds, review |
| `crates/app` | The `stalemate` binary and Slint view-model glue |
| `crates/overlay` | Overlay binary (stub) |
| `crates/model-trainer` | Win-probability model trainer (stub) |
| `crates/fixture-recorder` | Records matches into test fixtures (stub) |
| `ui/` | Slint UI files |
| `config/` | Thresholds, Live-Safe field list, default settings |
| `i18n/` | UI strings |
| `assets/` | Fonts and placeholder art |
| `packaging/` | Flatpak manifest, desktop file, metainfo |
| `scripts/` | Compliance and release checks |
| `tests/` | Compliance and fixture tests |
| `third_party/haste` | Vendored, patched broadcast parser |
| `dotnet/` | Reserved for the .NET fallback parser |
| `00`–`19-*.md` | The design documents. Start at [`00-index.md`](00-index.md) |

## Design documents

The numbered Markdown files at the repository root are the specification. Good starting points:

- [`00-index.md`](00-index.md): the map of all documents
- [`01-overview.md`](01-overview.md): vision, goals and non-goals
- [`02-requirements.md`](02-requirements.md): every requirement and its ID
- [`03-fair-play-and-compliance.md`](03-fair-play-and-compliance.md): what the app may and may not do
- [`17-roadmap.md`](17-roadmap.md): tasks, dependencies and exit criteria
- [`19-decision-log.md`](19-decision-log.md): every decision and why

## Contributing

The project is at an early stage and the design is still moving, so please open an issue before sending a pull request. Anything that touches fair-play rules needs a decision-log entry first. New dependencies must be GPL-3.0 compatible.

## Licence

GPL-3.0. See [`LICENSE`](LICENSE). *Deadlock* and its art belong to Valve. No Valve art or game files are in this repository, and hero and item images are loaded at runtime.
