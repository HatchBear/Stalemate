//! PRG-03 tests (TC-03-1 to TC-03-5), TST-013, TST-017, TST-032. All JSON is synthetic.

use std::{
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use governor::clock::{Clock, FakeRelativeClock};
use serde_json::json;
use wiremock::{
    matchers::{header, method, path, query_param, query_param_is_missing},
    Mock, MockServer, Request, Respond, ResponseTemplate,
};

use super::*;
use crate::settings::{MemorySecretStore, Rules};

fn rules() -> ApiRules {
    let p = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
    Rules::load(&p).unwrap().api
}

fn client_with(
    server: &MockServer,
    rules: &ApiRules,
    fake: Option<FakeRelativeClock>,
) -> ApiClient {
    let mut cfg = ApiConfig::default().with_base_url(server.uri());
    if let Some(c) = fake {
        cfg = cfg.with_fake_clock(c);
    }
    ApiClient::new(Storage::open_in_memory().unwrap(), rules, cfg).unwrap()
}

fn client(server: &MockServer) -> ApiClient {
    client_with(server, &rules(), None)
}

fn history_json(account: u32, match_id: u64) -> serde_json::Value {
    json!([{
        "account_id": account, "match_id": match_id, "hero_id": 7, "hero_level": 24,
        "start_time": 1_700_000_000u64, "match_duration_s": 1800, "match_result": 1,
        "player_team": 1, "player_match_outcome": 1, "game_mode": 1, "match_mode": 1,
        "player_kills": 5, "player_deaths": 2, "player_assists": 9, "last_hits": 100,
        "denies": 3, "net_worth": 30000, "objectives_mask_team0": 0, "objectives_mask_team1": 5,
        "ranked_delta": null, "a_new_field_upstream": [1, 2]
    }])
}

#[tokio::test]
async fn tc_03_1_second_identical_call_within_ttl_makes_no_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/players/42/match-history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_json(42, 900)))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let a = c.match_history(42).await.unwrap();
    let b = c.match_history(42).await.unwrap();
    assert_eq!(a, b);
    assert_eq!(a[0].match_id, 900);
    assert!(a[0].won());
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn tc_03_2_503_honours_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(503).insert_header("Retry-After", "7"))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let e = c.match_history(1).await.unwrap_err();
    assert!(
        matches!(
            e,
            ApiError::Busy {
                retry_after_s: 7,
                timeout: false
            }
        ),
        "{e:?}"
    );
    // The same query is held back for those 7 s without another request.
    let e = c.match_history(1).await.unwrap_err();
    assert!(matches!(e, ApiError::Busy { timeout: false, .. }), "{e:?}");
}

#[tokio::test]
async fn tc_03_2_busy_uses_the_config_default_without_retry_after() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(429))
        .mount(&server)
        .await;
    let c = client(&server);
    let want = rules().busy_retry_default_s;
    match c.match_history(1).await.unwrap_err() {
        ApiError::Busy { retry_after_s, .. } => assert_eq!(retry_after_s, want),
        e => panic!("{e:?}"),
    }
}

#[tokio::test]
async fn tc_03_3_504_blocks_the_same_query_for_five_minutes() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/players/1/match-history"))
        .respond_with(ResponseTemplate::new(504))
        .expect(2)
        .mount(&server)
        .await;
    let clock = FakeRelativeClock::default();
    let c = client_with(&server, &rules(), Some(clock.clone()));
    let e = c.match_history(1).await.unwrap_err();
    let ApiError::Busy {
        retry_after_s,
        timeout,
    } = e
    else {
        panic!("{e:?}")
    };
    assert!(retry_after_s >= 300 && timeout);

    // Inside the cooldown: no request, and the time left shrinks.
    clock.advance(Duration::from_secs(100));
    match c.match_history(1).await.unwrap_err() {
        ApiError::Busy {
            retry_after_s,
            timeout: true,
        } => assert!((199..=200).contains(&retry_after_s)),
        e => panic!("{e:?}"),
    }
    // After the cooldown the same query may go out again: that is request number 2.
    clock.advance(Duration::from_secs(201));
    assert!(matches!(
        c.match_history(1).await,
        Err(ApiError::Busy { timeout: true, .. })
    ));
}

/// Records the fake time at which each request reached the server.
struct Stamp {
    clock: FakeRelativeClock,
    times: Arc<Mutex<Vec<Duration>>>,
}

impl Respond for Stamp {
    fn respond(&self, _: &Request) -> ResponseTemplate {
        self.times
            .lock()
            .unwrap()
            .push(Duration::from(self.clock.now()));
        ResponseTemplate::new(200).set_body_json(json!([]))
    }
}

#[tokio::test]
async fn tc_03_4_400_analytics_calls_stay_within_160_per_minute() {
    let server = MockServer::start().await;
    let clock = FakeRelativeClock::default();
    let times = Arc::new(Mutex::new(Vec::new()));
    Mock::given(method("GET"))
        .respond_with(Stamp {
            clock: clock.clone(),
            times: times.clone(),
        })
        .mount(&server)
        .await;
    let r = rules();
    let c = client_with(&server, &r, Some(clock));
    let started = std::time::Instant::now();
    for i in 0..400u32 {
        // Each query differs, so the cache never answers.
        let v: Vec<serde_json::Value> = c
            .get_analytics(
                "API-07",
                "/v1/analytics/item-stats",
                &[("n", i.to_string())],
            )
            .await
            .unwrap();
        assert!(v.is_empty());
    }
    assert!(
        started.elapsed() < Duration::from_secs(30),
        "the fake clock must not wait"
    );
    let t = times.lock().unwrap().clone();
    assert_eq!(t.len(), 400);
    let window = Duration::from_secs(60);
    for (i, s) in t.iter().enumerate() {
        let n = t[i..].iter().filter(|x| **x - *s <= window).count();
        assert!(
            n <= r.analytics_per_min as usize,
            "{n} calls inside one minute"
        );
    }
    // It did have to wait: 400 calls at 160/min cannot finish in under 1.5 minutes.
    assert!(*t.last().unwrap() > Duration::from_secs(90));
}

#[tokio::test]
async fn tc_03_5_404_is_no_public_data_and_never_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    for _ in 0..3 {
        assert!(matches!(
            c.match_history(5).await,
            Err(ApiError::NoPublicData)
        ));
    }
    assert_eq!(server.received_requests().await.unwrap().len(), 1);
}

#[tokio::test]
async fn tst_032_a_404_is_remembered_across_clients_through_the_cache() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/players/5/match-history"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/players/6/match-history"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_json(6, 1)))
        .mount(&server)
        .await;
    let db = Storage::open_in_memory().unwrap();
    let mk = || {
        let cfg = ApiConfig::default().with_base_url(server.uri());
        ApiClient::new(db.clone(), &rules(), cfg).unwrap()
    };
    assert!(matches!(
        mk().match_history(5).await,
        Err(ApiError::NoPublicData)
    ));
    // A new client (as after a restart) reads the stored 404 and sends nothing.
    assert!(matches!(
        mk().match_history(5).await,
        Err(ApiError::NoPublicData)
    ));
    // Other players are unaffected.
    assert_eq!(mk().match_history(6).await.unwrap().len(), 1);
    let n5 = server
        .received_requests()
        .await
        .unwrap()
        .iter()
        .filter(|r| r.url.path().contains("/5/"))
        .count();
    assert_eq!(n5, 1);
}

#[tokio::test]
async fn forbidden_parameters_are_refused_before_any_request() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let c = client(&server);
    for bad in ["force_refetch", "refresh"] {
        let r: Result<Vec<serde_json::Value>, _> = c
            .get_analytics(
                "API-12",
                "/v1/analytics/hero-stats",
                &[(bad, "true".into())],
            )
            .await;
        assert!(
            matches!(r, Err(ApiError::ForbiddenParam(ref k)) if k == bad),
            "{r:?}"
        );
    }
    assert!(server.received_requests().await.unwrap().is_empty());
}

#[tokio::test]
async fn no_typed_call_ever_sends_force_refetch_or_refresh() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/client-versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([5])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([])))
        .mount(&server)
        .await;
    let c = client(&server);
    let _ = c.match_history(1).await;
    let _ = c.player_ranks(&[1, 2]).await;
    let _ = c.player_hero_stats(&[1, 2]).await;
    let _ = c.steam_profiles(&[1, 2]).await;
    let _ = c
        .hero_build_stats(7, &[1], &[("min_matches", "5".into())])
        .await;
    let _ = c
        .match_metadata_bulk(&[1, 2], &MetadataOptions::default())
        .await;
    let _ = c.community_builds(7, "favorites", 10).await;
    let _ = c.community_build(77).await;
    let _ = c.load_catalog().await;
    let reqs = server.received_requests().await.unwrap();
    assert!(reqs.len() >= 8, "the calls went out: {}", reqs.len());
    for r in &reqs {
        for (k, _) in r.url.query_pairs() {
            assert!(
                k != "force_refetch" && k != "refresh",
                "{} sent {k}",
                r.url.path()
            );
        }
    }
}

#[tokio::test]
async fn user_agent_is_sent_and_a_key_only_when_the_user_set_one() {
    let server = MockServer::start().await;
    let ua = user_agent();
    assert!(
        ua.starts_with("stalemate/") && ua.ends_with("(+https://github.com/HatchBear/Stalemate)")
    );
    Mock::given(method("GET"))
        .and(path("/v1/players/1/match-history"))
        .and(header("user-agent", ua.as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_json(1, 1)))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/players/2/match-history"))
        .and(header("X-API-KEY", "user-secret"))
        .respond_with(ResponseTemplate::new(200).set_body_json(history_json(2, 1)))
        .mount(&server)
        .await;
    let c = client(&server);
    c.match_history(1).await.unwrap();
    assert!(server.received_requests().await.unwrap()[0]
        .headers
        .get("x-api-key")
        .is_none());

    let store = MemorySecretStore::new();
    store
        .set(secret_keys::DEADLOCK_API_KEY, "user-secret")
        .await
        .unwrap();
    let cfg = ApiConfig::default()
        .with_base_url(server.uri())
        .with_key_from(&store)
        .await;
    let keyed = ApiClient::new(Storage::open_in_memory().unwrap(), &rules(), cfg).unwrap();
    keyed.match_history(2).await.unwrap();
}

#[tokio::test]
async fn rank_and_steam_use_batch_params_sorted_and_deduped() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/players/rank"))
        .and(query_param("account_ids", "3,9"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"account_id": 3, "badge": 65, "rank": 6, "subrank": 5, "last_match": null},
            {"account_id": 9, "badge": 0, "rank": 0, "subrank": 0}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/players/steam"))
        .and(query_param("account_ids", "3,9"))
        .and(query_param_is_missing("refresh"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"account_id": 3, "personaname": "Synthetic", "avatar": "", "avatarfull": "", "avatarmedium": "",
             "profileurl": "", "friends": [{"account_id": 1}], "realname": "Not Kept",
             "last_updated": "2026-01-01T00:00:00Z", "matches_played_last_30d": 4}
        ])))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let r = c.player_ranks(&[9, 3, 9]).await.unwrap();
    assert_eq!(r[0].badge, 65);
    assert!(r[1].last_match.is_none());
    // Same set in another order is the same cache key.
    c.player_ranks(&[3, 9]).await.unwrap();
    assert_eq!(
        c.steam_profiles(&[9, 3]).await.unwrap()[0].personaname,
        "Synthetic"
    );
    assert!(c.player_ranks(&[]).await.is_err());
}

fn meta_json(id: u64, with_stats: bool) -> serde_json::Value {
    let stats = if with_stats {
        json!([{"time_stamp_s": 180, "net_worth": 2100, "gold_source_lane_creeps_gold": 900,
            "gold_source_neutrals_gold": 0, "gold_source_item_cultist_sacrifice_gold": 0,
            "future_field": 1}])
    } else {
        json!([])
    };
    json!({
        "match_id": id, "start_time": "2026-11-06 00:08:40", "winning_team": "Team0",
        "duration_s": 1500, "match_outcome": "TeamWin", "match_mode": "Unranked", "game_mode": "Normal",
        "objectives": [{"team_objective": "Tier1Lane1", "team": "Team1", "destroyed_time_s": 400,
            "first_damage_time_s": 380}],
        "players": [{"account_id": 10, "hero_id": 7, "player_slot": 1, "team": "Team0",
            "assigned_lane": 4, "hero_build_id": 555, "pregame_hero_id": 3,
            "items": [{"item_id": 4_000_000_000u32, "game_time_s": 60, "sold_time_s": 0, "upgrade_id": 0, "flags": 0}],
            "stats": stats}],
        "banned_hero_ids": []
    })
}

#[tokio::test]
async fn bulk_metadata_caches_per_match_and_requests_only_new_ones() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/matches/metadata"))
        .and(query_param("match_ids", "1,2"))
        .and(query_param("include_player_info", "true"))
        .and(query_param("include_player_stats", "true"))
        .and(query_param("include_player_items", "true"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_json(json!([meta_json(1, true), meta_json(2, true)])),
        )
        .expect(1)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/matches/metadata"))
        .and(query_param("match_ids", "3"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([meta_json(3, false)])))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    let o = MetadataOptions::default();
    let first = c.match_metadata_bulk(&[1, 2], &o).await.unwrap();
    assert_eq!(first.len(), 2);
    let p = &first[0].players[0];
    assert_eq!(p.hero_build_id, Some(555));
    assert_eq!(p.pregame_hero_id, Some(3));
    assert_eq!(p.assigned_lane, Some(4));
    assert_eq!(p.items[0].item_id, 4_000_000_000);
    assert_eq!(p.stats[0].gold_source_lane_creeps_gold, 900);
    assert_eq!(first[0].objectives[0].team_objective, "Tier1Lane1");
    // 1 and 2 come from the cache, only 3 is requested; the answer keeps the asked order.
    let second = c.match_metadata_bulk(&[3, 2, 1], &o).await.unwrap();
    assert_eq!(
        second.iter().map(|m| m.match_id).collect::<Vec<_>>(),
        vec![3, 2, 1]
    );
    assert_eq!(server.received_requests().await.unwrap().len(), 2);
    // Different options are a different cache entry.
    let none = MetadataOptions {
        player_stats: false,
        ..o
    };
    assert_ne!(o.fingerprint(), none.fingerprint());
}

#[tokio::test]
async fn single_metadata_always_disables_steam() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/matches/77/metadata"))
        .and(query_param("disable_steam", "true"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "match_info": {"match_id": 77}, "hero_build_ids": {"10": 5}, "pregame_hero_ids": {}, "banned_hero_ids": [1]
        })))
        .expect(1)
        .mount(&server)
        .await;
    let m = client(&server).match_metadata_single(77).await.unwrap();
    assert_eq!(m.hero_build_ids["10"], 5);
    assert_eq!(m.banned_hero_ids, vec![1]);
}

#[tokio::test]
async fn builds_are_found_through_the_search_never_the_coordinator_path() {
    let server = MockServer::start().await;
    let build = json!([{"hero_build": {"hero_id": 7, "hero_build_id": 5, "name": "B", "version": 2,
        "details": {"mod_categories": [], "ability_order": null}}}]);
    Mock::given(method("GET"))
        .and(path("/v1/builds"))
        .and(query_param("build_id", "5"))
        .respond_with(ResponseTemplate::new(200).set_body_json(build))
        .expect(1)
        .mount(&server)
        .await;
    let b = client(&server).community_build(5).await.unwrap().unwrap();
    assert_eq!(b.hero_build.name, "B");
    for r in server.received_requests().await.unwrap() {
        assert_eq!(r.url.path(), "/v1/builds");
    }
}

#[tokio::test]
async fn tst_017_catalog_loads_from_assets_and_is_cached_per_patch() {
    let server = MockServer::start().await;
    // The first patch check answers 100, later ones 101.
    Mock::given(method("GET"))
        .and(path("/v1/assets/client-versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([99, 100])))
        .up_to_n_times(2)
        .expect(2)
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/v1/assets/client-versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([100, 101])))
        .mount(&server)
        .await;
    for (p, body) in [
        (
            "/v1/assets/heroes",
            json!([{"id": 7, "class_name": "hero_x", "name": "X", "player_selectable": true,
                "disabled": false, "in_development": false, "images": {}}]),
        ),
        (
            "/v1/assets/items",
            json!([{"id": 1, "class_name": "a", "name": "A", "type": "upgrade", "cost": 500},
                {"id": 2, "class_name": "b", "name": "B", "type": "upgrade", "cost": 1250,
                 "component_items": ["a"]}]),
        ),
    ] {
        Mock::given(method("GET"))
            .and(path(p))
            .and(query_param("client_version", "100"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body.clone()))
            .expect(1)
            .mount(&server)
            .await;
        Mock::given(method("GET"))
            .and(path(p))
            .and(query_param("client_version", "101"))
            .respond_with(ResponseTemplate::new(200).set_body_json(body))
            .expect(1)
            .mount(&server)
            .await;
    }
    let r = ApiRules {
        client_versions_ttl_s: 0,
        ..rules()
    };
    let c = client_with(&server, &r, None);
    let a = c.load_catalog().await.unwrap();
    assert_eq!(a.patch(), "100");
    assert_eq!(a.heroes()[0].name, "X");
    assert!(crate::purchase_log::ItemTree::is_component_of(&a, 1, 2));
    // Same patch: the catalog comes from api_cache, no asset requests.
    let b = c.load_catalog().await.unwrap();
    assert_eq!(b.patch(), "100");
    // New patch: assets are fetched again for 101.
    let d = c.load_catalog().await.unwrap();
    assert_eq!(d.patch(), "101");
    assert_eq!(c.patch().as_deref(), Some("101"));
}

#[tokio::test]
async fn images_download_once_into_the_patch_folder() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/img/a.png"))
        .and(query_param_is_missing("x"))
        .and(wiremock::matchers::header_regex(
            "user-agent",
            "^stalemate/",
        ))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"png-bytes".to_vec()))
        .expect(1)
        .mount(&server)
        .await;
    let tmp = tempfile::tempdir().unwrap();
    let dirs = AssetDirs::new(tmp.path());
    let c = client(&server);
    let url = format!("{}/img/a.png", server.uri());
    let p1 = c
        .ensure_image(&dirs, "100", ImageKind::Item, 1, &url)
        .await
        .unwrap();
    let p2 = c
        .ensure_image(&dirs, "100", ImageKind::Item, 1, &url)
        .await
        .unwrap();
    assert_eq!(p1, p2);
    assert_eq!(std::fs::read(&p1).unwrap(), b"png-bytes");
    assert!(p1.starts_with(tmp.path().join("assets")));
    assert!(c
        .ensure_image(&dirs, "100", ImageKind::Item, 2, "file:///etc/passwd")
        .await
        .is_err());
    // The API key must not go to the image host: no header on image requests.
    let reqs = server.received_requests().await.unwrap();
    assert!(!reqs.iter().any(|r| r.headers.contains_key("x-api-key")));
}

#[test]
fn endpoint_table_maps_each_id_to_its_scope() {
    use Endpoint::*;
    let t = |e: Endpoint| (e.spec().id, e.spec().scope);
    assert_eq!(t(MatchHistory), ("API-01", Scope::Players));
    assert_eq!(t(PlayerRank), ("API-02", Scope::Rank));
    assert_eq!(t(PlayerHeroStats), ("API-03", Scope::Players));
    assert_eq!(t(SteamProfiles), ("API-04", Scope::Players));
    assert_eq!(t(MatchMetadataBulk), ("API-05", Scope::MetadataBulk));
    assert_eq!(t(MatchMetadataSingle), ("API-05", Scope::Other));
    assert_eq!(t(HeroBuildStats), ("API-06", Scope::Analytics));
    assert_eq!(t(Analytics("API-09")), ("API-09", Scope::Analytics));
    assert_eq!(t(Assets), ("API-15", Scope::Other));
    assert_eq!(t(ClientVersions), ("API-15", Scope::Other));
    assert_eq!(t(Builds), ("API-16", Scope::Builds));
    assert_eq!(t(LiveUrls), ("LOC-1", Scope::LiveListing));
    assert_eq!(t(LiveUrl), ("LOC-2", Scope::LiveUrl));
}

#[tokio::test]
async fn locator_listing_decodes_and_is_never_cached() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/matches/live/urls"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!([
            {"match_id": 5, "broadcast_url": "http://relay.test/tv/5_1", "lobby_id": 9,
             "updated_at": 1, "new_field": true},
            {"match_id": 6, "broadcast_url": "http://relay.test/tv/6_2"}
        ])))
        .expect(2)
        .mount(&server)
        .await;
    let c = client(&server);
    let a = c.live_urls().await.unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!(a[0].broadcast_url, "http://relay.test/tv/5_1");
    assert_eq!(a[1].lobby_id, None);
    c.live_urls().await.unwrap(); // a second call is a second request
}

#[tokio::test]
async fn locator_per_match_call_respects_its_hourly_budget_without_sleeping() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(wiremock::matchers::path_regex(
            r"^/v1/matches/\d+/live/url$",
        ))
        .respond_with(
            ResponseTemplate::new(200).set_body_json(json!({"broadcast_url": "http://r/tv/1"})),
        )
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert_eq!(c.live_url_for_match(1).await.unwrap(), "http://r/tv/1");
    let t = std::time::Instant::now();
    match c.live_url_for_match(2).await {
        Err(ApiError::Busy { retry_after_s, .. }) => assert!(retry_after_s > 0),
        other => panic!("expected Busy, got {other:?}"),
    }
    assert!(t.elapsed() < Duration::from_secs(1), "must not sleep");
}

#[tokio::test]
async fn locator_per_match_404_is_remembered_and_not_retried() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/v1/matches/7/live/url"))
        .respond_with(ResponseTemplate::new(404))
        .expect(1)
        .mount(&server)
        .await;
    let c = client(&server);
    assert!(matches!(
        c.live_url_for_match(7).await,
        Err(ApiError::NoPublicData)
    ));
    assert!(matches!(
        c.live_url_for_match(7).await,
        Err(ApiError::NoPublicData)
    ));
}

/// TST-013: no request over any published limit. The published numbers are the 2026-11-05 spec's;
/// the config values must be at or under them, and one simulated hour through the buckets must
/// never put more than the published count in any window.
#[test]
fn tst_013_config_is_under_the_published_limits() {
    let r = rules();
    assert!(r.analytics_per_min <= 200);
    assert!(r.players_per_sec <= 100);
    assert!(r.rank_per_min <= 20);
    assert!(r.metadata_bulk_per_min <= 30);
    assert!(
        r.live_url_per_hour * 5 <= 6 * 4,
        "per-match locator: 6 req/h published"
    );
    // 80% rule (doc 04): not above 80% of the published limit.
    assert!(r.analytics_per_min * 5 <= 200 * 4);
    assert!(r.players_per_sec * 5 <= 100 * 4);
    assert!(r.rank_per_min * 5 <= 20 * 4);
    assert!(r.metadata_bulk_per_min * 5 <= 30 * 4);
}

#[tokio::test]
async fn tst_013_one_simulated_hour_never_exceeds_a_published_limit() {
    let r = rules();
    let fake = FakeRelativeClock::default();
    let clock = PaceClock::Fake(fake);
    let buckets = Buckets::new(&r, &clock);
    let hour = Duration::from_secs(3600);
    let cases = [
        (Scope::Analytics, 200usize, Duration::from_secs(60)),
        (Scope::Players, 100, Duration::from_secs(1)),
        (Scope::Builds, 100, Duration::from_secs(1)),
        (Scope::Rank, 20, Duration::from_secs(60)),
        (Scope::MetadataBulk, 30, Duration::from_secs(60)),
    ];
    for (scope, published, window) in cases {
        // Start each scope from time zero on its own clock offset: record times until an hour passes.
        let start = Duration::from(clock.now());
        let mut t = Vec::new();
        while Duration::from(clock.now()) - start < hour {
            buckets.get(scope).acquire().await;
            t.push(Duration::from(clock.now()));
        }
        let mut lo = 0;
        for hi in 0..t.len() {
            while t[hi] - t[lo] > window {
                lo += 1;
            }
            assert!(
                hi - lo < published,
                "{scope:?}: {} in one window",
                hi - lo + 1
            );
        }
    }
}
