# 05 — Player Scouting

## Purpose

Specifies the scouting system: what each Scouting Card shows, how every value is computed, and the Counter-buy filter.

## Scope

- **Covers:** Scouting data for the 11 other players and for the user. Requirements FR-SCOUT-001 to FR-SCOUT-016.
- **Does not cover:** In-match build detection, which is in [06](06-in-match-recommendations.md).

## Details

### Trigger and timing

1. The Broadcast delivers the roster: hero, Account ID and team for 12 players.
2. Scouting starts for all 11 other players in parallel, within the rate limits in [04](04-data-sources-and-ingestion.md). Fields fill in progressively: the basic fields first (FR-SCOUT-001 to 004), then Build Profile, Power spikes and Playstyle (DEC-022).
3. Scouting stays hidden until the Broadcast shows the game has started, which is after the pre-game hero swap phase closes, so scouting can't affect hero picks ([03](03-fair-play-and-compliance.md) FP-10, DEC-023). The length of that phase isn't documented, so the rule doesn't depend on it (DEC-047).
4. If the Broadcast is unavailable (FR-DATA-007), the user can enter or paste players manually [ASSUMPTION].

### Scouting Card fields

| Field | Requirement | Source | Computation |
|---|---|---|---|
| Overall win/loss | FR-SCOUT-001 | API-03 | Sum of wins and matches across heroes |
| Last 20 win/loss | FR-SCOUT-001 | API-01 | Wins in the 20 most recent matches |
| Rank and MMR trend | FR-SCOUT-002 | API-02, API-01 | Current rank from API-02. Trend: least-squares slope of cumulative `ranked_delta` over the last 20 ranked matches in API-01, shown as change per 10 matches in rank progress points (DEC-051) |
| Mains | FR-SCOUT-003 | API-03 | Top 5 heroes by matches, each with matches and win rate |
| Experience on current hero | FR-SCOUT-004 | API-03 | Lifetime matches on the hero in the roster |
| Build Profile | FR-SCOUT-005 | API-06, API-05 | See Build Profile below |
| Power spikes | FR-SCOUT-007 | API-11, API-05 | Ability most often maxed first; median purchase time of each core item |
| Playstyle | FR-SCOUT-008 | API-05, API-12 | See Playstyle profile |
| Flags | FR-SCOUT-009 | API-01, API-02, API-05, local DB | See Flags |
| Notes | FR-SCOUT-010 | Local DB | The user's notes for this Account ID |

### Build Profile

For player *p* on hero *h*:

1. **Selected builds:** API-06 with `account_ids=[p]` and `hero_build_id` breakdown gives which community builds *p* selected at game start, and how often.
2. **Purchases:** API-05 for *p*'s last 30 matches on *h* (fewer if they don't have 30) gives each item bought and the enemy heroes in each match.
3. **Counter-buy filter** (FR-SCOUT-006): classifies every item *i* bought by *p* on *h*.

Definitions:

- `M` = *p*'s matches on *h* in the sample. `|M|` = sample size.
- `r(i)` = share of matches in `M` where *p* bought *i*.
- `r(i | e)` = share of matches in `M` with enemy hero *e* where *p* bought *i*.
- `L(i, e)` = global lift: the purchase rate of *i* by hero *h* when *e* is on the enemy team (API-07 with `hero_ids=[h]`, `enemy_hero_ids=[e]`), divided by the purchase rate of *i* by *h* overall.
- `S` = the item set of *p*'s most-selected build.

| Class | Rule (thresholds in DATA-001) |
|---|---|
| Core | `r(i) ≥ 0.60` and, for every enemy hero *e* with at least 3 matches, `r(i \| e) − r(i) < 0.20` |
| Counter buy | `r(i) < 0.60`, and there is an enemy hero *e* with at least 3 matches where `r(i \| e) ≥ 2 × r(i)` and `L(i, e) ≥ 1.3` |
| Situational | Every other item with `r(i) ≥ 0.15` |
| Off-plan | Any item not in `S`. This is a separate tag that can combine with the classes above |
| Ignored | `r(i) < 0.15` and not a counter buy |

**Sample guards:**

- `|M| < 10`: Mark the profile "Low sample". Fill core items from API-06 global data for hero *h* at the player's rank.
- `|M| = 0`: Show the most popular build for *h* at the player's rank, labeled as a global default.

All thresholds are [ASSUMPTION] starting values. Tune them with TST-024.

### Playstyle profile

Each metric is shown as a percentile against players of the same hero within ±1 rank tier (API-12 with badge filters). The sample is the same last 30 matches on the current hero as the Build Profile.

| Metric | Formula | Source |
|---|---|---|
| Farm pace | Souls per minute, mean over the sample | API-05 |
| Hero damage share | Hero damage ÷ (hero damage + objective damage) | API-05 |
| Objective damage per minute | Objective damage ÷ minutes | API-05 |
| Healing per minute | Healing ÷ minutes | API-05 |
| Deaths per 10 minutes | Deaths ÷ minutes × 10 | API-05 |
| KDA | (Kills + assists) ÷ max(deaths, 1) | API-05 |
| Usual lane | Mode of assigned lane | API-05 [VERIFY field] |
| Swap rate | Share of matches where pre-swap hero ≠ played hero | API-05 [VERIFY field] |

> **[CREATIVE DECISION – AUTHOR TO FILL IN]**
> Topic: CD-09 — Optional playstyle tag names (short labels summarizing a profile, if wanted)
> Constraints from the project: Each tag must map to a rule over the metrics above (for example, farm pace ≥ 80th percentile). Tags must fit the Scouting Card's width (see [09](09-ui-window-and-overlay.md)).
> Options to consider (optional): No tags, percentiles only; tags.
> Decision: No tags. Percentiles only (2026-11-05, DEC-037).

### Flags

| Flag | Rule (DATA-001, all [ASSUMPTION]) | Source |
|---|---|---|
| Possible smurf | Lifetime matches < 100 and last-20 win rate ≥ 70%, or MMR slope in the top 5% of players at that rank | API-01, API-02, API-03 |
| Party stack | Two or more enemies share a party ID in the current match | Broadcast or API-05 [VERIFY field] |
| Met before | The Account ID appears in any match in the local DB | Local DB |

### About the user

| Item | Requirement | Source | Computation |
|---|---|---|---|
| Matchups | FR-SCOUT-011 | API-09, API-10 with `account_ids=[user]` | Record of the user's hero against each enemy hero, and with each teammate's hero |
| Repeat players | FR-SCOUT-012 | Local DB | The user's wins and losses in past matches with or against each player |
| Session check | FR-SCOUT-013 | Local DB | A reminder in the main window (never the overlay) after 3 consecutive losses, or 3 h of play in a Session |

### Ban habits

- **Per-player ban habits (FR-SCOUT-014):** Won't build. Ban data is recorded per match with no player attached (DEC-017).
- **Match-level ban rates (FR-SCOUT-016, Could):** Show ban rates for the current rank range from API-13. Kept as a Could item (OQ-08 resolved, DEC-035).

### Edge cases

| Case | Behavior |
|---|---|
| Player opted out of deadlock-api | Show "No public data". No retries (NFR-FAIR-004) |
| Bot or empty slot | Show the hero only |
| deadlock-api returns `503` | Show cached data with its age. Retry per `Retry-After` |
| Player changed display name | Key on Account ID only |
| Fewer than 20 matches | Show W/L over what exists, labeled with N |

## Dependencies

- [03 Fair play](03-fair-play-and-compliance.md)
- [04 Data sources](04-data-sources-and-ingestion.md): API-01 to API-13.
- [09 UI](09-ui-window-and-overlay.md): Scouting view.
- [11 Data model](11-data-model.md)
- [12 Programs](12-programs-and-code.md): PRG-08, PRG-09.
- [14 Data assets](14-data-and-content-assets.md): DATA-001.

## Open Questions

- OQ-05: Does API-05 expose per-match party ID, pre-swap hero and assigned lane?
- OQ-08 (resolved): Match-level ban rates are kept as a Could item (DEC-035).
- [ASSUMPTION] All thresholds in this doc.
- [ASSUMPTION] Manual player entry when the Broadcast is unavailable.

## Acceptance Criteria

- [ ] The basic fields of all Scouting Cards fill within 30 s of the roster arriving, for players with public data (TST-020).
- [ ] The Counter-buy filter classifies at least 80% of items the same way as a hand-labeled set of 30 player-hero profiles (TST-024).
- [ ] Players who opted out show "No public data" and cause no retries (TST-032).
- [ ] The session reminder never appears in the overlay (TST-0291).
