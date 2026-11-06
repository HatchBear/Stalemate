//! Finding a match's Broadcast URL (OQ-02, DEC-049).
//!
//! The app runs no game-coordinator client. It asks deadlock-api:
//!
//! 1. `GET /v1/matches/live/urls`: the free listing (100 req/s). Look for the match.
//! 2. If the match is not listed, `GET /v1/matches/{match_id}/live/url` (6 req/h per IP; the bucket
//!    is `rules.api.live_url_per_hour`). deadlock-api spectates the match on its side.
//!
//! The URL is kept per match and reused on every reconnect. After a failed connection the listing
//! is checked again (it is free), and a listed URL replaces the kept one. The per-match endpoint is
//! called again only when no URL is kept: a first call that was refused or failed, or a URL a
//! session proved to be for another match. A 404 from it is remembered by the API client, so it is
//! not repeated either (NFR-FAIR-004).

use std::{collections::HashMap, sync::Arc, sync::Mutex};

use async_trait::async_trait;

use crate::{
    api_client::{ApiClient, ApiError},
    types::MatchId,
};

#[derive(Debug, thiserror::Error)]
pub enum LocateError {
    /// Not in the listing and the per-match endpoint has no Broadcast for it (404).
    #[error("no Broadcast is available for this match")]
    NotAvailable,
    /// The per-match budget is spent, or deadlock-api asked us to wait.
    #[error("the locator is rate limited, retry after {retry_after_s}s")]
    Busy { retry_after_s: u64 },
    #[error("locator request failed: {0}")]
    Api(String),
}

/// Where the Broadcast client gets a match's URL. [`Locator`] is the real one; tests use a mock.
#[async_trait]
pub trait UrlLocator: Send + Sync {
    async fn locate(&self, match_id: MatchId) -> Result<String, LocateError>;

    /// The last URL failed. `wrong_match` means a session proved it belongs to another match, so
    /// it must be dropped. Otherwise it is kept, but the listing is checked first next time.
    fn report_failure(&self, match_id: MatchId, wrong_match: bool);
}

#[derive(Default)]
struct Entry {
    url: Option<String>,
    /// A connection failed since the URL was found: look at the listing again.
    stale: bool,
}

/// The deadlock-api locator.
pub struct Locator {
    api: Arc<ApiClient>,
    state: Mutex<HashMap<MatchId, Entry>>,
}

impl Locator {
    pub fn new(api: Arc<ApiClient>) -> Self {
        Self {
            api,
            state: Mutex::new(HashMap::new()),
        }
    }

    fn entry<R>(&self, id: MatchId, f: impl FnOnce(&mut Entry) -> R) -> R {
        let mut m = self.state.lock().unwrap_or_else(|e| e.into_inner());
        f(m.entry(id).or_default())
    }

    fn store(&self, id: MatchId, url: &str) {
        self.entry(id, |e| {
            e.url = Some(url.to_string());
            e.stale = false;
        });
    }
}

#[async_trait]
impl UrlLocator for Locator {
    async fn locate(&self, match_id: MatchId) -> Result<String, LocateError> {
        let (kept, stale) = self.entry(match_id, |e| (e.url.clone(), e.stale));
        if let (Some(url), false) = (&kept, stale) {
            return Ok(url.clone());
        }
        // Step 1: the free listing. A failure here is not fatal: fall through.
        match self.api.live_urls().await {
            Ok(list) => {
                if let Some(hit) = list.into_iter().find(|l| l.match_id == match_id) {
                    self.store(match_id, &hit.broadcast_url);
                    return Ok(hit.broadcast_url);
                }
            }
            Err(e) => tracing::debug!("broadcast listing failed: {e}"),
        }
        if let Some(url) = kept {
            // Not listed any more (or the listing is down): the kept URL stays the best guess.
            self.store(match_id, &url);
            return Ok(url);
        }
        // Step 2: the per-match endpoint.
        match self.api.live_url_for_match(match_id).await {
            Ok(url) => {
                self.store(match_id, &url);
                Ok(url)
            }
            Err(ApiError::NoPublicData) => Err(LocateError::NotAvailable),
            Err(ApiError::Busy { retry_after_s, .. }) => Err(LocateError::Busy { retry_after_s }),
            Err(e) => Err(LocateError::Api(e.to_string())),
        }
    }

    fn report_failure(&self, match_id: MatchId, wrong_match: bool) {
        self.entry(match_id, |e| {
            e.stale = true;
            if wrong_match {
                e.url = None;
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api_client::ApiConfig, settings::Rules, storage::Storage};
    use serde_json::json;
    use wiremock::{
        matchers::{method, path},
        Mock, MockServer, ResponseTemplate,
    };

    fn locator(server: &MockServer) -> Locator {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config/rules.toml");
        let rules = Rules::load(&p).unwrap().api;
        let api = ApiClient::new(
            Storage::open_in_memory().unwrap(),
            &rules,
            ApiConfig::default().with_base_url(server.uri()),
        )
        .unwrap();
        Locator::new(Arc::new(api))
    }

    async fn listing(server: &MockServer, entries: serde_json::Value, expect: u64) {
        Mock::given(method("GET"))
            .and(path("/v1/matches/live/urls"))
            .respond_with(ResponseTemplate::new(200).set_body_json(entries))
            .expect(expect)
            .mount(server)
            .await;
    }

    #[tokio::test]
    async fn a_listed_match_never_touches_the_per_match_endpoint() {
        let server = MockServer::start().await;
        listing(
            &server,
            json!([{"match_id": 1, "broadcast_url": "http://relay/tv/1_a"},
                   {"match_id": 2, "broadcast_url": "http://relay/tv/2_b"}]),
            1,
        )
        .await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/2/live/url"))
            .respond_with(ResponseTemplate::new(500))
            .expect(0)
            .mount(&server)
            .await;
        let l = locator(&server);
        assert_eq!(l.locate(2).await.unwrap(), "http://relay/tv/2_b");
        // The second call reuses the kept URL: no second listing request (expect(1) above).
        assert_eq!(l.locate(2).await.unwrap(), "http://relay/tv/2_b");
    }

    #[tokio::test]
    async fn an_unlisted_match_calls_the_per_match_endpoint_once_and_keeps_the_url() {
        let server = MockServer::start().await;
        listing(&server, json!([]), 1).await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/3/live/url"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"broadcast_url": "http://r/tv/3"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let l = locator(&server);
        assert_eq!(l.locate(3).await.unwrap(), "http://r/tv/3");
        assert_eq!(l.locate(3).await.unwrap(), "http://r/tv/3");
    }

    #[tokio::test]
    async fn after_a_failure_the_listing_is_rechecked_but_the_per_match_call_is_not_repeated() {
        let server = MockServer::start().await;
        // Listing never has the match; the kept URL stays in use.
        listing(&server, json!([]), 2).await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/4/live/url"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"broadcast_url": "http://r/tv/4"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let l = locator(&server);
        assert_eq!(l.locate(4).await.unwrap(), "http://r/tv/4"); // listing 1 + per-match
        l.report_failure(4, false);
        assert_eq!(l.locate(4).await.unwrap(), "http://r/tv/4"); // listing 2, kept URL
    }

    #[tokio::test]
    async fn a_listed_url_replaces_the_kept_one_after_a_failure() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/live/urls"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(
                    json!([{"match_id": 5, "broadcast_url": "http://relay/tv/5_new"}]),
                ),
            )
            .mount(&server)
            .await;
        let l = locator(&server);
        l.store(5, "http://relay/tv/5_old");
        assert_eq!(l.locate(5).await.unwrap(), "http://relay/tv/5_old");
        l.report_failure(5, false);
        assert_eq!(l.locate(5).await.unwrap(), "http://relay/tv/5_new");
    }

    #[tokio::test]
    async fn a_404_from_the_per_match_endpoint_is_not_available_and_not_retried() {
        let server = MockServer::start().await;
        listing(&server, json!([]), 2).await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/6/live/url"))
            .respond_with(ResponseTemplate::new(404))
            .expect(1)
            .mount(&server)
            .await;
        let l = locator(&server);
        assert!(matches!(l.locate(6).await, Err(LocateError::NotAvailable)));
        assert!(matches!(l.locate(6).await, Err(LocateError::NotAvailable)));
    }

    #[tokio::test]
    async fn a_url_for_the_wrong_match_is_dropped() {
        let server = MockServer::start().await;
        listing(&server, json!([]), 2).await;
        Mock::given(method("GET"))
            .and(path("/v1/matches/8/live/url"))
            .respond_with(
                ResponseTemplate::new(200).set_body_json(json!({"broadcast_url": "http://r/tv/8"})),
            )
            .expect(1)
            .mount(&server)
            .await;
        let l = locator(&server);
        l.locate(8).await.unwrap();
        l.report_failure(8, true);
        // Nothing kept: the listing is checked, then the per-match endpoint is asked again, but the
        // hourly budget (one in the burst) is spent, so it is refused without a request.
        assert!(matches!(l.locate(8).await, Err(LocateError::Busy { .. })));
    }
}
