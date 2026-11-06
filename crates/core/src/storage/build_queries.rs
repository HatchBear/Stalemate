//! Queries for `builds`, `build_items` and `active_build_plans` (doc 11, doc 08).
//!
//! `build_items` stores the section title on every item row, so a section with no items cannot be
//! stored. The builder rejects empty sections at save time (`builds::validate`).

use rusqlite::{params, OptionalExtension};

use super::{Result, Storage, StorageError};
use crate::builds::{Build, BuildSection, BuildSource};
use crate::types::HeroId;

const BUILD_COLS: &str = "build_id, source, source_ref, hero_id, name, patch, ability_order, \
     needs_update, created_at, updated_at";

fn load_items(conn: &rusqlite::Connection, build_id: &str) -> Result<Vec<BuildSection>> {
    let mut stmt = conn.prepare(
        "SELECT section_index, section_title, item_id FROM build_items
         WHERE build_id = ?1 ORDER BY section_index, position",
    )?;
    let mut rows = stmt.query([build_id])?;
    let mut sections: Vec<(i64, BuildSection)> = Vec::new();
    while let Some(r) = rows.next()? {
        let idx: i64 = r.get(0)?;
        let item: u32 = r.get(2)?;
        match sections.last_mut() {
            Some((i, s)) if *i == idx => s.item_ids.push(item),
            _ => sections.push((
                idx,
                BuildSection {
                    title: r.get(1)?,
                    item_ids: vec![item],
                },
            )),
        }
    }
    Ok(sections.into_iter().map(|(_, s)| s).collect())
}

fn build_from_row(conn: &rusqlite::Connection, r: &rusqlite::Row<'_>) -> Result<Build> {
    let build_id: String = r.get(0)?;
    let ability: Option<String> = r.get(6)?;
    Ok(Build {
        sections: load_items(conn, &build_id)?,
        source: BuildSource::parse(&r.get::<_, String>(1)?)
            .ok_or_else(|| StorageError::BadValue("builds.source".into()))?,
        source_ref: r.get(2)?,
        hero_id: r.get(3)?,
        name: r.get(4)?,
        patch: r.get(5)?,
        ability_order: ability
            .map(|s| serde_json::from_str(&s))
            .transpose()
            .map_err(|e| StorageError::BadValue(format!("builds.ability_order: {e}")))?,
        needs_update: r.get::<_, i64>(7)? != 0,
        created_at: r.get(8)?,
        updated_at: r.get(9)?,
        build_id,
    })
}

fn write_items(tx: &rusqlite::Transaction<'_>, b: &Build) -> Result<()> {
    let mut ins = tx.prepare(
        "INSERT INTO build_items (build_id, section_index, section_title, position, item_id)
         VALUES (?1, ?2, ?3, ?4, ?5)",
    )?;
    for (si, s) in b.sections.iter().enumerate() {
        for (pi, item) in s.item_ids.iter().enumerate() {
            ins.execute(params![b.build_id, si as i64, s.title, pi as i64, item])?;
        }
    }
    Ok(())
}

fn ability_json(b: &Build) -> Result<Option<String>> {
    b.ability_order
        .as_ref()
        .map(serde_json::to_string)
        .transpose()
        .map_err(|e| StorageError::BadValue(format!("ability_order: {e}")))
}

impl Storage {
    /// Insert a new build and its items in one transaction. Fails if the ID exists.
    pub fn insert_build(&self, b: &Build) -> Result<()> {
        let ability = ability_json(b)?;
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO builds (build_id, source, source_ref, hero_id, name, patch, ability_order,
               needs_update, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                b.build_id,
                b.source.as_str(),
                b.source_ref,
                b.hero_id,
                b.name,
                b.patch,
                ability,
                i64::from(b.needs_update),
                b.created_at,
                b.updated_at
            ],
        )?;
        write_items(&tx, b)?;
        tx.commit()?;
        Ok(())
    }

    /// Replace a build's fields and items. `created_at` is kept. Returns false if there is no such build.
    pub fn replace_build(&self, b: &Build) -> Result<bool> {
        let ability = ability_json(b)?;
        let mut conn = self.lock();
        let tx = conn.transaction()?;
        let n = tx.execute(
            "UPDATE builds SET source = ?2, source_ref = ?3, hero_id = ?4, name = ?5, patch = ?6,
               ability_order = ?7, needs_update = ?8, updated_at = ?9
             WHERE build_id = ?1",
            params![
                b.build_id,
                b.source.as_str(),
                b.source_ref,
                b.hero_id,
                b.name,
                b.patch,
                ability,
                i64::from(b.needs_update),
                b.updated_at
            ],
        )?;
        if n == 0 {
            return Ok(false);
        }
        tx.execute("DELETE FROM build_items WHERE build_id = ?1", [&b.build_id])?;
        write_items(&tx, b)?;
        // A plan pointing at this build must still match its hero.
        tx.execute(
            "DELETE FROM active_build_plans WHERE build_id = ?1 AND hero_id <> ?2",
            params![b.build_id, b.hero_id],
        )?;
        tx.commit()?;
        Ok(true)
    }

    pub fn get_build(&self, build_id: &str) -> Result<Option<Build>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {BUILD_COLS} FROM builds WHERE build_id = ?1"
        ))?;
        let mut rows = stmt.query([build_id])?;
        match rows.next()? {
            Some(r) => Ok(Some(build_from_row(&conn, r)?)),
            None => Ok(None),
        }
    }

    /// Builds for one hero, or all heroes, most recently updated first.
    pub fn list_builds(&self, hero_id: Option<HeroId>) -> Result<Vec<Build>> {
        let conn = self.lock();
        let mut stmt = conn.prepare(&format!(
            "SELECT {BUILD_COLS} FROM builds WHERE (?1 IS NULL OR hero_id = ?1)
             ORDER BY updated_at DESC, build_id"
        ))?;
        let mut rows = stmt.query([hero_id])?;
        let mut out = Vec::new();
        while let Some(r) = rows.next()? {
            out.push(build_from_row(&conn, r)?);
        }
        Ok(out)
    }

    /// Delete a build. Its items and any active plan go with it (cascade).
    pub fn delete_build(&self, build_id: &str) -> Result<bool> {
        Ok(self
            .lock()
            .execute("DELETE FROM builds WHERE build_id = ?1", [build_id])?
            > 0)
    }

    pub fn set_build_needs_update(&self, build_id: &str, needs_update: bool) -> Result<bool> {
        Ok(self.lock().execute(
            "UPDATE builds SET needs_update = ?2 WHERE build_id = ?1",
            params![build_id, i64::from(needs_update)],
        )? > 0)
    }

    /// Make a build the active plan for its hero (FR-BUILD-005). The build must be for `hero_id`.
    pub fn set_active_build(&self, hero_id: HeroId, build_id: &str) -> Result<()> {
        let conn = self.lock();
        let build_hero: Option<HeroId> = conn
            .query_row(
                "SELECT hero_id FROM builds WHERE build_id = ?1",
                [build_id],
                |r| r.get(0),
            )
            .optional()?;
        match build_hero {
            None => Err(StorageError::BadValue(format!("no build {build_id}"))),
            Some(h) if h != hero_id => Err(StorageError::BadValue(
                "the build is for a different hero".into(),
            )),
            Some(_) => {
                conn.execute(
                    "INSERT INTO active_build_plans (hero_id, build_id) VALUES (?1, ?2)
                     ON CONFLICT(hero_id) DO UPDATE SET build_id = excluded.build_id",
                    params![hero_id, build_id],
                )?;
                Ok(())
            }
        }
    }

    pub fn get_active_build_id(&self, hero_id: HeroId) -> Result<Option<String>> {
        Ok(self
            .lock()
            .query_row(
                "SELECT build_id FROM active_build_plans WHERE hero_id = ?1",
                [hero_id],
                |r| r.get(0),
            )
            .optional()?)
    }

    pub fn clear_active_build(&self, hero_id: HeroId) -> Result<bool> {
        Ok(self.lock().execute(
            "DELETE FROM active_build_plans WHERE hero_id = ?1",
            [hero_id],
        )? > 0)
    }

    /// Every hero's active build ID, ordered by hero.
    pub fn active_build_plans(&self) -> Result<Vec<(HeroId, String)>> {
        let conn = self.lock();
        let mut stmt =
            conn.prepare("SELECT hero_id, build_id FROM active_build_plans ORDER BY hero_id")?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }
}
