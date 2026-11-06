//! PRG-18 and PRG-19 tests: TC-18-1 to TC-18-3, TC-19-1, TC-19-2, TST-080, TST-082, TST-083,
//! TST-084. All data is synthetic.

use std::{path::Path, sync::Arc};

use serde_json::json;
use url::Url;
use wiremock::{
    matchers::{method, path, query_param},
    Mock, MockServer, ResponseTemplate,
};

use super::*;
use crate::{
    api_client::{ApiClient, ApiConfig, CommunityBuild, HeroAsset, ItemAsset},
    builds::importer::convert,
    settings::Rules,
};

fn hero(id: u32) -> HeroAsset {
    HeroAsset {
        id,
        class_name: format!("hero_{id}"),
        name: format!("Hero {id}"),
        player_selectable: true,
        disabled: false,
        in_development: false,
        card_image: None,
        small_image: None,
    }
}

fn item(id: u32, disabled: bool) -> ItemAsset {
    ItemAsset {
        id,
        class_name: format!("item_{id}"),
        name: format!("Item {id}"),
        kind: "upgrade".into(),
        cost: 500,
        tier: Some(1),
        slot: Some("weapon".into()),
        shopable: true,
        disabled,
        component_items: vec![],
        image: None,
        shop_image: None,
    }
}

/// Heroes 1 to 5 and items 1 to 40.
fn catalog(patch: &str) -> AssetCatalog {
    AssetCatalog::from_parts(
        patch,
        (1..=5).map(hero).collect(),
        (1..=40).map(|i| item(i, false)).collect(),
    )
}

fn sections(spec: &[(&str, &[u32])]) -> Vec<BuildSection> {
    spec.iter()
        .map(|(t, ids)| BuildSection {
            title: (*t).into(),
            item_ids: ids.to_vec(),
        })
        .collect()
}

fn manual(hero_id: u32, name: &str, secs: &[(&str, &[u32])]) -> Build {
    Build::new_manual(hero_id, name, "100", sections(secs), None)
}

fn lib() -> BuildLibrary {
    BuildLibrary::new(Storage::open_in_memory().unwrap())
}

#[test]
fn tc_18_1_adding_the_same_item_twice_is_rejected() {
    let cat = catalog("100");
    let l = lib();
    let same_section = manual(1, "dup", &[("Lane", &[1, 2, 2])]);
    assert!(matches!(
        l.add(same_section, &cat),
        Err(BuildError::DuplicateItems(d)) if d == vec![2]
    ));
    let across = manual(1, "dup2", &[("Lane", &[1, 2]), ("Late", &[3, 1])]);
    assert!(matches!(l.add(across, &cat), Err(BuildError::DuplicateItems(d)) if d == vec![1]));
    assert!(l.list(None).unwrap().is_empty(), "nothing was stored");
}

#[test]
fn builder_rules_unknown_item_hero_name_and_empty_section() {
    let cat = catalog("100");
    let l = lib();
    assert!(matches!(
        l.add(manual(1, "x", &[("a", &[1, 999])]), &cat),
        Err(BuildError::UnknownItems(u)) if u == vec![999]
    ));
    assert!(matches!(
        l.add(manual(77, "x", &[("a", &[1])]), &cat),
        Err(BuildError::UnknownHero(77))
    ));
    assert!(matches!(
        l.add(manual(1, "  ", &[("a", &[1])]), &cat),
        Err(BuildError::EmptyName)
    ));
    assert!(matches!(
        l.add(manual(1, "x", &[("a", &[1]), ("b", &[])]), &cat),
        Err(BuildError::EmptySection(1))
    ));
    // A disabled item counts as gone.
    let mut parts: Vec<_> = (1..=40).map(|i| item(i, i == 5)).collect();
    parts.truncate(40);
    let cat2 = AssetCatalog::from_parts("101", vec![hero(1)], parts);
    assert!(l.add(manual(1, "x", &[("a", &[5])]), &cat2).is_err());
}

#[test]
fn crud_keeps_section_and_item_order() {
    let cat = catalog("100");
    let l = lib();
    let mut b = manual(2, "Order", &[("Lane", &[9, 3, 7]), ("Late", &[20, 1])]);
    b.ability_order = Some(vec![5, 6, 5, 7]);
    let b = l.add(b, &cat).unwrap();
    let got = l.get(&b.build_id).unwrap().unwrap();
    assert_eq!(got, b);
    assert_eq!(got.sections[0].item_ids, vec![9, 3, 7]);

    let mut edit = got.clone();
    edit.name = "Renamed".into();
    edit.sections = sections(&[("Only", &[4, 2])]);
    let edit = l.update(edit, &cat).unwrap();
    let got = l.get(&b.build_id).unwrap().unwrap();
    assert_eq!(got.name, "Renamed");
    assert_eq!(got.sections.len(), 1);
    assert_eq!(got.created_at, b.created_at);
    assert_eq!(got, edit);

    assert!(matches!(
        l.update(manual(2, "ghost", &[("a", &[1])]), &cat),
        Err(BuildError::NotFound)
    ));
    assert_eq!(l.list(Some(2)).unwrap().len(), 1);
    assert!(l.list(Some(3)).unwrap().is_empty());
    assert!(l.delete(&b.build_id).unwrap());
    assert!(l.get(&b.build_id).unwrap().is_none());
    // The items went with it.
    let rows: i64 =
        l.db.with_conn(|c| c.query_row("SELECT COUNT(*) FROM build_items", [], |r| r.get(0)))
            .unwrap();
    assert_eq!(rows, 0);
}

#[test]
fn tc_18_2_export_then_import_is_byte_identical_after_normalization() {
    let cat = catalog("100");
    let l = lib();
    let mut b = manual(1, "Round trip", &[("Lane", &[1, 2, 3]), ("Late", &[4])]);
    b.ability_order = Some(vec![10, 11]);
    b.source_ref = Some("abc".into());
    let b = l.add(b, &cat).unwrap();

    let first = l.export_json(&b.build_id).unwrap();
    // Import into a second library (a fresh machine) and export again.
    let other = lib();
    let imported = other.import_json(&first, &cat).unwrap();
    assert_eq!(
        imported.build_id, b.build_id,
        "keeps its ID on an empty library"
    );
    let second = other.export_json(&imported.build_id).unwrap();
    assert_eq!(first, second);
    assert_eq!(normalize_json(&first).unwrap(), first);
    // Key order and whitespace in a hand-edited file normalize to the same text.
    let v: serde_json::Value = serde_json::from_str(&first).unwrap();
    let compact = serde_json::to_string(&v).unwrap();
    assert_eq!(normalize_json(&compact).unwrap(), first);
    // TST-082: the model fields survive too.
    assert_eq!(imported.sections, b.sections);
    assert_eq!(imported.ability_order, b.ability_order);
    assert_eq!(imported.name, b.name);
}

#[test]
fn import_into_a_library_that_has_the_id_makes_a_copy() {
    let cat = catalog("100");
    let l = lib();
    let b = l.add(manual(1, "Mine", &[("a", &[1])]), &cat).unwrap();
    let copy = l
        .import_json(&l.export_json(&b.build_id).unwrap(), &cat)
        .unwrap();
    assert_ne!(copy.build_id, b.build_id);
    assert_eq!(l.list(None).unwrap().len(), 2);
}

#[test]
fn import_rejects_bad_files() {
    let cat = catalog("100");
    assert!(matches!(
        import_json("not json", &cat),
        Err(BuildError::Json(_))
    ));
    let l = lib();
    let b = l.add(manual(1, "x", &[("a", &[1])]), &cat).unwrap();
    let text = export_json(&b).unwrap().replace(&b.build_id, "not-a-uuid");
    assert!(matches!(import_json(&text, &cat), Err(BuildError::BadId)));
    let dup = export_json(&b).unwrap().replace(
        "\"item_ids\": [\n        1\n      ]",
        "\"item_ids\": [1, 1]",
    );
    assert!(matches!(
        import_json(&dup, &cat),
        Err(BuildError::DuplicateItems(_))
    ));
}

#[test]
fn tc_18_3_item_removed_in_a_new_patch_sets_needs_update() {
    let l = lib();
    let old = catalog("100");
    let keep = l.add(manual(1, "Keeps", &[("a", &[1, 2])]), &old).unwrap();
    let hit = l
        .add(manual(1, "Loses", &[("a", &[3, 4, 5])]), &old)
        .unwrap();
    assert!(!hit.needs_update);

    // Patch 101 drops item 4 and disables item 5.
    let items: Vec<_> = (1..=40)
        .filter(|i| *i != 4)
        .map(|i| item(i, i == 5))
        .collect();
    let new = AssetCatalog::from_parts("101", (1..=5).map(hero).collect(), items);
    let flagged = l.revalidate_all(&new).unwrap();
    assert_eq!(flagged.len(), 1);
    assert_eq!(flagged[0].build_id, hit.build_id);
    assert_eq!(
        flagged[0].removed,
        vec![4, 5],
        "N items removed, for the details view"
    );

    let hit_now = l.get(&hit.build_id).unwrap().unwrap();
    assert!(hit_now.needs_update);
    assert_eq!(
        hit_now.sections[0].item_ids,
        vec![3, 4, 5],
        "items stay in the build"
    );
    assert!(!l.get(&keep.build_id).unwrap().unwrap().needs_update);

    // TST-084 reverse: a patch that brings the items back clears the flag.
    assert!(l.revalidate_all(&old).unwrap().is_empty());
    assert!(!l.get(&hit.build_id).unwrap().unwrap().needs_update);
}

#[test]
fn editing_a_flagged_build_to_valid_items_clears_the_flag() {
    let l = lib();
    let old = catalog("100");
    let b = l.add(manual(1, "x", &[("a", &[1, 40])]), &old).unwrap();
    let new = AssetCatalog::from_parts(
        "101",
        (1..=5).map(hero).collect(),
        (1..=39).map(|i| item(i, false)).collect(),
    );
    l.revalidate_all(&new).unwrap();
    let mut b = l.get(&b.build_id).unwrap().unwrap();
    assert!(b.needs_update);
    b.sections = sections(&[("a", &[1, 39])]);
    l.update(b.clone(), &new).unwrap();
    assert!(!l.get(&b.build_id).unwrap().unwrap().needs_update);
}

#[test]
fn tst_083_the_app_pick_always_wins_over_the_in_game_build() {
    let cat = catalog("100");
    let l = lib();
    let app = l.add(manual(1, "App pick", &[("a", &[1])]), &cat).unwrap();
    let other_app = l.add(manual(1, "Other", &[("a", &[2])]), &cat).unwrap();
    let ingame = manual(1, "In-game selected", &[("a", &[3])]);
    let hero2 = l.add(manual(2, "Hero 2", &[("a", &[4])]), &cat).unwrap();

    // Priority 3: nothing.
    assert!(l.active_plan(1).unwrap().is_none());
    assert!(l.active_plan_with(1, None).unwrap().is_none());
    // Priority 2 (Beta hook): the in-game build is used when there is no app pick.
    assert_eq!(
        l.active_plan_with(1, Some(ingame.clone()))
            .unwrap()
            .unwrap()
            .name,
        "In-game selected"
    );
    // It must be for this hero.
    assert!(l
        .active_plan_with(2, Some(ingame.clone()))
        .unwrap()
        .is_none());

    // Priority 1: an app pick beats the in-game build.
    l.set_active(1, &app.build_id).unwrap();
    assert_eq!(
        l.active_plan_with(1, Some(ingame.clone()))
            .unwrap()
            .unwrap()
            .build_id,
        app.build_id
    );
    assert_eq!(l.active_plan(1).unwrap().unwrap().build_id, app.build_id);
    // A pick made during the match replaces the earlier one at once.
    l.set_active(1, &other_app.build_id).unwrap();
    assert_eq!(
        l.active_plan_with(1, Some(ingame.clone()))
            .unwrap()
            .unwrap()
            .build_id,
        other_app.build_id
    );
    // Picks are per hero.
    assert!(l.active_plan(2).unwrap().is_none());
    assert!(matches!(
        l.set_active(1, &hero2.build_id),
        Err(BuildError::WrongHero)
    ));
    assert!(matches!(l.set_active(1, "nope"), Err(BuildError::NotFound)));
    assert_eq!(
        l.db.active_build_plans().unwrap(),
        vec![(1, other_app.build_id.clone())]
    );

    // Clearing the pick returns to priority 2 then 3.
    assert!(l.clear_active(1).unwrap());
    assert_eq!(
        l.active_plan_with(1, Some(ingame)).unwrap().unwrap().name,
        "In-game selected"
    );
    assert!(l.active_plan(1).unwrap().is_none());
}

#[test]
fn deleting_the_picked_build_removes_the_pick() {
    let cat = catalog("100");
    let l = lib();
    let b = l.add(manual(1, "x", &[("a", &[1])]), &cat).unwrap();
    l.set_active(1, &b.build_id).unwrap();
    l.delete(&b.build_id).unwrap();
    assert!(l.active_plan(1).unwrap().is_none());
    assert!(l.db.active_build_plans().unwrap().is_empty());
}

// ---------------------------------------------------------------------------------------------
// PRG-19
// ---------------------------------------------------------------------------------------------

fn community(hero_id: u32, id: u32, cats: &[(&str, &[u32])]) -> serde_json::Value {
    json!({
        "hero_build": {
            "hero_id": hero_id, "hero_build_id": id, "name": format!("Community {id}"),
            "version": 3, "author_account_id": 1, "origin_build_id": id, "language": 0,
            "details": {
                "mod_categories": cats.iter().map(|(n, ids)| json!({
                    "name": n, "width": 1.0, "height": 1.0, "description": "",
                    "mods": ids.iter().map(|i| json!({"ability_id": i, "annotation": null})).collect::<Vec<_>>()
                })).collect::<Vec<_>>(),
                "ability_order": {"currency_changes": [
                    {"ability_id": 31, "currency_type": 2, "delta": -1},
                    {"ability_id": 32, "currency_type": 1, "delta": -2}
                ]}
            }
        },
        "num_favorites": 10, "num_ignores": null
    })
}

fn parse(v: serde_json::Value) -> CommunityBuild {
    serde_json::from_value(v).unwrap()
}

#[test]
fn tc_19_1_an_in_game_build_resolves_every_item() {
    let cat = catalog("100");
    let imp = convert(
        &parse(community(
            3,
            55,
            &[("Early", &[1, 2]), ("Late", &[3, 4, 5])],
        )),
        &cat,
    );
    assert!(imp.unresolved.is_empty());
    let b = &imp.build;
    assert_eq!(b.source, BuildSource::Ingame);
    assert_eq!(b.source_ref.as_deref(), Some("55"));
    assert_eq!(b.hero_id, 3);
    assert_eq!(b.patch, "100");
    assert_eq!(b.name, "Community 55");
    assert_eq!(b.sections[0].title, "Early");
    assert_eq!(b.sections[1].item_ids, vec![3, 4, 5]);
    assert_eq!(b.ability_order, Some(vec![31, 32]));
    // It passes the builder rules and stores.
    lib().add(imp.build, &cat).unwrap();
}

#[test]
fn tc_19_2_an_unknown_item_is_listed_not_dropped() {
    let cat = catalog("100");
    let imp = convert(
        &parse(community(1, 9, &[("A", &[1, 4_000_000_001, 2])])),
        &cat,
    );
    assert_eq!(imp.build.sections[0].item_ids, vec![1, 2]);
    assert_eq!(imp.unresolved.len(), 1);
    let u = &imp.unresolved[0];
    assert_eq!(
        (u.reason, u.id, u.section_title.as_str()),
        (SkipReason::Unknown, Some(4_000_000_001), "A")
    );
    assert_eq!(u.name, "4000000001");
    lib().add(imp.build, &cat).unwrap();
}

#[test]
fn duplicates_removed_items_and_empty_sections_are_all_accounted_for() {
    let mut items: Vec<_> = (1..=40).map(|i| item(i, false)).collect();
    items[6].disabled = true; // id 7
    let cat = AssetCatalog::from_parts("100", (1..=5).map(hero).collect(), items);
    let imp = convert(
        &parse(community(
            1,
            9,
            &[("A", &[1, 2]), ("B", &[2, 7]), ("", &[3])],
        )),
        &cat,
    );
    let reasons: Vec<_> = imp
        .unresolved
        .iter()
        .map(|u| (u.id.unwrap(), u.reason))
        .collect();
    assert_eq!(
        reasons,
        vec![(2, SkipReason::Duplicate), (7, SkipReason::Removed)]
    );
    assert_eq!(
        imp.build.sections.len(),
        2,
        "section B has nothing placeable and is left out"
    );
    assert_eq!(imp.build.sections[1].title, "Section 3");
    assert_eq!(imp.unresolved[1].name, "Item 7");
    lib().add(imp.build, &cat).unwrap();
}

async fn importer(server: &MockServer) -> (InGameCommunity, Arc<AssetCatalog>) {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
    let rules = Rules::load(&p).unwrap().api;
    let cfg = ApiConfig::default().with_base_url(server.uri());
    let client = Arc::new(ApiClient::new(Storage::open_in_memory().unwrap(), &rules, cfg).unwrap());
    let cat = Arc::new(catalog("100"));
    (InGameCommunity::new(client, cat.clone()), cat)
}

#[tokio::test]
async fn in_game_importer_handles_its_url_and_imports_by_id() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/builds"))
        .and(query_param("build_id", "55"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([community(
            2,
            55,
            &[("A", &[1, 2, 99])]
        )])))
        .expect(1)
        .mount(&server)
        .await;
    let (imp, _) = importer(&server).await;
    assert_eq!(imp.id(), "ingame-community");
    let url = Url::parse(&format!("{}/v1/builds?build_id=55", server.uri())).unwrap();
    assert!(imp.can_handle(&url));
    assert!(!imp.can_handle(&Url::parse("https://example.com/v1/builds?build_id=55").unwrap()));
    assert!(!imp.can_handle(&Url::parse(&format!("{}/v1/builds", server.uri())).unwrap()));
    let got = imp.import(&url).await.unwrap();
    assert_eq!(got.build.hero_id, 2);
    assert_eq!(got.unresolved.len(), 1);
    // A build the API doesn't have is an error, not an empty build.
    let missing = Url::parse(&format!("{}/v1/builds?build_id=56", server.uri())).unwrap();
    assert!(imp.import(&missing).await.is_err());
}

#[tokio::test]
async fn tst_080_twenty_builds_for_five_heroes_all_resolve() {
    let server = MockServer::start().await;
    for hero_id in 1..=5u32 {
        let list: Vec<_> = (0..4u32)
            .map(|n| {
                let first = 1 + n * 3;
                community(
                    hero_id,
                    hero_id * 100 + n,
                    &[
                        ("Early", &[first, first + 1]),
                        ("Late", &[first + 2, 30 + n]),
                    ],
                )
            })
            .collect();
        Mock::given(method("GET"))
            .and(path("/v1/builds"))
            .and(query_param("hero_id", hero_id.to_string()))
            .and(query_param("only_latest", "true"))
            .respond_with(ResponseTemplate::new(200).set_body_json(list))
            .expect(1)
            .mount(&server)
            .await;
    }
    let (imp, cat) = importer(&server).await;
    let l = lib();
    let mut total = 0;
    for hero_id in 1..=5 {
        let got = imp.top_for_hero(hero_id, "favorites", 4).await.unwrap();
        assert_eq!(got.len(), 4);
        for g in got {
            assert!(g.unresolved.is_empty(), "{:?}", g.unresolved);
            l.add(g.build, &cat).unwrap();
            total += 1;
        }
    }
    assert_eq!(total, 20);
    assert_eq!(l.list(None).unwrap().len(), 20);
}
