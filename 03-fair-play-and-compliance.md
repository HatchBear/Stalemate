# 03 — Fair Play and Compliance

## Purpose

Defines the rules that keep the app within Valve's terms and fair to other players. Every other doc must follow these rules.

## Scope

- **Covers:**
    - Steam Subscriber Agreement (SSA) constraints.
    - Steam Web API terms.
    - deadlock-api data use.
    - The Live-Safe whitelist.
    - Privacy obligations.
    - Licensing of Valve content.
- **Note:** This is not legal advice. The app stays free in v1 (DEC-018), so a legal review is needed only if a paid tier is revisited (OQ-10).

## Details

### Source constraints

| Source | Constraint | Effect on the app |
|---|---|---|
| [SSA §4.B Cheating](https://store.steampowered.com/agreement) | No creating or using software that gives an unfair competitive advantage in multiplayer. No tampering with the game's execution | NFR-FAIR-001, the Live-Safe whitelist |
| [SSA §4.C Automation](https://store.steampowered.com/agreement) | No scripts, bots or macros interacting with game content | NFR-FAIR-002. The app never clicks, types or buys |
| [SSA §2.G](https://store.steampowered.com/agreement) | No reverse engineering. No emulating or redirecting Valve's network protocols for services without consent | No custom Valve game-coordinator client in the app (RSK-11) |
| [SSA §2.A](https://store.steampowered.com/agreement) | License to Valve content is personal and non-commercial | Hero and item art isn't bundled. The app stays free in v1 (DEC-018); any paid tier needs legal review first (OQ-10) |
| [Steam Web API terms](https://steamcommunity.com/dev/apiterms) | No features that give an unfair competitive advantage. Retrieve a user's data only as requested by that user. 100,000 calls per day. A privacy policy is required | NFR-FAIR-005. The Steam Web API is not used for other players |
| [deadlock-api data privacy](https://deadlock-api.com/data-privacy) | Players can delete their data and block future collection | NFR-FAIR-004. Empty results are shown as "No public data" |
| Dota 2 precedent | Valve patched Dota 2 to block player profiles during the pregame phase, cutting off draft-phase scouting tools | Scouting fills in only after the Broadcast catches up, which is after the hero swap window closes ([05](05-player-scouting.md)). Deadlock's pre-game swap phase length isn't documented, and the author can't test it (no ranked access). Scouting stays hidden until the game has started (FP-10), which is safe either way (DEC-047) |

### Rule set

| Rule | Statement |
|---|---|
| FP-1 | The app never reads or writes Deadlock's process memory, injects code, hooks graphics APIs, or captures network packets |
| FP-2 | The app never sends keyboard, mouse or controller input to the game |
| FP-3 | The app reads only files the game and Steam write for normal use (console log, replays, Steam config), the public Broadcast, deadlock-api, and the app's own release check (FR-SET-004) |
| FP-4 | In Live Mode, the overlay and main window show only Live-Safe fields (LS-xx below) |
| FP-5 | In Review Mode, the app may use every field from the Broadcast or replay, including positions and cooldowns. Chat is recorded in the raw Broadcast but never displayed (DEC-025) |
| FP-6 | The app does not analyze fights during a match (FR-LIVE-015) |
| FP-7 | The app never retries, scrapes or mirrors data for players who opted out of deadlock-api |
| FP-8 | The app calls the Steam Web API, if at all, only for the user's own account |
| FP-9 | The app doesn't claim or imply Valve endorsement, and states that it isn't affiliated with Valve |
| FP-10 | Scouting Cards are shown only after the Broadcast shows the game has started, which means any pre-game hero-swap phase has closed. The rule doesn't depend on how long that phase is (DEC-023, DEC-047) |

### Live-Safe whitelist

A field is Live-Safe if a player could see it in-game at that moment. Fields marked **Verify** must be checked against the in-game scoreboard (task T-007). Until then they're blocked.

| ID | Field | Subject | Live-Safe | Status | Basis |
|---|---|---|---|---|---|
| LS-01 | Hero | Every player | Yes | Confirmed | Visible in-game |
| LS-02 | Player display name | Every player | Yes | Confirmed | Visible in-game |
| LS-03 | Team | Every player | Yes | Confirmed | Visible in-game |
| LS-04 | Items owned, and item purchases and sales | Every Player | Yes | Confirmed | The [shop purchase log shows enemy purchases](https://mobalytics.gg/ja_jp/deadlock/guides/patch-notes) |
| LS-05 | Items owned | Every Player | Yes | Confirmed (T-007, DEC-048) | Scoreboard |
| LS-06 | Kills, deaths, assists | Every player | Yes | Confirmed (T-007, DEC-048) | Scoreboard |
| LS-07 | Level | Every player | Yes | Confirmed (T-007, DEC-048) | Scoreboard |
| LS-08 | Souls or net worth | Every player | Yes | Confirmed (T-007, DEC-048) | Scoreboard |
| LS-09 | Assigned lane | Every player | Yes | Confirmed (T-007, DEC-048) | Scoreboard |
| LS-10 | Match clock | Match | Yes | Confirmed | HUD |
| LS-11 | Objectives destroyed (guardians, walkers, shrines, patron) | Match | Yes | Confirmed (T-007, DEC-048) | HUD and map |
| LS-12 | Own souls, items, abilities, cooldowns, health | User | Yes | Confirmed | User's own HUD |
| LS-13 | Own team's flex slot unlock state | User's team | Yes | Confirmed (T-007, DEC-048) | HUD |
| LS-14 | Historical public stats from deadlock-api | Every player | Yes | Confirmed | Public data, also shown by existing overlays |

**Never Live-Safe** (used only in Review Mode):

- Enemy positions.
- Enemy health, ability cooldowns and ultimate state.
- Enemy team chat.
- Any data on unseen enemies beyond LS-01 to LS-07.

### Enforcement

- The `live_safe` module (M-06, PRG-06) is the only path from game state to the UI in Live Mode. It whitelists by field ID.
- A test fails the build if any Live Mode payload contains a field not on the whitelist (TST-030).
- Mode switching: Live Mode starts at match start (FR-DATA-001) and ends when the Broadcast ends. Review Mode starts only after that.

### Privacy obligations

- All data is stored locally (NFR-PRIV-001).
- A privacy policy covering stored data and deadlock-api use is published before public release (NFR-PRIV-002).
- Player notes stay on the user's machine and are never uploaded.
- Display a "not affiliated with Valve" notice in About and on store listings (FP-9).
- Broadcast recordings are stored locally as raw fragments. They contain chat and positions from the match. The app never displays chat, and the privacy policy says so (DEC-025).
- The update check (FR-SET-004) is the only app-initiated request beyond deadlock-api, the Broadcast and the configured LLM provider. The privacy policy discloses it.

## Dependencies

- [02 Requirements](02-requirements.md): NFR-FAIR, NFR-PRIV.
- [04 Data sources](04-data-sources-and-ingestion.md)
- [06 In-match recommendations](06-in-match-recommendations.md)
- [10 Architecture](10-technical-architecture.md): M-06.
- [12 Programs](12-programs-and-code.md): PRG-06.
- [16 Testing](16-testing-and-qa.md): TST-030 to TST-033.

## Open Questions

- OQ-04 (resolved): All of LS-05 to LS-09, LS-11 and LS-13 are visible in-game. Lanes are visible for every player (DEC-048).
- OQ-02 (resolved): Yes. deadlock-api's live URL endpoints locate the Broadcast, so the app runs no Valve protocol client (DEC-049).
- OQ-10 (deferred): Does a paid tier conflict with SSA §2.A's non-commercial license? Not needed for v1 (DEC-018). Get legal review before any paid tier.
- OQ-19 (deferred): How long is Deadlock's pre-game hero-swap phase, and does it close before the game clock starts? Secondary sources say each player gets one swap and the phase closes before the game starts, but give no duration. The author has no ranked access, so a Beta tester with ranked access checks it (DEC-047). FP-10 is safe without the answer.

## Acceptance Criteria

- [x] Every LS field marked Verify has been checked in-game and set to Yes or No (2026-11-05, DEC-048).
- [ ] TST-030 passes: no non-Live-Safe field appears in any Live Mode payload across 20 recorded matches.
- [ ] Code review confirms no memory access, injection, hooking, packet capture or input APIs are linked. Two reviewed exceptions (DEC-079): `LoadLibrary*`, which the Windows C runtime and Rust's standard library use to load system DLLs into the app's own process; and `SendInput` in the main Windows app, linked by Slint's winit and muda libraries but unreachable, because the source check forbids the calls that reach it (bring to front, fullscreen windows, native menus). Every other injection and input API stays forbidden in every binary.
- [ ] The privacy policy is published at a stable URL before v1.
- [ ] No Scouting Card is shown until the Broadcast reports game time above 0 (FP-10). A Beta tester with ranked access confirms the swap phase is over by then.
