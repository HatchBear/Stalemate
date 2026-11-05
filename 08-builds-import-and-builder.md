# 08 — Builds: Import and Builder

## Purpose

Specifies how builds enter the app: in-game community builds, external websites and the manual builder. Also covers how the user's active Build plan is chosen.

## Scope

- **Covers:** FR-BUILD-001 to FR-BUILD-006.
- **Uses:** Builds are consumed by the Build Matcher and Build Check ([06](06-in-match-recommendations.md)) and by the Review ([07](07-post-match-review-engine.md)).

## Details

### Build model

```json
{
  "build_id": "uuid",
  "source": "ingame | external | manual",
  "source_ref": "community build ID or URL or null",
  "hero_id": 0,
  "name": "string, user-visible",
  "patch": "game build/version string at import time",
  "sections": [
    { "title": "string", "item_ids": [0, 0, 0] }
  ],
  "ability_order": [0, 0, 0],
  "created_at": "RFC 3339",
  "updated_at": "RFC 3339"
}
```

- `sections` keeps the order the source used. The Build Matcher uses all items; the Projector and Build Check use the order.
- `ability_order` is optional.
- Table layout: [11](11-data-model.md) `builds`, `build_items`.

### Sources

| Source | Requirement | Method | Phase |
|---|---|---|---|
| In-game community builds | FR-BUILD-001 | API-16 (path [VERIFY], OQ-07), searched by hero and sorted by usage | Alpha |
| External websites | FR-BUILD-002 | One adapter per site (DATA-009). Import by pasted URL or share code | v1 (Should). JSON and share-code import ship first, and site adapters follow (DEC-031) |
| Manual builder | FR-BUILD-003 | In-app editor | Alpha |

#### External site adapters

- **List of sites:** Not decided (OQ-06). FR-BUILD-002 is Should, so v1 ships JSON and share-code import first and adds site adapters as sites are chosen (DEC-031).
- **Allowed sources:** An adapter may only use a site's public export, share code or API. It may scrape HTML only if the site's terms allow it. Each adapter records the terms URL and the date it was checked.
- **Interface:** `fn can_handle(url) -> bool` and `fn import(url) -> Result<Build>` ([12](12-programs-and-code.md) PRG-19).
- **Unknown items:** Item names map to IDs through API-15. Unmatched names are listed for the user to resolve, never dropped silently.

### Manual builder (FR-BUILD-003)

- Pick a hero, add sections, add items by search, reorder items within and across sections, and set an optional ability order.
- Item data and costs come from API-15 for the current patch.
- Validation: no duplicate items within a build, and every item exists in the current patch.
- Builds export as JSON in the model above, for sharing [ASSUMPTION].

### Active Build plan

| Priority | Source | Requirement |
|---|---|---|
| 1 | The build the user marked active for this hero in the app, including a pick made during the match | FR-BUILD-005 |
| 2 | The build the user selected in-game, read from the Broadcast at game start (the user's own data, so allowed). Used only when no app pick exists for this hero | FR-BUILD-004 |
| 3 | None. Build Check is off; counter recommendations still work | — |

An explicit pick always wins (DEC-026). The user can change the active plan during a match from the main window.

### Patch handling (FR-BUILD-006)

- On each patch change, re-validate all builds against API-15.
- Flag builds with removed items: "Needs update: N items removed".
- Changed items stay, but are listed in the build's details.

## Dependencies

- [04 Data sources](04-data-sources-and-ingestion.md): API-15, API-16.
- [06 In-match recommendations](06-in-match-recommendations.md)
- [07 Review](07-post-match-review-engine.md): R-08 build deviation.
- [11 Data model](11-data-model.md)
- [12 Programs](12-programs-and-code.md): PRG-18, PRG-19.
- [14 Data assets](14-data-and-content-assets.md): DATA-009.

## Open Questions

- OQ-06: Which external sites should be supported first?
- OQ-07: What's the path and response format of the community builds endpoint (API-16)?
- OQ-05: Does the Broadcast expose the user's selected build ID at game start?
- [ASSUMPTION] JSON export for sharing builds.

## Acceptance Criteria

- [ ] 20 community builds for 5 heroes import with every item resolved (TST-080).
- [ ] A manual build round-trips export then import unchanged (TST-082).
- [ ] The active Build plan follows the priority table above (TST-083).
- [ ] After a simulated patch removes an item, affected builds are flagged (TST-084).
