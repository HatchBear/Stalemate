//! M-16 Storage (PRG-23). See 11-data-model.md.
//!
//! One SQLite connection behind a mutex. `Storage` is cheap to clone and safe to share between threads.
//! Migrations are numbered SQL files in `migrations/`, applied in order, each inside a transaction.

mod build_queries;
mod cache;
mod queries;

pub use cache::{cache_key, CacheRow};
pub use queries::{DataSource, GameMode, MatchPlayerRow, MatchResult, MatchRow, PlayerRow};

use rusqlite::Connection;
use std::{
    path::Path,
    sync::{Arc, Mutex, MutexGuard},
};

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("database error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("could not create the data directory: {0}")]
    Io(#[from] std::io::Error),
    #[error("database schema version {found} is newer than this build supports ({supported})")]
    NewerSchema { found: u32, supported: u32 },
    #[error("value out of range for the database: {0}")]
    OutOfRange(&'static str),
    #[error("bad value in the database: {0}")]
    BadValue(String),
    #[error("time formatting error: {0}")]
    Time(String),
}

pub type Result<T> = std::result::Result<T, StorageError>;

/// Numbered migrations, in order. Add new files here (T-015 adds the builds queries, not new tables).
const MIGRATIONS: &[(u32, &str)] = &[(1, include_str!("migrations/001.sql"))];

#[derive(Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    /// Open (or create) the database file, set the pragmas and run migrations.
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir)?;
            }
        }
        Self::init(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        // Pragmas cannot run inside the migration transaction, so they are set here.
        // `journal_mode` returns a row, so read it. In-memory databases answer "memory".
        conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get::<_, String>(0))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        migrate(&mut conn)?;
        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    pub(crate) fn lock(&self) -> MutexGuard<'_, Connection> {
        // A panic in another thread must not brick the database handle.
        self.conn.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Run a closure with the connection, for queries that have no typed helper yet.
    pub fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> rusqlite::Result<T>,
    ) -> Result<T> {
        Ok(f(&mut self.lock())?)
    }

    /// The highest migration applied (0 never happens after `open`).
    pub fn schema_version(&self) -> Result<u32> {
        current_version(&self.lock())
    }
}

fn current_version(conn: &Connection) -> Result<u32> {
    let has_table: bool = conn.query_row(
        "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'schema_version')",
        [],
        |r| r.get(0),
    )?;
    if !has_table {
        return Ok(0);
    }
    let v: Option<u32> =
        conn.query_row("SELECT MAX(version) FROM schema_version", [], |r| r.get(0))?;
    Ok(v.unwrap_or(0))
}

fn migrate(conn: &mut Connection) -> Result<()> {
    migrate_with(conn, MIGRATIONS)
}

/// Apply every migration newer than the stored version. Checking the version first makes this
/// idempotent on an empty database and on a populated one (TC-23-2, TST-105).
fn migrate_with(conn: &mut Connection, migrations: &[(u32, &str)]) -> Result<()> {
    let current = current_version(conn)?;
    let supported = migrations.last().map(|m| m.0).unwrap_or(0);
    if current > supported {
        return Err(StorageError::NewerSchema {
            found: current,
            supported,
        });
    }
    for (version, sql) in migrations.iter().filter(|m| m.0 > current) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        // Migration 1 inserts its own row; this keeps exactly one row for every migration.
        tx.execute("DELETE FROM schema_version", [])?;
        tx.execute("INSERT INTO schema_version VALUES (?1)", [version])?;
        tx.commit()?;
    }
    Ok(())
}

pub(crate) fn now_rfc3339() -> Result<String> {
    use time::format_description::well_known::Rfc3339;
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .map_err(|e| StorageError::Time(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tc_23_1_fresh_database_is_version_1() {
        let s = Storage::open_in_memory().unwrap();
        assert_eq!(s.schema_version().unwrap(), 1);
        let rows: i64 = s
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(rows, 1);
    }

    #[test]
    fn tc_23_2_migrations_twice_change_nothing() {
        let s = Storage::open_in_memory().unwrap();
        s.with_conn(|c| {
            c.execute(
                "INSERT INTO players (account_id, display_name, last_seen_at) VALUES (1, 'a', 'x')",
                [],
            )
        })
        .unwrap();
        s.with_conn(|c| Ok(migrate(c))).unwrap().unwrap();
        s.with_conn(|c| Ok(migrate(c))).unwrap().unwrap();
        assert_eq!(s.schema_version().unwrap(), 1);
        let n: i64 = s
            .with_conn(|c| c.query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(n, 1);
    }

    #[test]
    fn tst_105_reopening_a_populated_file_keeps_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sub").join("app.db");
        {
            let s = Storage::open(&path).unwrap();
            s.upsert_player(7, Some("Seven"), "2026-01-01T00:00:00Z")
                .unwrap();
        }
        let s = Storage::open(&path).unwrap();
        assert_eq!(s.schema_version().unwrap(), 1);
        assert_eq!(
            s.get_player(7).unwrap().unwrap().display_name.as_deref(),
            Some("Seven")
        );
    }

    #[test]
    fn tst_105_second_migration_applies_once_and_keeps_data() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS).unwrap();
        conn.execute(
            "INSERT INTO players (account_id, last_seen_at) VALUES (1, 'x')",
            [],
        )
        .unwrap();
        let more = [MIGRATIONS[0], (2, "CREATE TABLE extra (id INTEGER);")];
        migrate_with(&mut conn, &more).unwrap();
        migrate_with(&mut conn, &more).unwrap();
        assert_eq!(current_version(&conn).unwrap(), 2);
        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM players", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 1);
        let v: i64 = conn
            .query_row("SELECT COUNT(*) FROM schema_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(v, 1);
    }

    #[test]
    fn failed_migration_rolls_back() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS).unwrap();
        let bad = [
            MIGRATIONS[0],
            (2, "CREATE TABLE t (a); SELECT nope FROM nowhere;"),
        ];
        assert!(migrate_with(&mut conn, &bad).is_err());
        assert_eq!(current_version(&conn).unwrap(), 1);
        let t: bool = conn
            .query_row(
                "SELECT EXISTS (SELECT 1 FROM sqlite_master WHERE name = 't')",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(!t);
    }

    #[test]
    fn newer_database_is_refused() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate_with(&mut conn, MIGRATIONS).unwrap();
        conn.execute("UPDATE schema_version SET version = 9", [])
            .unwrap();
        assert!(matches!(
            migrate_with(&mut conn, MIGRATIONS),
            Err(StorageError::NewerSchema { found: 9, .. })
        ));
    }

    #[test]
    fn foreign_keys_are_on() {
        let s = Storage::open_in_memory().unwrap();
        let on: i64 = s
            .with_conn(|c| c.query_row("PRAGMA foreign_keys", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(on, 1);
    }

    #[test]
    fn file_database_uses_wal() {
        let dir = tempfile::tempdir().unwrap();
        let s = Storage::open(dir.path().join("app.db")).unwrap();
        let mode: String = s
            .with_conn(|c| c.query_row("PRAGMA journal_mode", [], |r| r.get(0)))
            .unwrap();
        assert_eq!(mode, "wal");
    }
}
