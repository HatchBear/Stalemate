# 19 — Decision Log

## Purpose

Records every decision behind the design doc set, so each `DEC-nnn` reference in the other docs resolves to one place.

## Scope

- **Covers:** DEC-001 to DEC-017, rebuilt on 2026-11-05 from how docs 01–18 cite them, and DEC-018 onward from the author's review answers on the same date.
- **Does not cover:** Open questions (see [18](18-risks-and-open-questions.md)) or creative decisions (see [00](00-index.md)).
- **Note:** The original design conversation isn't available. DEC-001 to DEC-017 are reconstructed from citations, so the author should review them.

## Details

### Decisions from the design conversation (reconstructed)

| ID | Decision | Where it appears | Rationale |
|---|---|---|---|
| DEC-001 | Not cited anywhere in docs 01–18 | — | Rationale not recorded |
| DEC-002 | Overwolf is a non-goal | [01](01-overview.md) | Windows-only, and its live data method is undisclosed |
| DEC-003 | Fair-play rules: no game memory access, injection, graphics hooks, packet capture or input; Live Mode payloads enforced as Live-Safe in code | [02](02-requirements.md) NFR-FAIR-001 to 003, [03](03-fair-play-and-compliance.md) | Steam Subscriber Agreement cheating and automation clauses |
| DEC-004 | Detect match start, end and ID from the game's console log | [02](02-requirements.md) FR-DATA-001 to 003, [04](04-data-sources-and-ingestion.md) | Needs no memory access. Requires the `-condebug` launch option |
| DEC-005 | No fight tracking or analysis during a match | [01](01-overview.md), [02](02-requirements.md) FR-LIVE-015 | The Broadcast delay makes it useless, and it's the riskiest data to show |
| DEC-006 | Design all live features for the 60–90 s Broadcast delay, and show the delay | [02](02-requirements.md) FR-DATA-003, FR-UI-008, [04](04-data-sources-and-ingestion.md) | The Broadcast runs behind real time |
| DEC-007 | Platforms: Flatpak on KDE Plasma Wayland first, then Windows, each with an overlay | [02](02-requirements.md) NFR-PLAT-001, NFR-PLAT-003, FR-UI-004, FR-UI-005 | The author's platform first |
| DEC-008 | Stack: Rust, with an optional .NET fallback parser | [10](10-technical-architecture.md), [02](02-requirements.md) NFR-REL-002 | The author prefers Rust plus .NET, or Rust alone. haste parses replays and broadcasts in Rust |
| DEC-009 | Free to the user and zero running cost to the developer. LLM text runs on a local model or the user's own key | [01](01-overview.md), [02](02-requirements.md) NFR-COST-001, NFR-COST-002 | The author doesn't want to run servers. The Upkeep tier part is superseded by DEC-018 |
| DEC-010 | The overlay shows only currently needed info. The main window shows everything good to know, including past recommendations | [09](09-ui-window-and-overlay.md), [02](02-requirements.md) | Keeps the overlay readable during play |
| DEC-011 | Review is chess.com-style: win-probability swings, 9 tiers, accuracy, 5 Key Moments, rules first, LLM only for explanations | [07](07-post-match-review-engine.md), [02](02-requirements.md) FR-REV-010 to 013 | The author's requested model |
| DEC-012 | v1 should "ideally have everything", delivered through Alpha, Beta and v1 milestones | [02](02-requirements.md), [17](17-roadmap.md) | The author's stated scope |
| DEC-013 | The Build plan comes from a manual pick or from the in-game selected build read from the Broadcast | [02](02-requirements.md) FR-BUILD-004, FR-BUILD-005, [08](08-builds-import-and-builder.md) | Two ways to learn the user's plan. Priority amended by DEC-026 |
| DEC-014 | Playstyle is shown as percentiles of available stats | [02](02-requirements.md) FR-SCOUT-008 | Avoids opinionated labels |
| DEC-015 | Keep a Purchase Log of enemy buys and sells, used as a counter-item input | [02](02-requirements.md) FR-LIVE-002, FR-LIVE-008, [06](06-in-match-recommendations.md) | Enemy purchases are Live-Safe |
| DEC-016 | Not cited anywhere in docs 01–18 | — | Rationale not recorded |
| DEC-017 | No per-player ban habits. Match-level ban rates are a Could item | [01](01-overview.md), [02](02-requirements.md) FR-SCOUT-014, FR-SCOUT-016, [05](05-player-scouting.md) | Ban data has no player attached |

### Decisions from the author review (2026-11-05)

| ID | Decision | Changed | Rationale |
|---|---|---|---|
| DEC-018 | The paid Upkeep tier and the hosted LLM provider are out of v1 scope. The app stays free | [01](01-overview.md), [03](03-fair-play-and-compliance.md), [07](07-post-match-review-engine.md), [11](11-data-model.md), [17](17-roadmap.md), [18](18-risks-and-open-questions.md) | Avoids the SSA §2.A non-commercial licence question and any running cost |
| DEC-019 | Standard mode only. Other modes are recorded but not analyzed | [01](01-overview.md), [04](04-data-sources-and-ingestion.md), [07](07-post-match-review-engine.md) | Keeps scope manageable |
| DEC-020 | The app is licensed GPL-3.0 | [01](01-overview.md), [10](10-technical-architecture.md), [15](15-tools-build-and-deploy.md) | Fits Slint under GPLv3 and open-source code signing |
| DEC-021 | T-008, the broadcast locator spike, is a hard go/no-go gate. If it fails, Alpha ships API scouting, manual roster entry and a post-match Review only | [04](04-data-sources-and-ingestion.md), [17](17-roadmap.md), [18](18-risks-and-open-questions.md) | Every live feature depends on a compliant locator |
| DEC-022 | Scouting fills progressively. Basic fields come first, and Build Profile, Power spikes and Playstyle follow. Lift lookups are lazy and cached per patch. SM-2 applies to the basic fields | [01](01-overview.md), [02](02-requirements.md), [04](04-data-sources-and-ingestion.md), [05](05-player-scouting.md), [12](12-programs-and-code.md), [16](16-testing-and-qa.md) | The original request budget left out the lift and percentile calls |
| DEC-023 | Scouting is hidden until the game has started and any pre-game swap phase has closed (verification deferred, see DEC-047) | [03](03-fair-play-and-compliance.md) FP-10, [05](05-player-scouting.md), [14](14-data-and-content-assets.md), [17](17-roadmap.md) | Secondary sources say the phase closes before the game starts, but give no duration |
| DEC-024 | Bundle the .NET fallback parser on both platforms. Measure the size at T-004 and revisit if needed | [15](15-tools-build-and-deploy.md), [17](17-roadmap.md) | A haste break after a patch is likely, and Linux is the primary platform |
| DEC-025 | Recordings keep the raw match, including chat. Chat is never displayed. The privacy policy discloses it | [03](03-fair-play-and-compliance.md), [11](11-data-model.md), [12](12-programs-and-code.md) | The fallback parser needs the raw fragments |
| DEC-026 | An explicit manual Build plan pick always wins. The in-game build is the default only when none is set for the hero | [02](02-requirements.md), [08](08-builds-import-and-builder.md) | The old order could override a deliberate choice |
| DEC-027 | Add a replay download fallback through a replay URL endpoint (API-17), gated on the same compliance check as the locator | [02](02-requirements.md), [04](04-data-sources-and-ingestion.md), [12](12-programs-and-code.md), [16](16-testing-and-qa.md), [17](17-roadmap.md) | FR-DATA-005 named a replay fallback with no source or task |
| DEC-028 | Check GitHub Releases at startup. On by default, can be turned off, and is disclosed | [02](02-requirements.md) FR-SET-004, [03](03-fair-play-and-compliance.md), [09](09-ui-window-and-overlay.md), [16](16-testing-and-qa.md), [17](17-roadmap.md) | The Windows MSI has no updater, and game patches can break the app |
| DEC-029 | Accuracy counts each swing once (priority death, fight, objective). Mean EPL is averaged per decision type | [07](07-post-match-review-engine.md), [12](12-programs-and-code.md), [14](14-data-and-content-assets.md) | Overlapping windows double-counted, and farm minutes outweighed deaths |
| DEC-030 | The LLM provider defaults to Off | [07](07-post-match-review-engine.md), [18](18-risks-and-open-questions.md) | Costs nothing and needs no setup |
| DEC-031 | External build importers are Should. JSON and share-code import ship first | [02](02-requirements.md), [08](08-builds-import-and-builder.md), [14](14-data-and-content-assets.md), [17](17-roadmap.md) | The site list is undecided |
| DEC-032 | Hand-fill objective timers and flex slot rules from the current patch before v1, and re-check them on patch day | [14](14-data-and-content-assets.md), [16](16-testing-and-qa.md) | These values change with patches |
| DEC-033 | Apply to a free open-source code signing programme. Ship unsigned with a README note until approved | [15](15-tools-build-and-deploy.md), [18](18-risks-and-open-questions.md) | Zero-cost goal |
| DEC-034 | Add a 30% contingency to the effort estimate. No target dates are set | [17](17-roadmap.md), [18](18-risks-and-open-questions.md) | The estimate had no contingency |
| DEC-035 | Keep match-level ban rates as a Could item | [02](02-requirements.md), [05](05-player-scouting.md), [18](18-risks-and-open-questions.md) | Small effort (1 day) |
| DEC-036 | The product name is Stalemate, confirmed by the author as unrelated to Valve. The repository is github.com/HatchBear/Stalemate and the Flatpak app ID is `io.github.hatchbear.stalemate` | [00](00-index.md) | The author's choice |
| DEC-037 | No playstyle tags. The nine tier names (Spot On, Solid, Fine, Slip, Costly, Disaster, Clutch, Gamble, Missed Shot) are approved | [05](05-player-scouting.md), [07](07-post-match-review-engine.md) | CD-09 decided. CD-07 and CD-08 pending |
| DEC-038 | Approve all assumptions in [18](18-risks-and-open-questions.md) as starting values | [01](01-overview.md), [02](02-requirements.md), [18](18-risks-and-open-questions.md) | Author sign-off, to be tuned during Alpha |
| DEC-039 | UI navigation sounds are an asset of their own (AUD-002), on by default, with mute and volume. The overlay alert sound (AUD-001) stays undecided | [02](02-requirements.md) FR-UI-009, [09](09-ui-window-and-overlay.md), [14](14-data-and-content-assets.md), [16](16-testing-and-qa.md), [17](17-roadmap.md) | The author's choice for CD-12 |
| DEC-040 | The visual style reflects how Deadlock's in-game UI looks, using original artwork only | [09](09-ui-window-and-overlay.md) CD-02 | The author's direction. Valve art is off limits (SSA §2.A) |
| DEC-041 | Add a team comp win-rate prediction, based on deadlock-api's team builder, computed once the roster is known and shown in the main window only. It is not a pre-game feature | [02](02-requirements.md) FR-LIVE-016, [04](04-data-sources-and-ingestion.md), [06](06-in-match-recommendations.md), [16](16-testing-and-qa.md), [17](17-roadmap.md) | The author asked for pre-game analytics. A true pre-game feature would conflict with FP-10 and the Dota 2 precedent, and the roster isn't available before the game starts |
| DEC-042 | The win-probability model uses only features available in both API-05 and the Broadcast. One model is trained on the last few patches and retrained only when its Brier score degrades | [07](07-post-match-review-engine.md), [12](12-programs-and-code.md), [14](14-data-and-content-assets.md), [15](15-tools-build-and-deploy.md), [16](16-testing-and-qa.md), [18](18-risks-and-open-questions.md) | API-05 may lack some features. A new patch alone shouldn't force 20,000 matches of retraining |
| DEC-043 | The coach is a supportive friend: really excited when you do something cool, really disappointed when you mess up, hyping you up or giving you a hard time after a match. No profanity by default | [07](07-post-match-review-engine.md) CD-08, [14](14-data-and-content-assets.md) | The author's description |
| DEC-044 | Handoff defaults by the author's delegation: the binary and path name is `stalemate`. The author does the hands-on game tasks and supplies UI reference screenshots. The baseline High-impact risks are accepted. No profanity from the coach, AUD-002 specifics, a 3-patch model pool, FR-LIVE-016 as Should, and a GitHub Pages privacy policy are accepted | [00](00-index.md), [17](17-roadmap.md), [18](18-risks-and-open-questions.md) | The author asked for best judgement on the remaining sign-offs |
| DEC-045 | The overlay stays silent. The overlay alert sound (AUD-001) is dropped | [14](14-data-and-content-assets.md) | UI navigation sounds cover the audio the author wanted |
| DEC-046 | Palette, type, overlay look and motion decided: `#222021`, `#2f4442`, `#3f5d4d`, `#72947f`, `#efdebf` (text and borders), plus Deadlock's Amber `#D4860B` and Sapphire `#4D75C3` for teams. Jost and Noto Sans. Minimal motion | [09](09-ui-window-and-overlay.md) CD-03 to CD-06 | The author's palette. Team colours come from Deadlock's own UI style values |
| DEC-047 | The author has no ranked access and can't test the swap phase or other ranked-only behavior. FP-10 stays as a conservative rule that doesn't depend on the swap phase's length. At least 2 Beta testers need ranked access | [03](03-fair-play-and-compliance.md), [16](16-testing-and-qa.md), [17](17-roadmap.md), [18](18-risks-and-open-questions.md) | The rule is safe either way, and a tester can still verify it |

## Dependencies

- All documents [01](01-overview.md)–[18](18-risks-and-open-questions.md), which cite these IDs.

## Open Questions

- DEC-001 and DEC-016 aren't cited anywhere. What were they?
- Do the reconstructed rationales for DEC-002 to DEC-017 match the author's memory of the design conversation?

## Acceptance Criteria

- [ ] Every `DEC-nnn` cited in docs 01–18 has a row here.
- [ ] The author has reviewed DEC-001 to DEC-017.
