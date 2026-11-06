//! M-15 Build importers (PRG-19). In-game community builds over API-16 (FR-BUILD-001, OQ-07).
//!
//! Real shape of `GET /v1/builds` (2026-11-05): each entry is `hero_build` with `hero_id`,
//! `hero_build_id`, `name`, `version` and `details`. `details.mod_categories[]` are the sections
//! (`name`, `mods[]`); each mod is `{ ability_id }` where `ability_id` is the API-15 item `id`.
//! `details.ability_order.currency_changes[]` is the ability point sequence (`ability_id`).
//!
//! Items that can't be placed are listed in `unresolved`, never dropped silently (TC-19-2).

use std::{collections::HashSet, sync::Arc};

use async_trait::async_trait;
use url::Url;

use super::{Build, BuildSection, BuildSource};
use crate::{
    api_client::{ApiClient, AssetCatalog, CommunityBuild},
    types::ItemId,
};

/// Path of an in-game build URL: `<api base>/v1/builds?build_id=<id>`.
pub const IN_GAME_BUILD_PATH: &str = "/v1/builds";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkipReason {
    /// The ID is not in the current catalog.
    Unknown,
    /// In the catalog but disabled (removed by a patch).
    Removed,
    /// Already in the build; a build can hold an item once.
    Duplicate,
}

/// An item left out of the build, shown to the user to resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresolvedItem {
    pub section_title: String,
    /// The item's name when the catalog knows it, else the raw ID as text.
    pub name: String,
    pub id: Option<ItemId>,
    pub reason: SkipReason,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImportedBuild {
    pub build: Build,
    pub unresolved: Vec<UnresolvedItem>,
}

#[async_trait]
pub trait BuildImporter: Send + Sync {
    fn id(&self) -> &'static str;
    fn can_handle(&self, url: &Url) -> bool;
    /// Doc 12 shows this as sync; fetching needs the network, so it is async here.
    async fn import(&self, url: &Url) -> anyhow::Result<ImportedBuild>;
}

pub struct InGameCommunity {
    client: Arc<ApiClient>,
    catalog: Arc<AssetCatalog>,
}

impl InGameCommunity {
    pub fn new(client: Arc<ApiClient>, catalog: Arc<AssetCatalog>) -> Self {
        Self { client, catalog }
    }

    /// Import one community build by its ID. `None` when there is no such build.
    pub async fn import_build_id(
        &self,
        hero_build_id: u32,
    ) -> anyhow::Result<Option<ImportedBuild>> {
        Ok(self
            .client
            .community_build(hero_build_id)
            .await?
            .map(|b| convert(&b, &self.catalog)))
    }

    /// The top builds for a hero (`sort_by` as the API names it, such as `favorites`), converted.
    pub async fn top_for_hero(
        &self,
        hero_id: u32,
        sort_by: &str,
        limit: u32,
    ) -> anyhow::Result<Vec<ImportedBuild>> {
        Ok(self
            .client
            .community_builds(hero_id, sort_by, limit)
            .await?
            .iter()
            .map(|b| convert(b, &self.catalog))
            .collect())
    }

    fn build_id_of(&self, url: &Url) -> Option<u32> {
        let base = Url::parse(self.client.base_url()).ok()?;
        if url.host_str() != base.host_str()
            || url.port_or_known_default() != base.port_or_known_default()
        {
            return None;
        }
        if url.path() != IN_GAME_BUILD_PATH {
            return None;
        }
        url.query_pairs()
            .find(|(k, _)| k == "build_id")
            .and_then(|(_, v)| v.parse().ok())
    }
}

#[async_trait]
impl BuildImporter for InGameCommunity {
    fn id(&self) -> &'static str {
        "ingame-community"
    }

    fn can_handle(&self, url: &Url) -> bool {
        self.build_id_of(url).is_some()
    }

    async fn import(&self, url: &Url) -> anyhow::Result<ImportedBuild> {
        let id = self
            .build_id_of(url)
            .ok_or_else(|| anyhow::anyhow!("not an in-game build URL"))?;
        self.import_build_id(id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("no community build {id}"))
    }
}

/// Turn an API-16 entry into a [`Build`] against the current catalog. Pure, so it is tested
/// without a network.
pub fn convert(cb: &CommunityBuild, catalog: &AssetCatalog) -> ImportedBuild {
    let hb = &cb.hero_build;
    let mut seen: HashSet<ItemId> = HashSet::new();
    let mut sections = Vec::new();
    let mut unresolved = Vec::new();
    for (i, cat) in hb.details.mod_categories.iter().enumerate() {
        let title = if cat.name.trim().is_empty() {
            format!("Section {}", i + 1)
        } else {
            cat.name.trim().to_string()
        };
        let mut items = Vec::new();
        for m in &cat.mods {
            let id = m.ability_id;
            let skip = match catalog.item(id) {
                None => Some((SkipReason::Unknown, id.to_string())),
                Some(it) if it.disabled => Some((SkipReason::Removed, it.name.clone())),
                Some(it) if !seen.insert(id) => Some((SkipReason::Duplicate, it.name.clone())),
                Some(_) => None,
            };
            match skip {
                None => items.push(id),
                Some((reason, name)) => unresolved.push(UnresolvedItem {
                    section_title: title.clone(),
                    name,
                    id: Some(id),
                    reason,
                }),
            }
        }
        // A section with no placeable items can't be stored (build_items has one row per item).
        if !items.is_empty() {
            sections.push(BuildSection {
                title,
                item_ids: items,
            });
        }
    }
    let order: Vec<u32> = hb
        .details
        .ability_order
        .as_ref()
        .map(|a| a.currency_changes.iter().map(|c| c.ability_id).collect())
        .unwrap_or_default();
    let name = hb.name.trim();
    let mut build = Build::new_manual(
        hb.hero_id,
        if name.is_empty() {
            format!("Community build {}", hb.hero_build_id)
        } else {
            name.to_string()
        },
        catalog.patch(),
        sections,
        (!order.is_empty()).then_some(order),
    );
    build.source = BuildSource::Ingame;
    build.source_ref = Some(hb.hero_build_id.to_string());
    ImportedBuild { build, unresolved }
}
