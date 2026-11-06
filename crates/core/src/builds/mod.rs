//! M-15 Build library and builder (PRG-18). See 08-builds-import-and-builder.md.
//!
//! FR-BUILD-003 (manual builder rules), FR-BUILD-005 (explicit active plan), FR-BUILD-006 (flag
//! builds whose items a patch removed).

mod importer;

use serde::{Deserialize, Serialize};
use time::format_description::well_known::Rfc3339;

pub use importer::{
    BuildImporter, ImportedBuild, InGameCommunity, SkipReason, UnresolvedItem, IN_GAME_BUILD_PATH,
};

use crate::{
    api_client::AssetCatalog,
    storage::{Storage, StorageError},
    types::{HeroId, ItemId},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BuildSource {
    Ingame,
    External,
    Manual,
}

impl BuildSource {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Ingame => "ingame",
            Self::External => "external",
            Self::Manual => "manual",
        }
    }

    pub(crate) fn parse(s: &str) -> Option<Self> {
        match s {
            "ingame" => Some(Self::Ingame),
            "external" => Some(Self::External),
            "manual" => Some(Self::Manual),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BuildSection {
    pub title: String,
    pub item_ids: Vec<ItemId>,
}

/// The doc 08 build model. `needs_update` is stored but not exported: it describes this machine's
/// patch state, not the build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Build {
    /// UUID.
    pub build_id: String,
    pub source: BuildSource,
    /// Community build ID or URL, or `None`.
    pub source_ref: Option<String>,
    pub hero_id: HeroId,
    pub name: String,
    /// The catalog patch at import time.
    pub patch: String,
    /// Sections keep the order the source used.
    pub sections: Vec<BuildSection>,
    #[serde(default)]
    pub ability_order: Option<Vec<u32>>,
    #[serde(skip)]
    pub needs_update: bool,
    /// RFC 3339.
    pub created_at: String,
    /// RFC 3339.
    pub updated_at: String,
}

#[derive(Debug, thiserror::Error)]
pub enum BuildError {
    #[error("the build needs a name")]
    EmptyName,
    #[error("hero {0} is not in the current patch")]
    UnknownHero(HeroId),
    #[error("section {0} has no items")]
    EmptySection(usize),
    #[error("duplicate items: {0:?}")]
    DuplicateItems(Vec<ItemId>),
    #[error("items not in the current patch: {0:?}")]
    UnknownItems(Vec<ItemId>),
    #[error("no such build")]
    NotFound,
    #[error("this build is for a different hero")]
    WrongHero,
    #[error("a build with this ID already exists")]
    AlreadyExists,
    #[error("not a valid build file: {0}")]
    Json(#[from] serde_json::Error),
    #[error("the build ID is not a UUID")]
    BadId,
    #[error(transparent)]
    Storage(#[from] StorageError),
}

pub type Result<T> = std::result::Result<T, BuildError>;

fn now() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

impl Build {
    /// A new manual build with a fresh ID and timestamps. Not validated or stored.
    pub fn new_manual(
        hero_id: HeroId,
        name: impl Into<String>,
        patch: impl Into<String>,
        sections: Vec<BuildSection>,
        ability_order: Option<Vec<u32>>,
    ) -> Self {
        let t = now();
        Self {
            build_id: uuid::Uuid::new_v4().to_string(),
            source: BuildSource::Manual,
            source_ref: None,
            hero_id,
            name: name.into(),
            patch: patch.into(),
            sections,
            ability_order,
            needs_update: false,
            created_at: t.clone(),
            updated_at: t,
        }
    }

    /// Every item in section order.
    pub fn all_items(&self) -> impl Iterator<Item = ItemId> + '_ {
        self.sections
            .iter()
            .flat_map(|s| s.item_ids.iter().copied())
    }

    /// Items the catalog no longer has, or has disabled (FR-BUILD-006: "Needs update: N items removed").
    pub fn removed_items(&self, catalog: &AssetCatalog) -> Vec<ItemId> {
        self.all_items()
            .filter(|i| !catalog.is_item_available(*i))
            .collect()
    }
}

/// Builder rules (FR-BUILD-003, TC-18-1): a name, a known hero, no empty sections, no duplicate
/// items anywhere in the build, and every item present in the current catalog.
pub fn validate(b: &Build, catalog: &AssetCatalog) -> Result<()> {
    if b.name.trim().is_empty() {
        return Err(BuildError::EmptyName);
    }
    if catalog.hero(b.hero_id).is_none() {
        return Err(BuildError::UnknownHero(b.hero_id));
    }
    if let Some(i) = b.sections.iter().position(|s| s.item_ids.is_empty()) {
        return Err(BuildError::EmptySection(i));
    }
    let mut seen = std::collections::HashSet::new();
    let mut dups: Vec<ItemId> = Vec::new();
    for i in b.all_items() {
        if !seen.insert(i) && !dups.contains(&i) {
            dups.push(i);
        }
    }
    if !dups.is_empty() {
        return Err(BuildError::DuplicateItems(dups));
    }
    let unknown = b.removed_items(catalog);
    if !unknown.is_empty() {
        return Err(BuildError::UnknownItems(unknown));
    }
    Ok(())
}

/// Export as JSON in the doc 08 model. Keys are sorted and the layout is fixed, so exporting the
/// same build always gives the same bytes (TC-18-2).
pub fn export_json(b: &Build) -> Result<String> {
    // `Value` keeps its keys sorted, which is the normal form.
    Ok(serde_json::to_string_pretty(&serde_json::to_value(b)?)?)
}

/// Normal form of a build file: parsed, keys sorted, pretty-printed. Whitespace and key order in
/// a hand-edited file don't matter.
pub fn normalize_json(text: &str) -> Result<String> {
    let v: serde_json::Value = serde_json::from_str(text)?;
    Ok(serde_json::to_string_pretty(&v)?)
}

/// Parse a build file and validate it against the current catalog. Not stored.
pub fn import_json(text: &str, catalog: &AssetCatalog) -> Result<Build> {
    let b: Build = serde_json::from_str(text)?;
    uuid::Uuid::parse_str(&b.build_id).map_err(|_| BuildError::BadId)?;
    validate(&b, catalog)?;
    Ok(b)
}

/// One build flagged by a patch check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Revalidated {
    pub build_id: String,
    pub name: String,
    pub removed: Vec<ItemId>,
}

/// The build library over `builds`, `build_items` and `active_build_plans`.
#[derive(Clone)]
pub struct BuildLibrary {
    db: Storage,
}

impl BuildLibrary {
    pub fn new(db: Storage) -> Self {
        Self { db }
    }

    /// Validate and store a new build (a manual build, or the result of an import).
    pub fn add(&self, mut b: Build, catalog: &AssetCatalog) -> Result<Build> {
        validate(&b, catalog)?;
        if self.db.get_build(&b.build_id)?.is_some() {
            return Err(BuildError::AlreadyExists);
        }
        b.needs_update = false;
        self.db.insert_build(&b)?;
        Ok(b)
    }

    /// Validate and save changes to a stored build. A build that passes validation is current, so
    /// `needs_update` clears.
    pub fn update(&self, mut b: Build, catalog: &AssetCatalog) -> Result<Build> {
        validate(&b, catalog)?;
        b.needs_update = false;
        b.updated_at = now();
        if !self.db.replace_build(&b)? {
            return Err(BuildError::NotFound);
        }
        Ok(b)
    }

    pub fn get(&self, build_id: &str) -> Result<Option<Build>> {
        Ok(self.db.get_build(build_id)?)
    }

    pub fn list(&self, hero_id: Option<HeroId>) -> Result<Vec<Build>> {
        Ok(self.db.list_builds(hero_id)?)
    }

    pub fn delete(&self, build_id: &str) -> Result<bool> {
        Ok(self.db.delete_build(build_id)?)
    }

    /// Import a build file. The build keeps its ID unless that ID is already stored, in which case
    /// it gets a new one (a second copy, not an overwrite).
    pub fn import_json(&self, text: &str, catalog: &AssetCatalog) -> Result<Build> {
        let mut b = import_json(text, catalog)?;
        if self.db.get_build(&b.build_id)?.is_some() {
            b.build_id = uuid::Uuid::new_v4().to_string();
        }
        self.add(b, catalog)
    }

    pub fn export_json(&self, build_id: &str) -> Result<String> {
        export_json(&self.get(build_id)?.ok_or(BuildError::NotFound)?)
    }

    /// FR-BUILD-006: after a patch change, check every build against the new catalog. A build with
    /// removed items gets `needs_update = 1`; one with none gets it cleared. Items stay in the
    /// build. Returns the builds that need an update, with the removed items for the details view.
    pub fn revalidate_all(&self, catalog: &AssetCatalog) -> Result<Vec<Revalidated>> {
        let mut flagged = Vec::new();
        for b in self.db.list_builds(None)? {
            let removed = b.removed_items(catalog);
            let needs = !removed.is_empty();
            if needs != b.needs_update {
                self.db.set_build_needs_update(&b.build_id, needs)?;
            }
            if needs {
                flagged.push(Revalidated {
                    build_id: b.build_id,
                    name: b.name,
                    removed,
                });
            }
        }
        Ok(flagged)
    }

    // -----------------------------------------------------------------------------------------
    // Active plan (FR-BUILD-005)
    // -----------------------------------------------------------------------------------------

    /// The user's pick for a hero in the app. Works during a match too. The build must be for
    /// that hero.
    pub fn set_active(&self, hero_id: HeroId, build_id: &str) -> Result<()> {
        let b = self.get(build_id)?.ok_or(BuildError::NotFound)?;
        if b.hero_id != hero_id {
            return Err(BuildError::WrongHero);
        }
        Ok(self.db.set_active_build(hero_id, build_id)?)
    }

    pub fn clear_active(&self, hero_id: HeroId) -> Result<bool> {
        Ok(self.db.clear_active_build(hero_id)?)
    }

    /// The doc 08 priority table with no in-game input: the app pick, else `None`.
    pub fn active_plan(&self, hero_id: HeroId) -> Result<Option<Build>> {
        self.active_plan_with(hero_id, None)
    }

    /// The doc 08 priority table:
    /// 1. The build the user marked active for this hero in the app. It always wins (DEC-026).
    /// 2. `ingame_selected`: the build the user chose in-game, read from the Broadcast at game
    ///    start (FR-BUILD-004, Beta). Only used when there is no app pick for this hero.
    /// 3. `None`: Build Check is off.
    ///
    /// A pick that points at a deleted build, or at one for another hero, counts as no pick.
    pub fn active_plan_with(
        &self,
        hero_id: HeroId,
        ingame_selected: Option<Build>,
    ) -> Result<Option<Build>> {
        if let Some(id) = self.db.get_active_build_id(hero_id)? {
            if let Some(b) = self.db.get_build(&id)?.filter(|b| b.hero_id == hero_id) {
                return Ok(Some(b));
            }
        }
        Ok(ingame_selected.filter(|b| b.hero_id == hero_id))
    }
}

#[cfg(test)]
mod tests;
