# 09 — UI: Main Window and Overlay

## Purpose

Specifies what the main window and overlay contain, how they behave, and how the overlay works on each platform. Visual design is left to the author.

## Scope

- **Covers:** Information architecture, behavior, states and platform mechanics for FR-UI-001 to FR-UI-008.
- **Does not cover:** Visual style, color, type, layout aesthetics and motion. These are creative decisions CD-02 to CD-06 below.

## Details

### Split of responsibilities (DEC-010)

- **Overlay:** Currently needed info only, during Live Mode.
- **Main window:** Everything good to know, including past item recommendations, scouting and reviews.

### Main window views (FR-UI-001)

| View | Contents | Requirements |
|---|---|---|
| Home | Status of setup checks (Steam found, `-condebug` working, Account ID, Broadcast state, deadlock-api reachable) and the last match summary | FR-SET-003 |
| Scouting | 11 Scouting Cards grouped by team, plus the "About you" panel | FR-SCOUT-001 to 016 |
| Live | Enemy board, Purchase Log, Build Matcher results, projections, top 5 counter items with drivers, Build Check, pace, timers, flex slots, recommendation history, team comp prediction | FR-LIVE-001 to 014, FR-LIVE-016 |
| Reviews | List of past Reviews, and the Review report ([07](07-post-match-review-engine.md)) | FR-REV-001 to 016 |
| Builds | Build library, importers, manual builder, active Build plan per hero | FR-BUILD-001 to 006 |
| Players | Players seen before, records with and against them, notes | FR-SCOUT-010, FR-SCOUT-012, FR-REV-017 |
| Settings | Account, LLM provider, own API keys, update check toggle, UI sound mute and volume, overlay hotkey, anchor and scale, data export and delete, privacy policy, About (with the "not affiliated with Valve" notice) | FR-SET-001, FR-SET-002, FR-SET-004, FR-UI-009, FP-9 |

During Live Mode, the Live view shows only Live-Safe fields (NFR-FAIR-003).

### Overlay panels (FR-UI-002)

| ID | Panel | Content | Max content |
|---|---|---|---|
| OV-1 | Counter items | Phase label (Lane or Team), top 3 items with icon and score change | 3 rows |
| OV-2 | Build swap | Top suggestion: "swap X → Y" | 1 row |
| OV-3 | Pace | Souls/min difference against benchmark; next core item ahead, on time or behind | 1 row |
| OV-4 | Next objective | Next timer or flex slot requirement | 1 row |
| OV-5 | Status | Broadcast delay in seconds; a stale warning when the delay is over 60 s (DEC-050) | 1 row |

Each panel can be turned on or off in Settings.

### Overlay behavior

| Behavior | Rule |
|---|---|
| Visibility | Shown only in Live Mode; hidden automatically otherwise (FR-UI-007) |
| Input | Always click-through. Never takes keyboard focus (FR-UI-003) |
| Toggle | A global hotkey set by the user. No default binding [ASSUMPTION] |
| Position | One of 9 screen anchors, plus an x/y offset in logical pixels |
| Scale | 75%–200% in 25% steps |
| Update rate | Redraws only on data change, at most once per second (NFR-PERF-003) |
| Game mode required | Borderless windowed or windowed. Exclusive fullscreen can't be overlaid |

### Platform mechanics

| Platform | Overlay surface | Click-through | Global hotkey |
|---|---|---|---|
| KDE Plasma 6, Wayland (primary) | wlr-layer-shell surface on the `overlay` layer, `keyboard_interactivity = none`, no exclusive zone | Empty input region (`wl_surface.set_input_region` with an empty region) | XDG desktop portal GlobalShortcuts (through `ashpd`) |
| Linux X11 (best effort, NFR-PLAT-004) | Override-redirect, always-on-top window | X Shape extension, empty input shape | Portal if available; otherwise disabled |
| Windows 10/11 | Top-level window with `WS_EX_LAYERED \| WS_EX_TRANSPARENT \| WS_EX_TOPMOST \| WS_EX_NOACTIVATE \| WS_EX_TOOLWINDOW` | `WS_EX_TRANSPARENT` | `RegisterHotKey` |

Whether KWin keeps an `overlay`-layer surface above a game running under XWayland must be tested (RSK-05, TST-090).

### Data states

| State | Condition | Shown as |
|---|---|---|
| Loading | Request in flight | A placeholder per field |
| Empty | No public data (opt-out or new account) | "No public data" |
| Stale | Broadcast delay over 60 s (DEC-050), or cached data older than its TTL | Stale marker with age |
| Error | Source failed after retries | Error with source name and a retry action |
| Offline | No network | Banner; cached data stays visible |

### Accessibility [ASSUMPTION]

- No information is carried by color alone. Tiers, flags and states also use icon shape or text.
- Window text scales 75%–200%.
- Every window control is reachable by keyboard.

### Creative decisions

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-02 — Overall visual style of the main window and overlay
> Constraints from the project: Rendered with Slint ([10](10-technical-architecture.md)). Must stay readable over any game scene at 75% scale. Must meet the accessibility rules above.
> Options to consider (optional): —
> Decision: Reflect how Deadlock's own in-game UI looks (2026-11-05, DEC-040). The app uses original artwork only: no Valve art, logos or fonts ([13](13-art-and-asset-pipeline.md), [03](03-fair-play-and-compliance.md) SSA §2.A). [ui.deadlock-api.com](https://ui.deadlock-api.com/) is a reference only: it is a web-component library for item cards, which Slint can't use. CD-03 to CD-06 stay open and follow from this direction.

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-03 — Color palette, including tier colors and state colors
> Constraints from the project: Text contrast of at least 4.5:1 against its own background, including the overlay background over game scenes. Not the only carrier of meaning.
> Options to consider (optional): —
> Decision: Decided 2026-11-05 (DEC-046). The darker the colour, the more prominent it is.
> - `#222021`: base background and the largest areas.
> - `#2f4442`: panels, cards and rows.
> - `#3f5d4d`: raised, hovered and selected fills.
> - `#72947f`: secondary text, icons and dividers, on `#222021` only (4.83:1).
> - `#efdebf`: all primary text and all borders (12.24:1 on `#222021`, 7.83:1 on `#2f4442`, 5.5:1 on `#3f5d4d`).
> - Teams, taken from Deadlock's own UI style definitions (hex values only, read from the local game install): Amber `#D4860B` with text `#201500` (6.17:1), and Sapphire `#4D75C3` with text `#FFFFFF` (4.52:1). Lighter tones for text on dark surfaces: Amber `#FBDCA0`, Sapphire `#BDCBFF` (10.14:1 on `#222021`). Darker tones for fills: Amber `#937600` and `#6F3806`, Sapphire `#4156A0` and `#243265`.
> - Plain Amber and Sapphire fail 4.5:1 as text on `#2f4442`, so use them as fills, or use the lighter tones for text.
> - Team colour follows the actual side (Amber or Sapphire). "You" and "Enemy" are shown as text.
> - Tiers are told apart by icon shape and name, and colour only adds to it. T1, T2, T7 and T8 use `#efdebf` on `#3f5d4d`. T3 and T4 use `#efdebf` on `#2f4442`. T5, T6 and T9 use `#ee5945`, the game's warning red (4.74:1 on `#222021`) [ASSUMPTION].
> - Error uses `#ee5945`. Stale and offline use `#72947f`. Every state also carries an icon and text.

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-04 — Typography
> Constraints from the project: The font license must allow embedding and redistribution in a Flatpak and a Windows installer (for example, SIL OFL). Item names must fit OV-1 rows at 75% scale.
> Options to consider (optional): —
> Decision: Decided 2026-11-05 (DEC-046). Jost (SIL OFL) for headings and UI labels, as Deadlock's own catalog UI uses Jost. Noto Sans (SIL OFL) for body text, dense numbers and non-Latin fallback. Valve's own display fonts (Radiance, Reaver) are not used, because their redistribution licence is unverified. Item-name fit in OV-1 at 75% scale is checked in T-027.

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-05 — Overlay panel look and the layout aesthetics of both surfaces
> Constraints from the project: The panel set and row limits above are fixed. Panels must be readable at a glance. Background opacity must be adjustable [ASSUMPTION].
> Options to consider (optional): —
> Decision: Decided 2026-11-05 (DEC-046), using the CD-03 colours. Overlay panels: `#222021` background at 85% opacity by default [ASSUMPTION], adjustable in Settings. Rows sit on `#2f4442`, with a 1 px `#efdebf` border and `#efdebf` text. The Lane or Team label uses the lighter tone of the user's team colour. The main window uses the same tokens: `#222021` base, `#2f4442` panels, `#3f5d4d` selected, and 1 px `#efdebf` borders.

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-06 — Motion and transitions
> Constraints from the project: The overlay redraws at most once per second (NFR-PERF-003). Animations must not raise CPU above NFR-PERF-001.
> Options to consider (optional): No motion; minimal motion.
> Decision: Minimal motion (2026-11-05, DEC-046). The overlay has no animation. The main window uses fades and slides of 150 ms or less, and honours the system reduce-motion setting [ASSUMPTION].

## Dependencies

- [03 Fair play](03-fair-play-and-compliance.md)
- [05](05-player-scouting.md), [06](06-in-match-recommendations.md), [07](07-post-match-review-engine.md), [08](08-builds-import-and-builder.md): content.
- [10 Architecture](10-technical-architecture.md): M-18, M-19.
- [12 Programs](12-programs-and-code.md): PRG-20, PRG-22.
- [13 Assets](13-art-and-asset-pipeline.md)
- [14 Data assets](14-data-and-content-assets.md): DATA-006 strings.

## Open Questions

- RSK-05: Does KWin keep a layer-shell overlay above a full-screen XWayland or Wayland game?
- [ASSUMPTION] No default hotkey, adjustable overlay opacity, and the accessibility rules.

## Acceptance Criteria

- [ ] On KDE Plasma 6 Wayland, the overlay stays above Deadlock in borderless windowed mode and passes clicks through (TST-090).
- [ ] On Windows 11, the same holds (TST-091).
- [ ] The overlay is hidden outside Live Mode (TST-053).
- [ ] Every data state above has been reached in testing and renders as specified (TST-050).
- [x] CD-02 to CD-06 are filled in before Beta UI work starts (2026-11-05).
