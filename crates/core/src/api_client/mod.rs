//! M-03 deadlock-api client (PRG-03). See 04-data-sources-and-ingestion.md.
//!
//! Every call goes through one place: [`ApiClient::fetch`] applies the forbidden-parameter guard,
//! the per-scope token bucket and the status handling. [`ApiClient::get_cached`] adds the
//! `api_cache` layer. The endpoint table ([`Endpoint::spec`]) maps each endpoint ID to its limit
//! scope and TTL, so no call site picks either.
//!
//! Rules enforced here (doc 04 "Client rules", NFR-FAIR-004, NFR-FAIR-005):
//! - `404` is [`ApiError::NoPublicData`]. It is never retried, and the key is remembered (in memory
//!   and in `api_cache`) so the same request isn't sent again during the match.
//! - `503` and `429` honour `Retry-After`. `504` blocks the same query for `timeout_cooldown_s`.
//! - `force_refetch` and `refresh` are refused for every endpoint ([`ApiError::ForbiddenParam`]).
//! - One bucket per limit scope, at 80% of the published limit (config), with a burst small enough
//!   that no window ever holds more than the limit.
//! - No key ships. A user key from the secret store is sent as `X-API-KEY`.

pub mod assets;
mod limiter;
pub mod types;

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Mutex, RwLock,
    },
    time::Duration,
};

use governor::{clock::Clock, clock::FakeRelativeClock, nanos::Nanos};
use serde::de::DeserializeOwned;

pub use assets::{AssetCatalog, AssetDirs, HeroAsset, ImageKind, ItemAsset};
use limiter::{Bucket, PaceClock};
pub use types::*;

use crate::{
    settings::{secret_keys, ApiRules, SecretStore},
    storage::{cache_key, Storage},
    types::{AccountId, HeroId, MatchId},
};

pub const DEFAULT_BASE_URL: &str = "https://api.deadlock-api.com";

/// The spec caps array parameters at 1000 items.
pub const MAX_BATCH_IDS: usize = 1000;

/// Query parameters that make deadlock-api call Valve for other players. Refused on every call.
const FORBIDDEN_PARAMS: [&str; 2] = ["force_refetch", "refresh"];

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    /// 404: the player opted out or has no public data. Never retried (NFR-FAIR-004).
    #[error("no public data")]
    NoPublicData,
    #[error("busy, retry after {retry_after_s}s")]
    Busy { retry_after_s: u64, timeout: bool },
    #[error("http {0}")]
    Http(u16),
    #[error("a forbidden query parameter was used: {0}")]
    ForbiddenParam(String),
    #[error("invalid request: {0}")]
    Invalid(String),
    #[error(transparent)]
    Net(#[from] reqwest::Error),
    #[error(transparent)]
    Decode(#[from] serde_json::Error),
    #[error("could not write an asset file: {0}")]
    Io(#[from] std::io::Error),
}

/// A limit scope. Each has its own bucket.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Shared by all analytics endpoints: 200 req/min published.
    Analytics,
    /// Stored player data, 100 req/s published (API-01, API-03, API-04).
    Players,
    /// API-02 rank batch, 20 req/min published.
    Rank,
    /// API-05 bulk metadata, 30 req/min published.
    MetadataBulk,
    /// API-16 builds, 100 req/s published. Its own bucket, sized like `Players`.
    Builds,
    /// No published limit (API-15, single metadata): 60 req/min [ASSUMPTION].
    Other,
    /// Broadcast listing `GET /v1/matches/live/urls`, 100 req/s published. Its own bucket.
    LiveListing,
    /// Per-match Broadcast locator, 6 req/h per IP published. Never waits: a call over the budget
    /// returns [`ApiError::Busy`] at once, because the wait could be most of an hour.
    LiveUrl,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ttl {
    History,
    Analytics,
    Metadata,
    Assets,
    ClientVersions,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EndpointSpec {
    pub id: &'static str,
    pub scope: Scope,
    pub ttl: Ttl,
}

/// The endpoints this client calls. The one table that maps an endpoint to its ID, limit scope and
/// TTL is [`Endpoint::spec`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Endpoint {
    /// API-01
    MatchHistory,
    /// API-02 (current rank only; MMR history has no endpoint)
    PlayerRank,
    /// API-03
    PlayerHeroStats,
    /// API-04 (read only, never `refresh=true`)
    SteamProfiles,
    /// API-05 bulk
    MatchMetadataBulk,
    /// API-05 single (always `disable_steam=true`)
    MatchMetadataSingle,
    /// API-06
    HeroBuildStats,
    /// API-07 to API-14, API-18 and other analytics endpoints, by ID.
    Analytics(&'static str),
    /// API-15 heroes, items, ranks
    Assets,
    /// API-15 client versions (the patch key)
    ClientVersions,
    /// API-16
    Builds,
    /// Broadcast locator, step 1 (DEC-049). Not cached: the listing changes every few seconds.
    LiveUrls,
    /// Broadcast locator, step 2 (DEC-049). Not cached here; the caller keeps the URL.
    LiveUrl,
}

impl Endpoint {
    pub const fn spec(self) -> EndpointSpec {
        let (id, scope, ttl) = match self {
            Self::MatchHistory => ("API-01", Scope::Players, Ttl::History),
            Self::PlayerRank => ("API-02", Scope::Rank, Ttl::History),
            Self::PlayerHeroStats => ("API-03", Scope::Players, Ttl::History),
            Self::SteamProfiles => ("API-04", Scope::Players, Ttl::History),
            Self::MatchMetadataBulk => ("API-05", Scope::MetadataBulk, Ttl::Metadata),
            Self::MatchMetadataSingle => ("API-05", Scope::Other, Ttl::Metadata),
            Self::HeroBuildStats => ("API-06", Scope::Analytics, Ttl::Analytics),
            Self::Analytics(id) => (id, Scope::Analytics, Ttl::Analytics),
            Self::Assets => ("API-15", Scope::Other, Ttl::Assets),
            Self::ClientVersions => ("API-15", Scope::Other, Ttl::ClientVersions),
            Self::Builds => ("API-16", Scope::Builds, Ttl::History),
            // No API-nn ID in doc 04 yet; the locator is DEC-049.
            Self::LiveUrls => ("LOC-1", Scope::LiveListing, Ttl::History),
            Self::LiveUrl => ("LOC-2", Scope::LiveUrl, Ttl::History),
        };
        EndpointSpec { id, scope, ttl }
    }
}

/// Settings that are not rules: where to call, who we are, an optional user key.
#[derive(Clone)]
pub struct ApiConfig {
    pub base_url: String,
    pub user_agent: String,
    /// The user's own key from the secret store. `None` means no key header.
    pub api_key: Option<String>,
    pub request_timeout: Duration,
    clock: Option<FakeRelativeClock>,
}

impl Default for ApiConfig {
    fn default() -> Self {
        Self {
            base_url: DEFAULT_BASE_URL.into(),
            user_agent: user_agent(),
            api_key: None,
            request_timeout: Duration::from_secs(30),
            clock: None,
        }
    }
}

impl ApiConfig {
    /// The defaults with another base URL (a mock server in tests).
    pub fn with_base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// Read the user's optional key from the secret store. A store error means no key.
    pub async fn with_key_from(mut self, store: &dyn SecretStore) -> Self {
        self.api_key = store
            .get(secret_keys::DEADLOCK_API_KEY)
            .await
            .ok()
            .flatten()
            .filter(|k| !k.trim().is_empty());
        self
    }

    /// Tests only: use a fake clock so limiter waits and cooldowns take no real time.
    #[cfg(test)]
    pub(crate) fn with_fake_clock(mut self, c: FakeRelativeClock) -> Self {
        self.clock = Some(c);
        self
    }
}

/// `stalemate/<version> (+https://github.com/HatchBear/Stalemate)`. Never includes the user's email.
pub fn user_agent() -> String {
    format!(
        "stalemate/{} (+https://github.com/HatchBear/Stalemate)",
        env!("CARGO_PKG_VERSION")
    )
}

struct Buckets {
    analytics: Bucket,
    players: Bucket,
    rank: Bucket,
    metadata_bulk: Bucket,
    builds: Bucket,
    other: Bucket,
    live_listing: Bucket,
    live_url: Bucket,
}

impl Buckets {
    fn new(r: &ApiRules, clock: &PaceClock) -> Self {
        let min = Duration::from_secs(60);
        let sec = Duration::from_secs(1);
        let b = |n, w| Bucket::new(n, w, clock.clone());
        Self {
            analytics: b(r.analytics_per_min, min),
            players: b(r.players_per_sec, sec),
            rank: b(r.rank_per_min, min),
            metadata_bulk: b(r.metadata_bulk_per_min, min),
            builds: b(r.players_per_sec, sec),
            other: b(r.other_per_min, min),
            live_listing: b(r.players_per_sec, sec),
            live_url: b(r.live_url_per_hour, Duration::from_secs(3600)),
        }
    }

    fn get(&self, s: Scope) -> &Bucket {
        match s {
            Scope::Analytics => &self.analytics,
            Scope::Players => &self.players,
            Scope::Rank => &self.rank,
            Scope::MetadataBulk => &self.metadata_bulk,
            Scope::Builds => &self.builds,
            Scope::Other => &self.other,
            Scope::LiveListing => &self.live_listing,
            Scope::LiveUrl => &self.live_url,
        }
    }
}

/// A same-query block after a 503, 429 or 504.
#[derive(Clone, Copy)]
struct Block {
    until: Nanos,
    timeout: bool,
}

pub struct ApiClient {
    http: reqwest::Client,
    base_url: String,
    api_key: Option<reqwest::header::HeaderValue>,
    rules: ApiRules,
    db: Storage,
    clock: PaceClock,
    buckets: Buckets,
    patch: RwLock<Option<String>>,
    blocked: Mutex<HashMap<String, Block>>,
    no_data: Mutex<HashSet<String>>,
    rate_limit_waits: AtomicU64,
}

impl ApiClient {
    pub fn new(db: Storage, rules: &ApiRules, cfg: ApiConfig) -> Result<Self, ApiError> {
        let clock = cfg
            .clock
            .clone()
            .map_or_else(PaceClock::real, PaceClock::Fake);
        let api_key = cfg
            .api_key
            .as_deref()
            .map(|k| {
                let mut v = reqwest::header::HeaderValue::from_str(k.trim()).map_err(|_| {
                    ApiError::Invalid("the API key has characters a header can't carry".into())
                })?;
                v.set_sensitive(true);
                Ok::<_, ApiError>(v)
            })
            .transpose()?;
        Ok(Self {
            http: reqwest::Client::builder()
                .user_agent(cfg.user_agent)
                .timeout(cfg.request_timeout)
                .build()?,
            base_url: cfg.base_url.trim_end_matches('/').to_string(),
            api_key,
            rules: rules.clone(),
            db,
            buckets: Buckets::new(rules, &clock),
            clock,
            patch: RwLock::new(None),
            blocked: Mutex::new(HashMap::new()),
            no_data: Mutex::new(HashSet::new()),
            rate_limit_waits: AtomicU64::new(0),
        })
    }

    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// Cumulative count, since this client was made, of times a request was held back by rate
    /// limiting: it waited for a token bucket, was refused because the same query was cooling down
    /// after a `Retry-After`/504, was refused by the per-match locator budget, or the server answered
    /// 429/503/504. A request counts once per cause. Cheap; for diagnostics (FR-SET-005).
    pub fn rate_limit_waits(&self) -> u64 {
        self.rate_limit_waits.load(Ordering::Relaxed)
    }

    fn count_wait(&self) {
        self.rate_limit_waits.fetch_add(1, Ordering::Relaxed);
    }

    pub fn storage(&self) -> &Storage {
        &self.db
    }

    /// The patch key last set by [`Self::current_patch`] (or [`Self::set_patch`]).
    pub fn patch(&self) -> Option<String> {
        self.patch.read().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_patch(&self, patch: &str) {
        *self.patch.write().unwrap_or_else(|e| e.into_inner()) = Some(patch.to_string());
    }

    fn ttl_s(&self, t: Ttl) -> i64 {
        match t {
            Ttl::History => self.rules.history_ttl_s,
            Ttl::Analytics => self.rules.analytics_ttl_s,
            Ttl::Metadata => self.rules.metadata_ttl_s,
            Ttl::Assets => self.rules.assets_ttl_s,
            Ttl::ClientVersions => self.rules.client_versions_ttl_s,
        }
    }

    // -----------------------------------------------------------------------------------------
    // The one request path
    // -----------------------------------------------------------------------------------------

    /// Send one GET through the guard, the scope bucket and the status mapping. No cache. No retry.
    pub async fn fetch(
        &self,
        ep: Endpoint,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<Vec<u8>, ApiError> {
        if let Some((k, _)) = query.iter().find(|(k, _)| FORBIDDEN_PARAMS.contains(k)) {
            return Err(ApiError::ForbiddenParam((*k).to_string()));
        }
        let spec = ep.spec();
        let key = cache_key("GET", path, query);

        if self
            .no_data
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&key)
        {
            return Err(ApiError::NoPublicData);
        }
        if let Some(b) = self
            .blocked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(&key)
            .copied()
        {
            let now = self.clock.now();
            if now < b.until {
                self.count_wait();
                let left = Duration::from(b.until.saturating_sub(now));
                return Err(ApiError::Busy {
                    retry_after_s: left.as_secs() + u64::from(left.subsec_nanos() > 0),
                    timeout: b.timeout,
                });
            }
        }

        if spec.scope == Scope::LiveUrl {
            // Honour the hourly budget without sleeping through it.
            if let Err(wait) = self.buckets.get(spec.scope).try_acquire() {
                self.count_wait();
                return Err(ApiError::Busy {
                    retry_after_s: wait.as_secs() + u64::from(wait.subsec_nanos() > 0),
                    timeout: false,
                });
            }
        } else {
            if self.buckets.get(spec.scope).acquire().await {
                self.count_wait();
            }
        }

        // reqwest's `query` helper is behind a feature this crate doesn't enable; build the URL here.
        let mut url = url::Url::parse(&format!("{}{path}", self.base_url))
            .map_err(|e| ApiError::Invalid(format!("url: {e}")))?;
        if !query.is_empty() {
            url.query_pairs_mut()
                .extend_pairs(query.iter().map(|(k, v)| (*k, v.as_str())));
        }
        let mut req = self.http.get(url);
        if let Some(k) = &self.api_key {
            req = req.header("X-API-KEY", k.clone());
        }
        let resp = req.send().await?;
        let status = resp.status().as_u16();
        let retry_after = resp
            .headers()
            .get(reqwest::header::RETRY_AFTER)
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.trim().parse::<u64>().ok())
            .unwrap_or(self.rules.busy_retry_default_s);
        match status {
            200 => Ok(resp.bytes().await?.to_vec()),
            404 => {
                self.no_data
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .insert(key);
                Err(ApiError::NoPublicData)
            }
            503 | 429 => {
                self.count_wait();
                self.block(&key, retry_after, false);
                Err(ApiError::Busy {
                    retry_after_s: retry_after,
                    timeout: false,
                })
            }
            504 => {
                self.count_wait();
                // No same-query retry for the cooldown, even when Retry-After says less.
                let wait = retry_after.max(self.rules.timeout_cooldown_s);
                self.block(&key, wait, true);
                Err(ApiError::Busy {
                    retry_after_s: wait,
                    timeout: true,
                })
            }
            s => Err(ApiError::Http(s)),
        }
    }

    fn block(&self, key: &str, secs: u64, timeout: bool) {
        let until = self.clock.now() + Nanos::from(Duration::from_secs(secs));
        self.blocked
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.to_string(), Block { until, timeout });
    }

    /// GET with the `api_cache` layer. A fresh cached body means no network. A fresh cached 404
    /// answers `NoPublicData` without a request (TST-032).
    pub async fn get_cached<T: DeserializeOwned>(
        &self,
        ep: Endpoint,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        let spec = ep.spec();
        let ttl = self.ttl_s(spec.ttl);
        let key = cache_key("GET", path, query);
        let patch = self.patch();
        if let Some(hit) = self.db.cache_get(&key).ok().flatten() {
            if hit.is_fresh(ttl, patch.as_deref()) {
                if hit.http_status == 404 {
                    return Err(ApiError::NoPublicData);
                }
                if let Ok(v) = serde_json::from_slice(&hit.body) {
                    return Ok(v);
                }
                // A body that no longer decodes (an old schema) is refetched.
            }
        }
        match self.fetch(ep, path, query).await {
            Ok(body) => {
                let v = serde_json::from_slice(&body)?;
                let _ = self
                    .db
                    .cache_put(&key, spec.id, ttl, patch.as_deref(), 200, &body);
                Ok(v)
            }
            Err(ApiError::NoPublicData) => {
                // A remembered 404 uses the history TTL when the endpoint's TTL is "until patch".
                let t = if ttl < 0 {
                    self.rules.history_ttl_s
                } else {
                    ttl
                };
                let _ = self
                    .db
                    .cache_put(&key, spec.id, t, patch.as_deref(), 404, b"");
                Err(ApiError::NoPublicData)
            }
            Err(e) => Err(e),
        }
    }

    // -----------------------------------------------------------------------------------------
    // Typed endpoints
    // -----------------------------------------------------------------------------------------

    /// API-01. Stored history only: `force_refetch` is never sent.
    pub async fn match_history(
        &self,
        account_id: AccountId,
    ) -> Result<Vec<MatchHistoryEntry>, ApiError> {
        self.get_cached(
            Endpoint::MatchHistory,
            &format!("/v1/players/{account_id}/match-history"),
            &[],
        )
        .await
    }

    /// API-02 current rank, batch. Accounts the API leaves out (protected accounts) are absent.
    pub async fn player_ranks(
        &self,
        account_ids: &[AccountId],
    ) -> Result<Vec<AccountRank>, ApiError> {
        self.get_cached(
            Endpoint::PlayerRank,
            "/v1/players/rank",
            &[("account_ids", csv(account_ids)?)],
        )
        .await
    }

    /// API-03, batch.
    pub async fn player_hero_stats(
        &self,
        account_ids: &[AccountId],
    ) -> Result<Vec<PlayerHeroStats>, ApiError> {
        self.get_cached(
            Endpoint::PlayerHeroStats,
            "/v1/players/hero-stats",
            &[("account_ids", csv(account_ids)?)],
        )
        .await
    }

    /// API-04, batch, read only. Never `refresh=true` (NFR-FAIR-005).
    pub async fn steam_profiles(
        &self,
        account_ids: &[AccountId],
    ) -> Result<Vec<SteamProfile>, ApiError> {
        self.get_cached(
            Endpoint::SteamProfiles,
            "/v1/players/steam",
            &[("account_ids", csv(account_ids)?)],
        )
        .await
    }

    /// API-06 for one hero. `account_ids` narrows it to those players. `extra` adds filters such as
    /// `min_matches`; the forbidden-parameter guard still applies.
    pub async fn hero_build_stats(
        &self,
        hero_id: HeroId,
        account_ids: &[AccountId],
        extra: &[(&str, String)],
    ) -> Result<Vec<HeroBuildStats>, ApiError> {
        let mut q: Vec<(&str, String)> = extra.to_vec();
        if !account_ids.is_empty() {
            q.push(("account_ids", csv(account_ids)?));
        }
        self.get_cached(
            Endpoint::HeroBuildStats,
            &format!("/v1/analytics/hero-build-stats/{hero_id}"),
            &q,
        )
        .await
    }

    /// Any other analytics endpoint (API-07 to API-14), by its doc 04 ID. Shares the analytics bucket
    /// and the 24 h TTL.
    pub async fn get_analytics<T: DeserializeOwned>(
        &self,
        endpoint_id: &'static str,
        path: &str,
        query: &[(&str, String)],
    ) -> Result<T, ApiError> {
        self.get_cached(Endpoint::Analytics(endpoint_id), path, query)
            .await
    }

    /// API-05 bulk. Each match is cached on its own (matches never change), so only matches not
    /// seen before are requested, in as few calls as the spec's 1000 IDs per call allow.
    /// Matches the API doesn't return are absent from the result. Order follows `match_ids`.
    pub async fn match_metadata_bulk(
        &self,
        match_ids: &[MatchId],
        opts: &MetadataOptions,
    ) -> Result<Vec<MatchMetadata>, ApiError> {
        let spec = Endpoint::MatchMetadataBulk.spec();
        let ttl = self.ttl_s(spec.ttl);
        let fp = opts.fingerprint();
        let key_for = |id: MatchId| {
            cache_key(
                "GET",
                "/v1/matches/metadata#match",
                &[("match_id", id.to_string()), ("opts", fp.clone())],
            )
        };
        let mut found: HashMap<MatchId, MatchMetadata> = HashMap::new();
        let mut missing: Vec<MatchId> = Vec::new();
        for &id in match_ids {
            if found.contains_key(&id) || missing.contains(&id) {
                continue;
            }
            let hit = self
                .db
                .cache_get(&key_for(id))
                .ok()
                .flatten()
                .filter(|h| h.is_fresh(ttl, None) && h.http_status == 200)
                .and_then(|h| serde_json::from_slice::<MatchMetadata>(&h.body).ok());
            match hit {
                Some(m) => {
                    found.insert(id, m);
                }
                None => missing.push(id),
            }
        }
        for chunk in missing.chunks(MAX_BATCH_IDS) {
            let mut q = opts.query();
            q.push((
                "match_ids",
                chunk
                    .iter()
                    .map(u64::to_string)
                    .collect::<Vec<_>>()
                    .join(","),
            ));
            q.push(("limit", chunk.len().to_string()));
            let body = self
                .fetch(Endpoint::MatchMetadataBulk, "/v1/matches/metadata", &q)
                .await?;
            let raw: Vec<serde_json::Value> = serde_json::from_slice(&body)?;
            for v in raw {
                let one = serde_json::to_vec(&v)?;
                let m: MatchMetadata = serde_json::from_value(v)?;
                let _ = self
                    .db
                    .cache_put(&key_for(m.match_id), spec.id, ttl, None, 200, &one);
                found.insert(m.match_id, m);
            }
        }
        Ok(match_ids.iter().filter_map(|id| found.remove(id)).collect())
    }

    /// API-05 single. Always `disable_steam=true`, so deadlock-api never goes to Valve for it.
    pub async fn match_metadata_single(
        &self,
        match_id: MatchId,
    ) -> Result<SingleMatchMetadata, ApiError> {
        self.get_cached(
            Endpoint::MatchMetadataSingle,
            &format!("/v1/matches/{match_id}/metadata"),
            &[("disable_steam", "true".to_string())],
        )
        .await
    }

    /// API-16 listing for a hero. `sort_by` is an API value such as `favorites` or
    /// `weekly_favorites`. Only the latest version of each build is returned.
    pub async fn community_builds(
        &self,
        hero_id: HeroId,
        sort_by: &str,
        limit: u32,
    ) -> Result<Vec<CommunityBuild>, ApiError> {
        self.get_cached(
            Endpoint::Builds,
            "/v1/builds",
            &[
                ("hero_id", hero_id.to_string()),
                ("sort_by", sort_by.to_string()),
                ("only_latest", "true".to_string()),
                ("limit", limit.to_string()),
            ],
        )
        .await
    }

    /// API-16 by build ID, through the search (`/v1/builds/{hero_id}/{build_id}` can make
    /// deadlock-api ask Valve). Returns the newest version, or `None` if there is no such build.
    pub async fn community_build(
        &self,
        hero_build_id: u32,
    ) -> Result<Option<CommunityBuild>, ApiError> {
        let v: Vec<CommunityBuild> = self
            .get_cached(
                Endpoint::Builds,
                "/v1/builds",
                &[
                    ("build_id", hero_build_id.to_string()),
                    ("only_latest", "true".to_string()),
                ],
            )
            .await?;
        Ok(v.into_iter()
            .filter(|b| b.hero_build.hero_build_id == hero_build_id)
            .max_by_key(|b| b.hero_build.version))
    }

    // -----------------------------------------------------------------------------------------
    // Broadcast locator (DEC-049, FR-DATA-003)
    // -----------------------------------------------------------------------------------------

    /// `GET /v1/matches/live/urls`: every Broadcast currently listed. Free (100 req/s), so the
    /// locator tries it first. Not cached.
    pub async fn live_urls(&self) -> Result<Vec<LiveUrl>, ApiError> {
        let body = self
            .fetch(Endpoint::LiveUrls, "/v1/matches/live/urls", &[])
            .await?;
        Ok(serde_json::from_slice(&body)?)
    }

    /// `GET /v1/matches/{match_id}/live/url`: makes deadlock-api spectate the match and return its
    /// Broadcast URL. 6 req/h per IP, so the bucket (`live_url_per_hour`) is checked without
    /// waiting: over budget gives [`ApiError::Busy`]. A 404 is [`ApiError::NoPublicData`] and is
    /// remembered, so a failed lookup is never repeated. Call it only through
    /// [`crate::broadcast::Locator`], which tries the listing first and keeps the URL.
    pub async fn live_url_for_match(&self, match_id: MatchId) -> Result<String, ApiError> {
        let body = self
            .fetch(
                Endpoint::LiveUrl,
                &format!("/v1/matches/{match_id}/live/url"),
                &[],
            )
            .await?;
        Ok(serde_json::from_slice::<MatchSpectate>(&body)?.broadcast_url)
    }

    // -----------------------------------------------------------------------------------------
    // Assets (API-15)
    // -----------------------------------------------------------------------------------------

    /// The patch key: the newest value of `/v1/assets/client-versions`. Sets [`Self::patch`].
    pub async fn current_patch(&self) -> Result<String, ApiError> {
        let versions: Vec<u32> = self
            .get_cached(Endpoint::ClientVersions, "/v1/assets/client-versions", &[])
            .await?;
        let newest = versions
            .into_iter()
            .max()
            .ok_or_else(|| ApiError::Invalid("client-versions was empty".into()))?
            .to_string();
        self.set_patch(&newest);
        Ok(newest)
    }

    /// Heroes and items for the current patch, from `api_cache` when that patch is already stored.
    /// Calling this twice in one patch makes no asset requests the second time.
    pub async fn load_catalog(&self) -> Result<AssetCatalog, ApiError> {
        let patch = self.current_patch().await?;
        let spec = Endpoint::Assets.spec();
        let ttl = self.ttl_s(spec.ttl);
        let key = cache_key(
            "GET",
            "/v1/assets/catalog",
            &[("client_version", patch.clone())],
        );
        if let Some(hit) = self.db.cache_get(&key).ok().flatten() {
            if hit.is_fresh(ttl, Some(&patch)) {
                if let Ok(d) = serde_json::from_slice::<assets::CatalogData>(&hit.body) {
                    return Ok(AssetCatalog::from_data(d));
                }
            }
        }
        let q = [("client_version", patch.clone())];
        let heroes = assets::parse_heroes(
            &self
                .fetch(Endpoint::Assets, "/v1/assets/heroes", &q)
                .await?,
        )?;
        let items =
            assets::parse_items(&self.fetch(Endpoint::Assets, "/v1/assets/items", &q).await?)?;
        let cat = AssetCatalog::from_parts(patch.clone(), heroes, items);
        let _ = self.db.cache_put(
            &key,
            spec.id,
            ttl,
            Some(&patch),
            200,
            &serde_json::to_vec(&cat.to_data())?,
        );
        // Older catalogs are no longer valid for the new patch.
        let _ = self.db.purge_expired(Some(&patch));
        Ok(cat)
    }

    /// Download one image into `<data>/assets/<patch>/...` if it isn't there yet. The CDN is not
    /// the API, so the key and the API buckets don't apply. Returns the file path.
    pub async fn ensure_image(
        &self,
        dirs: &AssetDirs,
        patch: &str,
        kind: ImageKind,
        id: u32,
        url: &str,
    ) -> Result<PathBuf, ApiError> {
        let path = dirs.image_path(patch, kind, id, url);
        if path.is_file() {
            return Ok(path);
        }
        let parsed =
            url::Url::parse(url).map_err(|e| ApiError::Invalid(format!("image url: {e}")))?;
        if !matches!(parsed.scheme(), "http" | "https") {
            return Err(ApiError::Invalid("image url must be http or https".into()));
        }
        let resp = self.http.get(parsed).send().await?;
        let status = resp.status().as_u16();
        if status != 200 {
            return Err(ApiError::Http(status));
        }
        let bytes = resp.bytes().await?;
        if let Some(dir) = path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        let tmp = path.with_extension("part");
        tokio::fs::write(&tmp, &bytes).await?;
        tokio::fs::rename(&tmp, &path).await?;
        Ok(path)
    }

    /// Download every hero card, hero icon and shop item image missing for the catalog's patch, one
    /// at a time. A failed image is skipped and counted, never fatal. Returns `(downloaded_or_present, failed)`.
    pub async fn download_catalog_images(
        &self,
        catalog: &AssetCatalog,
        dirs: &AssetDirs,
    ) -> (usize, usize) {
        let patch = catalog.patch();
        let mut jobs: Vec<(ImageKind, u32, &str)> = Vec::new();
        for h in catalog.heroes().iter().filter(|h| h.player_selectable) {
            if let Some(u) = &h.card_image {
                jobs.push((ImageKind::HeroCard, h.id, u));
            }
            if let Some(u) = &h.small_image {
                jobs.push((ImageKind::HeroSmall, h.id, u));
            }
        }
        for i in catalog
            .items()
            .iter()
            .filter(|i| i.kind == "upgrade" && i.shopable)
        {
            if let Some(u) = i.shop_image.as_ref().or(i.image.as_ref()) {
                jobs.push((ImageKind::Item, i.id, u));
            }
        }
        let (mut ok, mut failed) = (0, 0);
        for (kind, id, url) in jobs {
            match self.ensure_image(dirs, patch, kind, id, url).await {
                Ok(_) => ok += 1,
                Err(e) => {
                    tracing::warn!("image {kind:?} {id} failed: {e}");
                    failed += 1;
                }
            }
        }
        (ok, failed)
    }
}

/// Comma-joined IDs for a batch parameter, sorted and de-duplicated so the same set always gives
/// the same cache key.
fn csv(ids: &[AccountId]) -> Result<String, ApiError> {
    if ids.is_empty() {
        return Err(ApiError::Invalid(
            "at least one account id is required".into(),
        ));
    }
    let mut v = ids.to_vec();
    v.sort_unstable();
    v.dedup();
    if v.len() > MAX_BATCH_IDS {
        return Err(ApiError::Invalid(format!(
            "at most {MAX_BATCH_IDS} ids per call"
        )));
    }
    Ok(v.iter().map(u32::to_string).collect::<Vec<_>>().join(","))
}

#[cfg(test)]
mod tests;
