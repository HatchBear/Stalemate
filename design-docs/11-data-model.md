# 11 — Data Model

## Purpose

Defines all persistent data (the SQLite schema and files) and all structured messages (IPC and LLM fact sheets).

## Scope

- **Covers:** The local database, recording files, retention, and JSON schemas for `OverlayMessage`, `LiveSafeState`, `PurchaseEvent` and `ReviewFacts`.
- **Elsewhere:** Config file formats are in [14](14-data-and-content-assets.md).

## Details

### Storage overview

| Store | Format | Location ([10](10-technical-architecture.md)) | Owner |
|---|---|---|---|
| Database | SQLite 3, WAL mode | `<data>/app.db` | M-16 |
| Broadcast recordings | Raw fragments, appended | `<data>/recordings/<match_id>.bcast` | M-04 |
| Asset cache | JSON plus images | `<data>/assets/<patch>/` | M-03 |
| Logs | Text, rotated daily, 7 files | `<data>/logs/` | All |

### SQLite schema (version 1)

```sql
PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE schema_version (version INTEGER NOT NULL);
INSERT INTO schema_version VALUES (1);

-- Players seen in any match (no data beyond what deadlock-api returns)
CREATE TABLE players (
  account_id     INTEGER PRIMARY KEY,
  display_name   TEXT,
  last_seen_at   TEXT NOT NULL            -- RFC 3339
);

CREATE TABLE player_notes (
  id             INTEGER PRIMARY KEY,
  account_id     INTEGER NOT NULL REFERENCES players(account_id) ON DELETE CASCADE,
  note           TEXT NOT NULL CHECK (length(note) <= 2000),
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

-- The user's own matches
CREATE TABLE matches (
  match_id       INTEGER PRIMARY KEY,
  started_at     TEXT NOT NULL,
  duration_s     INTEGER,
  game_mode      TEXT NOT NULL,            -- 'normal' | 'street_brawl'
  match_mode     TEXT,                     -- 'ranked' | 'unranked' | ...
  patch          TEXT NOT NULL,
  user_team      INTEGER NOT NULL,
  user_hero_id   INTEGER NOT NULL,
  result         TEXT CHECK (result IN ('win','loss','abandoned')),
  data_source    TEXT NOT NULL CHECK (data_source IN ('broadcast','replay','api')),
  recording_path TEXT
);

CREATE TABLE match_players (
  match_id       INTEGER NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
  account_id     INTEGER NOT NULL,
  team           INTEGER NOT NULL,
  hero_id        INTEGER NOT NULL,
  assigned_lane  INTEGER,                  -- from API-05 (OQ-05 resolved)
  party_id       INTEGER,                  -- always NULL for now: API-05 has no party field (DEC-053)
  PRIMARY KEY (match_id, account_id)
);

CREATE TABLE purchase_events (
  id             INTEGER PRIMARY KEY,
  match_id       INTEGER NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
  account_id     INTEGER NOT NULL,
  t_game_s       INTEGER NOT NULL,
  item_id        INTEGER NOT NULL,
  action         TEXT NOT NULL CHECK (action IN ('buy','sell','upgrade')),
  cost           INTEGER NOT NULL
);
CREATE INDEX purchase_events_match ON purchase_events(match_id, t_game_s);

CREATE TABLE recommendation_history (
  id             INTEGER PRIMARY KEY,
  match_id       INTEGER NOT NULL REFERENCES matches(match_id) ON DELETE CASCADE,
  t_game_s       INTEGER NOT NULL,
  phase          TEXT NOT NULL CHECK (phase IN ('lane','team')),
  items_json     TEXT NOT NULL,            -- [{item_id, score, delta_pp}]
  drivers_json   TEXT NOT NULL             -- [{account_id, hero_id, weight}]
);

-- Builds (see 08)
CREATE TABLE builds (
  build_id       TEXT PRIMARY KEY,         -- UUID
  source         TEXT NOT NULL CHECK (source IN ('ingame','external','manual')),
  source_ref     TEXT,
  hero_id        INTEGER NOT NULL,
  name           TEXT NOT NULL,
  patch          TEXT NOT NULL,
  ability_order  TEXT,                     -- JSON array of ability IDs
  needs_update   INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL,
  updated_at     TEXT NOT NULL
);

CREATE TABLE build_items (
  build_id       TEXT NOT NULL REFERENCES builds(build_id) ON DELETE CASCADE,
  section_index  INTEGER NOT NULL,
  section_title  TEXT NOT NULL,
  position       INTEGER NOT NULL,
  item_id        INTEGER NOT NULL,
  PRIMARY KEY (build_id, section_index, position)
);

CREATE TABLE active_build_plans (
  hero_id        INTEGER PRIMARY KEY,
  build_id       TEXT NOT NULL REFERENCES builds(build_id) ON DELETE CASCADE
);

-- Reviews (see 07)
CREATE TABLE reviews (
  match_id       INTEGER PRIMARY KEY REFERENCES matches(match_id) ON DELETE CASCADE,
  accuracy       INTEGER,                  -- 0..100, NULL for stats-only
  accuracy_lane  INTEGER,
  accuracy_mid   INTEGER,
  accuracy_late  INTEGER,
  tier_counts    TEXT,                     -- JSON {"T1":n,...,"T9":n}
  facts_json     TEXT NOT NULL,            -- ReviewFacts
  report_json    TEXT NOT NULL,            -- rendered report data
  model_version  TEXT,
  llm_provider   TEXT NOT NULL CHECK (llm_provider IN ('off','local','own_key')),
  partial        INTEGER NOT NULL DEFAULT 0,
  created_at     TEXT NOT NULL
);

CREATE TABLE review_decisions (
  id             INTEGER PRIMARY KEY,
  match_id       INTEGER NOT NULL REFERENCES reviews(match_id) ON DELETE CASCADE,
  code           TEXT NOT NULL CHECK (code IN ('D-DEATH','D-ITEM','D-OBJ','D-FARM','D-FIGHT')),
  t_game_s       INTEGER NOT NULL,
  epl            REAL NOT NULL CHECK (epl BETWEEN 0 AND 1),
  delta_p        REAL NOT NULL,
  tier           TEXT NOT NULL CHECK (tier IN ('T1','T2','T3','T4','T5','T6','T7','T8','T9')),
  is_key_moment  INTEGER NOT NULL DEFAULT 0,
  facts_json     TEXT NOT NULL
);

CREATE TABLE review_findings (
  id             INTEGER PRIMARY KEY,
  match_id       INTEGER NOT NULL REFERENCES reviews(match_id) ON DELETE CASCADE,
  rule_id        TEXT NOT NULL,            -- 'R-01'..'R-12'
  t_game_s       INTEGER,
  payload_json   TEXT NOT NULL
);

-- API response cache (see 04)
CREATE TABLE api_cache (
  cache_key      TEXT PRIMARY KEY,         -- method + path + sorted query
  endpoint_id    TEXT NOT NULL,            -- 'API-01'..'API-17'
  fetched_at     TEXT NOT NULL,
  ttl_s          INTEGER NOT NULL,         -- -1 = until patch change
  patch          TEXT,
  http_status    INTEGER NOT NULL,
  body           BLOB NOT NULL
);
```

### Retention [ASSUMPTION]

| Data | Kept |
|---|---|
| Broadcast recordings | The last 50 matches or 30 days, whichever is more. The Review is kept after its recording is deleted. Recordings hold the raw match, including chat. Chat is never displayed (DEC-025) |
| `api_cache` | Expired rows purged at startup |
| Matches, purchases, reviews, builds, notes | Until the user deletes them (FR-SET-002) |

### Message schemas

#### `PurchaseEvent`

```json
{ "match_id": 0, "account_id": 0, "t_game_s": 0, "item_id": 0, "action": "buy|sell|upgrade", "cost": 0 }
```

#### `LiveSafeState` (output of M-06)

Only fields LS-01 to LS-14 from [03](03-fair-play-and-compliance.md). A field whose LS status is "Verify" is omitted until the whitelist (DATA-003) enables it.

```json
{
  "match_id": 0,
  "t_game_s": 0,
  "delay_s": 0,
  "players": [
    { "account_id": 0, "name": "LS-02", "team": 0, "hero_id": 0,
      "items": [0], "kda": [0, 0, 0], "level": 0, "lane": 0 }
  ],
  "objectives": { "destroyed": [] },
  "self": { "souls": 0, "items": [0], "cooldowns": {}, "flex_slots": 0 }
}
```

#### `OverlayMessage` (M-17)

```json
{
  "type": "update | show | hide | config | ack | hotkey | error",
  "seq": 0,
  "payload": {
    "ov1_counters": { "phase": "lane|team", "items": [ { "item_id": 0, "delta_pp": 0.0 } ] },
    "ov2_swap":     { "from_item_id": 0, "to_item_id": 0 },
    "ov3_pace":     { "spm_delta_pct": 0, "next_core_item_id": 0, "timing": "ahead|on_time|behind" },
    "ov4_next":     { "kind": "timer|flex", "label_key": "string", "t_game_s": 0 },
    "ov5_status":   { "delay_s": 0, "stale": false }
  }
}
```

Every key in `payload` must trace to LS fields or to derived values computed only from LS fields and public data. TST-030 checks this.

#### `ReviewFacts` (LLM input, [07](07-post-match-review-engine.md))

```json
{
  "match": { "match_id": 0, "result": "win|loss", "hero": "string", "duration_s": 0 },
  "accuracy": { "overall": 0, "lane": 0, "mid": 0, "late": 0 },
  "key_moments": [
    { "t_game_s": 0, "code": "D-DEATH", "tier": "T6", "delta_p": -0.0, "facts": { } }
  ],
  "fixes": [ { "rank": 1, "cause": "death|item|objective|farm", "epl_total": 0.0, "rule_ids": ["R-01"] } ],
  "phases": { "lane": { }, "mid": { }, "late": { } }
}
```

Numbers in LLM output are validated against this object ([07](07-post-match-review-engine.md)).

## Dependencies

- [04 Data sources](04-data-sources-and-ingestion.md)
- [06 In-match recommendations](06-in-match-recommendations.md)
- [07 Review](07-post-match-review-engine.md)
- [08 Builds](08-builds-import-and-builder.md)
- [10 Architecture](10-technical-architecture.md)
- [12 Programs](12-programs-and-code.md): PRG-23, PRG-24.

## Open Questions

- OQ-05: Will `assigned_lane` and `party_id` be available?
- [ASSUMPTION] Retention limits and note length (2,000 characters).

## Acceptance Criteria

- [ ] Migration 1 applies cleanly on an empty database and is idempotent (TST-105).
- [ ] Deleting a match cascades to its purchases, recommendations and Review.
- [ ] `OverlayMessage` payloads validate against the schema in every recorded test match (TST-030).
