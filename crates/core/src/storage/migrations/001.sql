-- Migration 1: the doc 11 schema (version 1), verbatim except for the first two lines.
-- PRAGMA journal_mode = WAL and PRAGMA foreign_keys = ON are set on open in storage/mod.rs:
-- WAL cannot be changed inside a transaction, and migrations run inside one.

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
  assigned_lane  INTEGER,                  -- nullable until OQ-05 resolved
  party_id       INTEGER,                  -- nullable until OQ-05 resolved
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
