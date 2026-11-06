//! Game data from API-15 (FR-DATA-008, NFR-MAINT-001): heroes and items, loaded at runtime and
//! cached per patch. Nothing here is a hard-coded list. Hero and item art is downloaded at runtime
//! into `<data>/assets/<patch>/` and never bundled (doc 13).
//!
//! Real `/v1/assets/items` shape, checked 2026-11-05: every item has a numeric `id` and a
//! `class_name`. The `id` equals MurmurHash2 (seed `0x31415926`) of the case-sensitive
//! `class_name` for all 782 items, which is the 32-bit string-token hash the Broadcast uses.
//! `component_items` lists class names, not ids. `cost`, `item_tier` and `item_slot_type` exist
//! only on `type == "upgrade"` items (the shop items).

use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    purchase_log::ItemTree,
    types::{HeroId, ItemId},
};

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct HeroAsset {
    pub id: HeroId,
    pub class_name: String,
    pub name: String,
    pub player_selectable: bool,
    pub disabled: bool,
    pub in_development: bool,
    /// Hero card art URL (png), loaded at runtime.
    pub card_image: Option<String>,
    /// Small icon URL (png).
    pub small_image: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub struct ItemAsset {
    /// The numeric id, MurmurHash2 of `class_name`. Fits `u32`; 371 of 782 exceed `i32::MAX`.
    pub id: ItemId,
    pub class_name: String,
    pub name: String,
    /// `"upgrade"` (shop items), `"ability"` or `"weapon"`.
    pub kind: String,
    /// Souls. 0 for items without a price.
    pub cost: u32,
    pub tier: Option<u32>,
    /// `"weapon"`, `"armor"` or `"tech"` on shop items.
    pub slot: Option<String>,
    pub shopable: bool,
    /// Disabled items stay in the data after a removal. Treated as not available.
    pub disabled: bool,
    /// Class names of the direct components.
    pub component_items: Vec<String>,
    pub image: Option<String>,
    pub shop_image: Option<String>,
}

/// What is stored in `api_cache` for a patch. Smaller than the raw responses (8 MB).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
pub(crate) struct CatalogData {
    pub patch: String,
    pub heroes: Vec<HeroAsset>,
    pub items: Vec<ItemAsset>,
}

/// Heroes and items for one patch.
#[derive(Debug, Clone)]
pub struct AssetCatalog {
    patch: String,
    heroes: Vec<HeroAsset>,
    items: Vec<ItemAsset>,
    item_index: HashMap<ItemId, usize>,
    class_index: HashMap<String, ItemId>,
}

impl AssetCatalog {
    pub fn from_parts(
        patch: impl Into<String>,
        heroes: Vec<HeroAsset>,
        items: Vec<ItemAsset>,
    ) -> Self {
        let item_index = items.iter().enumerate().map(|(i, it)| (it.id, i)).collect();
        let class_index = items
            .iter()
            .map(|it| (it.class_name.clone(), it.id))
            .collect();
        Self {
            patch: patch.into(),
            heroes,
            items,
            item_index,
            class_index,
        }
    }

    pub(crate) fn from_data(d: CatalogData) -> Self {
        Self::from_parts(d.patch, d.heroes, d.items)
    }

    pub(crate) fn to_data(&self) -> CatalogData {
        CatalogData {
            patch: self.patch.clone(),
            heroes: self.heroes.clone(),
            items: self.items.clone(),
        }
    }

    /// The patch key: the newest client version at load time.
    pub fn patch(&self) -> &str {
        &self.patch
    }

    pub fn heroes(&self) -> &[HeroAsset] {
        &self.heroes
    }

    pub fn items(&self) -> &[ItemAsset] {
        &self.items
    }

    pub fn hero(&self, id: HeroId) -> Option<&HeroAsset> {
        self.heroes.iter().find(|h| h.id == id)
    }

    pub fn item(&self, id: ItemId) -> Option<&ItemAsset> {
        self.item_index.get(&id).map(|&i| &self.items[i])
    }

    pub fn item_by_class(&self, class_name: &str) -> Option<&ItemAsset> {
        self.class_index
            .get(class_name)
            .and_then(|&id| self.item(id))
    }

    /// Item names are not unique across languages or abilities, so this matches the display name
    /// exactly (case-insensitive) among shop items only. Used by importers that have names.
    pub fn shop_item_by_name(&self, name: &str) -> Option<&ItemAsset> {
        self.items
            .iter()
            .find(|i| i.kind == "upgrade" && i.name.eq_ignore_ascii_case(name))
    }

    /// In the current data and not disabled.
    pub fn is_item_available(&self, id: ItemId) -> bool {
        self.item(id).is_some_and(|i| !i.disabled)
    }

    fn components_of(&self, item: ItemId) -> Vec<ItemId> {
        self.item(item)
            .map(|i| {
                i.component_items
                    .iter()
                    .filter_map(|c| self.class_index.get(c).copied())
                    .collect()
            })
            .unwrap_or_default()
    }
}

/// A component is anything in the item's build tree, not only the direct parts: a tier 3 item
/// built from a tier 2 item consumes the tier 1 item inside it too.
impl ItemTree for AssetCatalog {
    fn is_component_of(&self, component: ItemId, item: ItemId) -> bool {
        let mut seen = HashSet::new();
        let mut stack = self.components_of(item);
        while let Some(c) = stack.pop() {
            if c == component {
                return true;
            }
            if seen.insert(c) {
                stack.extend(self.components_of(c));
            }
        }
        false
    }

    fn cost(&self, item: ItemId) -> u32 {
        self.item(item).map_or(0, |i| i.cost)
    }
}

// ---------------------------------------------------------------------------------------------
// Parsing the raw API-15 responses
// ---------------------------------------------------------------------------------------------

#[derive(Deserialize)]
struct RawHero {
    id: HeroId,
    class_name: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    player_selectable: bool,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    in_development: bool,
    #[serde(default)]
    images: RawHeroImages,
}

#[derive(Deserialize, Default)]
struct RawHeroImages {
    #[serde(default)]
    icon_hero_card: Option<String>,
    #[serde(default)]
    icon_image_small: Option<String>,
}

#[derive(Deserialize)]
struct RawItem {
    id: ItemId,
    class_name: String,
    #[serde(default)]
    name: String,
    #[serde(default, rename = "type")]
    kind: String,
    #[serde(default)]
    cost: Option<u32>,
    #[serde(default)]
    item_tier: Option<u32>,
    #[serde(default)]
    item_slot_type: Option<String>,
    #[serde(default)]
    shopable: Option<bool>,
    #[serde(default)]
    disabled: Option<bool>,
    #[serde(default)]
    component_items: Option<Vec<String>>,
    #[serde(default)]
    image: Option<String>,
    #[serde(default)]
    shop_image: Option<String>,
}

pub(crate) fn parse_heroes(body: &[u8]) -> Result<Vec<HeroAsset>, serde_json::Error> {
    let raw: Vec<RawHero> = serde_json::from_slice(body)?;
    Ok(raw
        .into_iter()
        .map(|h| HeroAsset {
            id: h.id,
            class_name: h.class_name,
            name: h.name,
            player_selectable: h.player_selectable,
            disabled: h.disabled,
            in_development: h.in_development,
            card_image: h.images.icon_hero_card,
            small_image: h.images.icon_image_small,
        })
        .collect())
}

pub(crate) fn parse_items(body: &[u8]) -> Result<Vec<ItemAsset>, serde_json::Error> {
    let raw: Vec<RawItem> = serde_json::from_slice(body)?;
    Ok(raw
        .into_iter()
        .map(|i| ItemAsset {
            id: i.id,
            class_name: i.class_name,
            name: i.name,
            kind: i.kind,
            cost: i.cost.unwrap_or(0),
            tier: i.item_tier,
            slot: i.item_slot_type,
            shopable: i.shopable.unwrap_or(false),
            disabled: i.disabled.unwrap_or(false),
            component_items: i.component_items.unwrap_or_default(),
            image: i.image,
            shop_image: i.shop_image,
        })
        .collect())
}

// ---------------------------------------------------------------------------------------------
// Image files
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    HeroCard,
    HeroSmall,
    Item,
}

impl ImageKind {
    fn dir(self) -> &'static str {
        match self {
            Self::HeroCard => "heroes/card",
            Self::HeroSmall => "heroes/small",
            Self::Item => "items",
        }
    }
}

/// Where art for a patch lives: `<data>/assets/<patch>/`.
#[derive(Debug, Clone)]
pub struct AssetDirs {
    root: PathBuf,
}

impl AssetDirs {
    /// `data_dir` is the app data directory; `assets/` is added here.
    pub fn new(data_dir: &Path) -> Self {
        Self {
            root: data_dir.join("assets"),
        }
    }

    /// The patch comes from the API; keep only safe characters so it can't escape the folder.
    pub fn patch_dir(&self, patch: &str) -> PathBuf {
        let safe: String = patch
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '.' || c == '-' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let only_dots = safe.chars().all(|c| c == '.');
        self.root
            .join(if only_dots { "unknown".into() } else { safe })
    }

    pub fn image_path(&self, patch: &str, kind: ImageKind, id: u32, url: &str) -> PathBuf {
        let ext = url
            .split(['?', '#'])
            .next()
            .and_then(|u| u.rsplit('.').next())
            .filter(|e| matches!(*e, "png" | "webp" | "jpg" | "jpeg" | "svg"))
            .unwrap_or("png");
        self.patch_dir(patch)
            .join(kind.dir())
            .join(format!("{id}.{ext}"))
    }

    /// Delete folders of other patches (retention: art is re-downloadable).
    pub fn remove_other_patches(&self, keep: &str) -> std::io::Result<usize> {
        let keep_dir = self.patch_dir(keep);
        let mut n = 0;
        let Ok(rd) = std::fs::read_dir(&self.root) else {
            return Ok(0);
        };
        for e in rd.flatten() {
            if e.path() != keep_dir && e.path().is_dir() {
                std::fs::remove_dir_all(e.path())?;
                n += 1;
            }
        }
        Ok(n)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(crate) fn item(id: u32, class: &str, cost: u32, comps: &[&str]) -> ItemAsset {
        ItemAsset {
            id,
            class_name: class.into(),
            name: class.to_uppercase(),
            kind: "upgrade".into(),
            cost,
            tier: Some(1),
            slot: Some("weapon".into()),
            shopable: true,
            disabled: false,
            component_items: comps.iter().map(|s| s.to_string()).collect(),
            image: None,
            shop_image: None,
        }
    }

    #[test]
    fn item_tree_is_transitive_and_has_costs() {
        let cat = AssetCatalog::from_parts(
            "1",
            vec![],
            vec![
                item(1, "t1", 500, &[]),
                item(2, "t2", 1250, &["t1"]),
                item(3, "t3", 3000, &["t2"]),
                item(4, "other", 800, &[]),
            ],
        );
        assert!(cat.is_component_of(2, 3));
        assert!(cat.is_component_of(1, 2));
        assert!(cat.is_component_of(1, 3), "tier 1 sits inside tier 3");
        assert!(!cat.is_component_of(3, 1));
        assert!(!cat.is_component_of(4, 3));
        assert!(!cat.is_component_of(1, 1));
        assert_eq!(cat.cost(3), 3000);
        assert_eq!(cat.cost(999), 0);
    }

    #[test]
    fn component_loops_do_not_hang() {
        let cat = AssetCatalog::from_parts(
            "1",
            vec![],
            vec![
                item(1, "a", 1, &["b"]),
                item(2, "b", 1, &["a"]),
                item(3, "c", 1, &[]),
            ],
        );
        assert!(!cat.is_component_of(3, 1));
    }

    #[test]
    fn parses_synthetic_api_15_shapes() {
        let heroes = br#"[{"id":7,"class_name":"hero_x","name":"X","player_selectable":true,
            "disabled":false,"in_development":false,"unknown_new_field":{"a":1},
            "images":{"icon_hero_card":"https://cdn.example/x_card.png","other":"y"}}]"#;
        let h = parse_heroes(heroes).unwrap();
        assert_eq!(h[0].id, 7);
        assert_eq!(
            h[0].card_image.as_deref(),
            Some("https://cdn.example/x_card.png")
        );
        let items = br#"[{"id":4284855775,"class_name":"upgrade_big","name":"Big","type":"upgrade",
            "cost":3200,"item_tier":3,"item_slot_type":"tech","shopable":true,
            "component_items":["upgrade_small"]},
            {"id":5,"class_name":"ability_q","name":"Q","type":"ability"}]"#;
        let i = parse_items(items).unwrap();
        assert_eq!(i[0].id, 4_284_855_775, "ids above i32::MAX survive");
        assert_eq!(i[0].cost, 3200);
        assert_eq!(i[1].cost, 0);
        assert!(!i[1].shopable);
    }

    #[test]
    fn availability_and_lookup() {
        let mut gone = item(2, "gone", 1, &[]);
        gone.disabled = true;
        let cat = AssetCatalog::from_parts("1", vec![], vec![item(1, "here", 1, &[]), gone]);
        assert!(cat.is_item_available(1));
        assert!(!cat.is_item_available(2), "disabled counts as removed");
        assert!(!cat.is_item_available(3));
        assert_eq!(cat.item_by_class("here").unwrap().id, 1);
        assert_eq!(cat.shop_item_by_name("HERE").unwrap().id, 1);
    }

    #[test]
    fn image_paths_stay_inside_the_patch_folder() {
        let d = AssetDirs::new(Path::new("/data"));
        let p = d.image_path("../../etc", ImageKind::Item, 5, "https://x/y/a.webp?v=2");
        assert!(p.starts_with("/data/assets/"));
        assert!(!p
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)));
        assert!(!d
            .patch_dir("..")
            .components()
            .any(|c| matches!(c, std::path::Component::ParentDir)));
        assert!(p.to_string_lossy().ends_with("/items/5.webp"));
        assert!(d
            .image_path("1", ImageKind::HeroCard, 1, "https://x/noext")
            .to_string_lossy()
            .ends_with("/heroes/card/1.png"));
    }
}
