//! M-20 Settings and secrets (PRG-25). See 10-technical-architecture.md and 14-data-and-content-assets.md.
//!
//! - `Rules`: typed DATA-001 (`config/rules.toml`). Every field is required, so a missing value
//!   fails at load time instead of silently using a made-up default (NFR-MAINT-002).
//! - `Settings`: user settings (DATA-007 defaults, overridden by the user's `settings.toml`).
//! - `SecretStore`: keys never go into TOML.

use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Where the Flatpak installs the shipped config (doc 10).
const FLATPAK_SHARE_DIR: &str = "/app/share/stalemate";
/// Environment variable that overrides the shipped config location.
pub const SHARE_DIR_ENV: &str = "STALEMATE_SHARE_DIR";
const APP_DIR: &str = "stalemate";
const RULES_FILE: &str = "rules.toml";
const DEFAULTS_FILE: &str = "settings.default.toml";
const USER_FILE: &str = "settings.toml";

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("could not read {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("could not parse {path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("could not write settings: {0}")]
    Write(String),
    #[error("no shipped config found; looked for config/{RULES_FILE} under: {}", .tried.join(", "))]
    NoShareDir { tried: Vec<String> },
    #[error("could not find the user's config or data directory")]
    NoUserDirs,
}

pub type Result<T> = std::result::Result<T, SettingsError>;

// ---------------------------------------------------------------------------------------------
// DATA-001 rules
// ---------------------------------------------------------------------------------------------

/// DATA-001 `rules.toml`. Field names equal the TOML keys.
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Rules {
    pub match_watcher: MatchWatcherRules,
    pub api: ApiRules,
    pub broadcast: BroadcastRules,
    pub scouting: ScoutingRules,
    pub build_matcher: BuildMatcherRules,
    pub projector: ProjectorRules,
    pub counter: CounterRules,
    pub build_check: BuildCheckRules,
    pub pace: PaceRules,
    pub review: ReviewRules,
    pub ui: UiRules,
    pub builds: BuildsRules,
    pub diagnostics: DiagnosticsRules,
    pub overlay: OverlayRules,
    pub retention: RetentionRules,
    pub updates: UpdateRules,
}

/// `[diagnostics]`: the Alpha diagnostics file (FR-SET-005, DEC-070).
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsRules {
    /// Collecting is on at first run in Alpha builds.
    pub enabled_default: bool,
    /// Days of diagnostics kept on disk.
    pub keep_days: u32,
    /// Seconds between CPU and memory samples during Live.
    pub perf_sample_s: u32,
    /// Size cap of the saved zip, in megabytes.
    pub max_file_mb: u32,
}

/// Regexes for the `console.log` lines (OQ-03, T-006). An empty string means "not configured".
#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MatchWatcherRules {
    pub start: String,
    pub end: String,
    pub match_id: String,
    pub exit: String,
    /// Fallback poll interval in seconds when `notify` misses an event.
    pub poll_s: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ApiRules {
    pub analytics_per_min: u32,
    pub players_per_sec: u32,
    pub other_per_min: u32,
    pub history_ttl_s: i64,
    pub analytics_ttl_s: i64,
    // Added by T-009 (pending doc 14 update).
    pub metadata_bulk_per_min: u32,
    pub rank_per_min: u32,
    pub timeout_cooldown_s: u64,
    pub busy_retry_default_s: u64,
    pub assets_ttl_s: i64,
    pub client_versions_ttl_s: i64,
    pub metadata_ttl_s: i64,
    /// Broadcast locator, per-match endpoint: 6 req/h per IP published (DEC-049).
    pub live_url_per_hour: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BroadcastRules {
    pub backoff_s: Vec<u64>,
    pub stale_delay_s: u32,
    /// Timeout for each request to the relay, seconds.
    pub request_timeout_s: u64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ScoutingRules {
    pub swap_closed_after_s: u32,
    pub history_window: u32,
    pub mmr_window: u32,
    /// Fewer ranked matches in the window shows "not enough data" (DEC-061).
    pub mmr_trend_min_points: u32,
    /// Heroes listed as mains on the scouting card (FR-SCOUT-003, DEC-065).
    pub mains_shown: u32,
    pub build_sample_matches: u32,
    pub low_sample_below: u32,
    pub core_rate: f64,
    pub core_max_cond_gap: f64,
    pub counter_mult: f64,
    pub counter_lift: f64,
    pub situational_rate: f64,
    pub min_enemy_matches: u32,
    pub smurf_max_lifetime: u32,
    pub smurf_min_last20_wr: f64,
    pub smurf_mmr_slope_pct: f64,
    pub session_gap_min: u32,
    pub session_loss_streak: u32,
    pub session_hours: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildMatcherRules {
    pub candidates: u32,
    pub p_in: f64,
    pub p_out: f64,
    pub unclear_below: f64,
    pub prior_player_weight: f64,
    pub min_player_matches: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectorRules {
    pub order_weight: f64,
    pub transition_weight: f64,
    pub order_step: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CounterRules {
    pub lane_phase_end_s: u32,
    pub min_matches: u32,
    pub refresh_min_s: u32,
    pub projected_value_weight: f64,
    pub affordability_income_s: u32,
    pub top_window: u32,
    pub top_overlay: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildCheckRules {
    pub margin: f64,
    pub max_swaps: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PaceRules {
    pub timing_tolerance_s: u32,
    pub timer_lead_s: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ObjWeights {
    pub guardian: u32,
    pub walker: u32,
    pub base: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRules {
    /// Seconds to wait before each attempt to fetch API-05 after a match ends. The API needs a few
    /// minutes to ingest the match; the last value repeats until the app gives up.
    pub api_poll_s: Vec<u64>,
    pub min_match_s: u32,
    pub sample_every_s: u32,
    pub accuracy_k: u32,
    /// T1..T5 upper bounds; above the last is T6.
    pub tiers: Vec<f64>,
    pub t7_low: f64,
    pub t7_even: [f64; 2],
    pub t7_high: f64,
    pub t7_min_share: f64,
    pub t8_min_gain: f64,
    pub t9_enemies_dead: u32,
    pub t9_window_s: u32,
    pub key_moments: u32,
    pub dedupe_priority: Vec<String>,
    pub obj_weights: ObjWeights,
    pub r01_unused_s: u32,
    pub r02_lane_deficit: u32,
    pub r03_camp_gap_pp: f64,
    pub r04_late_s: u32,
    pub r05_min_score: f64,
    pub r05_window_s: u32,
    pub r06_creep_share: f64,
    pub r07_hs_gap_pp: f64,
    pub r08_offplan_count: u32,
    pub r09_window_s: u32,
    pub r10_absent_count: u32,
    pub r11_parry_gap_pp: f64,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UiRules {
    pub flush_ms: u32,
    pub item_search_results: u32,
    pub secret_timeout_s: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct BuildsRules {
    pub community_limit: u32,
    pub community_sort: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayRules {
    pub max_redraw_hz: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RetentionRules {
    pub recordings_keep_matches: u32,
    pub recordings_keep_days: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateRules {
    /// FR-SET-004.
    pub check_on_start: bool,
}

impl Rules {
    pub fn from_toml_str(text: &str, origin: &Path) -> Result<Self> {
        toml::from_str(text).map_err(|e| SettingsError::Parse {
            path: origin.to_path_buf(),
            message: e.to_string(),
        })
    }

    pub fn load(path: &Path) -> Result<Self> {
        Self::from_toml_str(&read(path)?, path)
    }
}

// ---------------------------------------------------------------------------------------------
// User settings (DATA-007)
// ---------------------------------------------------------------------------------------------

/// FR-SET-001. Serialized names match the `reviews.llm_provider` column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmProvider {
    Off,
    Local,
    OwnKey,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct AccountSettings {
    /// FR-DATA-006. Empty = detect from Steam.
    pub manual_account: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SteamSettings {
    /// NFR-PLAT-002.
    pub extra_library_paths: Vec<PathBuf>,
    /// FR-SET-003.
    pub deadlock_app_id: u32,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LlmSettings {
    /// FR-SET-001, DEC-030.
    pub llm_provider: LlmProvider,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub account: AccountSettings,
    pub steam: SteamSettings,
    pub llm: LlmSettings,
    /// FR-SET-005, DEC-070. Absent in older files.
    #[serde(default)]
    pub diagnostics: DiagnosticsSettings,
}

/// "Save diagnostics while I play" (FR-SET-005, DEC-070).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct DiagnosticsSettings {
    /// `None` means "use `rules.diagnostics.enabled_default`". Only an explicit choice is saved.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enabled: Option<bool>,
}

fn merge(base: &mut toml::Value, over: toml::Value) {
    match (base, over) {
        (toml::Value::Table(b), toml::Value::Table(o)) => {
            for (k, v) in o {
                match b.get_mut(&k) {
                    Some(slot) => merge(slot, v),
                    None => {
                        b.insert(k, v);
                    }
                }
            }
        }
        (slot, v) => *slot = v,
    }
}

/// Remove from `v` every entry equal to the one in `base`, so the saved file holds only what the user changed.
fn strip_defaults(v: &mut toml::Value, base: &toml::Value) {
    if let (toml::Value::Table(t), toml::Value::Table(b)) = (&mut *v, base) {
        let keys: Vec<String> = t.keys().cloned().collect();
        for k in keys {
            if let Some(bv) = b.get(&k) {
                if t[&k] == *bv {
                    t.remove(&k);
                } else if let Some(child) = t.get_mut(&k) {
                    strip_defaults(child, bv);
                    if matches!(child, toml::Value::Table(c) if c.is_empty()) {
                        t.remove(&k);
                    }
                }
            }
        }
    }
}

fn parse_value(text: &str, origin: &Path) -> Result<toml::Value> {
    toml::from_str(text).map_err(|e| SettingsError::Parse {
        path: origin.to_path_buf(),
        message: e.to_string(),
    })
}

impl Settings {
    /// Whether diagnostics are on: the user's choice, else `rules.diagnostics.enabled_default`.
    pub fn diagnostics_enabled(&self, rules: &Rules) -> bool {
        self.diagnostics
            .enabled
            .unwrap_or(rules.diagnostics.enabled_default)
    }

    /// Defaults from `defaults_path`, with the user's `user_path` (if it exists) merged over them.
    pub fn load_from(defaults_path: &Path, user_path: &Path) -> Result<Self> {
        let mut merged = parse_value(&read(defaults_path)?, defaults_path)?;
        if user_path.exists() {
            merged_over(&mut merged, user_path)?;
        }
        merged
            .try_into()
            .map_err(|e: toml::de::Error| SettingsError::Parse {
                path: user_path.to_path_buf(),
                message: e.to_string(),
            })
    }

    /// Write `settings.toml` in `config_dir`. Only values that differ from the defaults are written.
    /// Secrets never reach this struct, so they cannot reach the file (TC-25-1).
    pub fn save(&self, defaults_path: &Path, config_dir: &Path) -> Result<()> {
        let base = parse_value(&read(defaults_path)?, defaults_path)?;
        let mut mine =
            toml::Value::try_from(self).map_err(|e| SettingsError::Write(e.to_string()))?;
        strip_defaults(&mut mine, &base);
        let text =
            toml::to_string_pretty(&mine).map_err(|e| SettingsError::Write(e.to_string()))?;
        let path = config_dir.join(USER_FILE);
        std::fs::create_dir_all(config_dir).map_err(|source| SettingsError::Io {
            path: config_dir.to_path_buf(),
            source,
        })?;
        // Write to a temp file and rename, so a crash never leaves half a file.
        let tmp = config_dir.join(format!("{USER_FILE}.tmp"));
        std::fs::write(&tmp, text)
            .and_then(|_| std::fs::rename(&tmp, &path))
            .map_err(|source| SettingsError::Io { path, source })
    }
}

fn merged_over(base: &mut toml::Value, user_path: &Path) -> Result<()> {
    let over = parse_value(&read(user_path)?, user_path)?;
    merge(base, over);
    Ok(())
}

fn read(path: &Path) -> Result<String> {
    std::fs::read_to_string(path).map_err(|source| SettingsError::Io {
        path: path.to_path_buf(),
        source,
    })
}

// ---------------------------------------------------------------------------------------------
// Locations and load()
// ---------------------------------------------------------------------------------------------

/// Where things live (doc 10, "File locations").
#[derive(Debug, Clone, PartialEq)]
pub struct Paths {
    /// Shipped config: contains `rules.toml` and `settings.default.toml`.
    pub shipped_config_dir: PathBuf,
    /// User config: `settings.toml`.
    pub config_dir: PathBuf,
    /// Database, recordings, caches, logs.
    pub data_dir: PathBuf,
}

impl Paths {
    pub fn detect() -> Result<Self> {
        let config_dir = dirs::config_dir()
            .ok_or(SettingsError::NoUserDirs)?
            .join(APP_DIR);
        // Linux (including the Flatpak, where XDG_DATA_HOME points into ~/.var/app/<id>/data):
        // data_dir. Windows: %LOCALAPPDATA%, which is data_local_dir.
        #[cfg(windows)]
        let data_base = dirs::data_local_dir();
        #[cfg(not(windows))]
        let data_base = dirs::data_dir();
        let data_dir = data_base.ok_or(SettingsError::NoUserDirs)?.join(APP_DIR);
        Ok(Self {
            shipped_config_dir: find_shipped_config_dir()?,
            config_dir,
            data_dir,
        })
    }

    pub fn rules_path(&self) -> PathBuf {
        self.shipped_config_dir.join(RULES_FILE)
    }
    pub fn defaults_path(&self) -> PathBuf {
        self.shipped_config_dir.join(DEFAULTS_FILE)
    }
    pub fn user_settings_path(&self) -> PathBuf {
        self.config_dir.join(USER_FILE)
    }
    pub fn db_path(&self) -> PathBuf {
        self.data_dir.join("app.db")
    }
    /// Raw Broadcast recordings, `<data>/recordings/<match_id>.bcast` (FR-DATA-005).
    pub fn recordings_dir(&self) -> PathBuf {
        self.data_dir.join("recordings")
    }
}

/// Shipped config lookup order: `STALEMATE_SHARE_DIR`, `/app/share/stalemate` (Flatpak), next to the
/// executable, then the repository root in dev builds. The first one with `config/rules.toml` wins;
/// the returned path is that directory's `config` folder.
fn find_shipped_config_dir() -> Result<PathBuf> {
    let mut roots: Vec<PathBuf> = Vec::new();
    if let Some(v) = std::env::var_os(SHARE_DIR_ENV) {
        roots.push(PathBuf::from(v));
    }
    roots.push(PathBuf::from(FLATPAK_SHARE_DIR));
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(Path::to_path_buf))
    {
        roots.push(dir);
    }
    if cfg!(debug_assertions) {
        // crates/core -> repo root
        roots.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../.."));
    }
    pick_config_dir(&roots)
}

fn pick_config_dir(roots: &[PathBuf]) -> Result<PathBuf> {
    roots
        .iter()
        .map(|r| r.join("config"))
        .find(|c| c.join(RULES_FILE).is_file())
        .ok_or_else(|| SettingsError::NoShareDir {
            tried: roots.iter().map(|r| r.display().to_string()).collect(),
        })
}

/// Everything the app reads at startup.
#[derive(Debug, Clone)]
pub struct Config {
    pub paths: Paths,
    pub rules: Rules,
    pub settings: Settings,
}

impl Config {
    pub fn load_from(paths: Paths) -> Result<Self> {
        let rules = Rules::load(&paths.rules_path())?;
        let settings = Settings::load_from(&paths.defaults_path(), &paths.user_settings_path())?;
        Ok(Self {
            paths,
            rules,
            settings,
        })
    }

    pub fn save_settings(&self) -> Result<()> {
        self.settings
            .save(&self.paths.defaults_path(), &self.paths.config_dir)
    }
}

/// Detect locations, read DATA-001 and the settings (defaults, then the user's file).
pub fn load() -> Result<Config> {
    Config::load_from(Paths::detect()?)
}

// ---------------------------------------------------------------------------------------------
// Secrets
// ---------------------------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("secret store error: {0}")]
    Backend(String),
}

/// Well-known secret names. Secrets never go into TOML.
pub mod secret_keys {
    pub const DEADLOCK_API_KEY: &str = "deadlock_api_key";
    pub const LLM_OWN_KEY: &str = "llm_own_key";
}

#[async_trait::async_trait]
pub trait SecretStore: Send + Sync {
    async fn set(&self, key: &str, value: &str) -> std::result::Result<(), SecretError>;
    async fn get(&self, key: &str) -> std::result::Result<Option<String>, SecretError>;
    /// Deleting a key that doesn't exist is not an error.
    async fn delete(&self, key: &str) -> std::result::Result<(), SecretError>;
}

/// For tests, and as a fallback when no keyring is available. Lives in memory only.
#[derive(Default)]
pub struct MemorySecretStore {
    map: Mutex<HashMap<String, String>>,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[async_trait::async_trait]
impl SecretStore for MemorySecretStore {
    async fn set(&self, key: &str, value: &str) -> std::result::Result<(), SecretError> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key.into(), value.into());
        Ok(())
    }
    async fn get(&self, key: &str) -> std::result::Result<Option<String>, SecretError> {
        Ok(self
            .map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(key)
            .cloned())
    }
    async fn delete(&self, key: &str) -> std::result::Result<(), SecretError> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
        Ok(())
    }
}

/// The OS keyring: the Secret portal / Secret Service through `oo7` on Linux, Windows Credential
/// Manager through `keyring` on Windows.
pub struct KeyringSecretStore {
    /// Namespace for the entries (the app name).
    service: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self {
            service: APP_DIR.into(),
        }
    }
}

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self::default()
    }
}

#[cfg(target_os = "linux")]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn set(&self, key: &str, value: &str) -> std::result::Result<(), SecretError> {
        let kr = oo7::Keyring::new().await.map_err(be)?;
        kr.create_item(
            &format!("{} {key}", self.service),
            &[("application", self.service.as_str()), ("key", key)],
            value,
            true,
        )
        .await
        .map_err(be)?;
        Ok(())
    }
    async fn get(&self, key: &str) -> std::result::Result<Option<String>, SecretError> {
        let kr = oo7::Keyring::new().await.map_err(be)?;
        let items = kr
            .search_items(&[("application", self.service.as_str()), ("key", key)])
            .await
            .map_err(be)?;
        match items.first() {
            Some(item) => {
                let secret = item.secret().await.map_err(be)?;
                Ok(Some(
                    String::from_utf8_lossy(secret.as_bytes()).into_owned(),
                ))
            }
            None => Ok(None),
        }
    }
    async fn delete(&self, key: &str) -> std::result::Result<(), SecretError> {
        let kr = oo7::Keyring::new().await.map_err(be)?;
        kr.delete(&[("application", self.service.as_str()), ("key", key)])
            .await
            .map_err(be)
    }
}

#[cfg(target_os = "linux")]
fn be(e: impl std::fmt::Display) -> SecretError {
    SecretError::Backend(e.to_string())
}

// Windows: not compiled or tested on Linux. Needs the `v1` feature of the `keyring` crate (see report).
#[cfg(windows)]
#[async_trait::async_trait]
impl SecretStore for KeyringSecretStore {
    async fn set(&self, key: &str, value: &str) -> std::result::Result<(), SecretError> {
        let e = keyring::Entry::new(&self.service, key).map_err(be)?;
        e.set_password(value).map_err(be)
    }
    async fn get(&self, key: &str) -> std::result::Result<Option<String>, SecretError> {
        let e = keyring::Entry::new(&self.service, key).map_err(be)?;
        match e.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(err) => Err(be(err)),
        }
    }
    async fn delete(&self, key: &str) -> std::result::Result<(), SecretError> {
        let e = keyring::Entry::new(&self.service, key).map_err(be)?;
        match e.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(err) => Err(be(err)),
        }
    }
}

#[cfg(windows)]
fn be(e: impl std::fmt::Display) -> SecretError {
    SecretError::Backend(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo_config() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config")
    }

    fn paths_in(tmp: &Path) -> Paths {
        Paths {
            shipped_config_dir: repo_config(),
            config_dir: tmp.join("config"),
            data_dir: tmp.join("data"),
        }
    }

    #[test]
    fn diagnostics_setting_defaults_to_rules_and_round_trips() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = paths_in(tmp.path());
        let rules = Rules::load(&paths.rules_path()).unwrap();
        let mut s =
            Settings::load_from(&paths.defaults_path(), &paths.user_settings_path()).unwrap();
        assert_eq!(s.diagnostics.enabled, None);
        assert_eq!(
            s.diagnostics_enabled(&rules),
            rules.diagnostics.enabled_default
        );
        // An existing settings.toml without [diagnostics] still loads (above). Save an explicit off.
        s.diagnostics.enabled = Some(!rules.diagnostics.enabled_default);
        s.save(&paths.defaults_path(), &paths.config_dir).unwrap();
        let text = std::fs::read_to_string(paths.user_settings_path()).unwrap();
        assert!(text.contains("[diagnostics]"), "{text}");
        let back =
            Settings::load_from(&paths.defaults_path(), &paths.user_settings_path()).unwrap();
        assert_eq!(
            back.diagnostics_enabled(&rules),
            !rules.diagnostics.enabled_default
        );
        // Back to None writes nothing for it.
        let mut s2 = back;
        s2.diagnostics.enabled = None;
        s2.save(&paths.defaults_path(), &paths.config_dir).unwrap();
        let text = std::fs::read_to_string(paths.user_settings_path()).unwrap();
        assert!(!text.contains("diagnostics"), "{text}");
    }

    #[test]
    fn shipped_rules_parse_with_every_key() {
        let r = Rules::load(&repo_config().join(RULES_FILE)).unwrap();
        assert_eq!(r.scouting.history_window, 20);
        assert_eq!(r.api.analytics_per_min, 160);
        assert_eq!(r.broadcast.backoff_s, vec![2, 4, 8, 16, 32]);
        assert_eq!(r.review.t7_even, [0.40, 0.60]);
        assert_eq!(r.review.obj_weights.base, 3);
        assert!(r.updates.check_on_start);
        assert_eq!(r.scouting.mmr_trend_min_points, 2);
        assert_eq!(r.scouting.mains_shown, 5);
        assert_eq!(r.ui.flush_ms, 500);
        assert_eq!(r.ui.item_search_results, 8);
        assert_eq!(r.ui.secret_timeout_s, 3);
        assert_eq!(r.builds.community_limit, 10);
        assert_eq!(r.builds.community_sort, "favorites");
        assert!(r.diagnostics.enabled_default);
        assert_eq!(r.diagnostics.keep_days, 14);
        assert_eq!(r.diagnostics.perf_sample_s, 10);
        assert_eq!(r.diagnostics.max_file_mb, 20);
        let w = &r.match_watcher;
        for (name, src) in [
            ("start", &w.start),
            ("end", &w.end),
            ("match_id", &w.match_id),
            ("exit", &w.exit),
        ] {
            assert!(!src.is_empty(), "{name} is configured");
            regex::Regex::new(src).unwrap_or_else(|e| panic!("{name} does not compile: {e}"));
        }
        let id = regex::Regex::new(&w.match_id).unwrap();
        assert_eq!(
            id.captures_len(),
            2,
            "match_id has exactly one capture group"
        );
    }

    #[test]
    fn rules_toml_matches_doc_14_block() {
        let doc = std::fs::read_to_string(repo_config().join("../14-data-and-content-assets.md"))
            .unwrap()
            .replace("\r\n", "\n");
        let start = doc.find("### DATA-001").unwrap();
        let block = &doc[start..];
        let block = &block[block.find("```toml\n").unwrap() + 8..];
        let block = &block[..block.find("```").unwrap()];
        let shipped = std::fs::read_to_string(repo_config().join(RULES_FILE)).unwrap();
        let doc_v: toml::Value = toml::from_str(block).unwrap();
        let shipped_v: toml::Value = toml::from_str(&shipped).unwrap();
        // Every key in the doc block must exist in the shipped file with the same value. The
        // shipped file may have more keys (pending a doc 14 update).
        fn subset(doc: &toml::Value, shipped: &toml::Value, path: &str) {
            match (doc, shipped) {
                (toml::Value::Table(d), toml::Value::Table(s)) => {
                    for (k, v) in d {
                        let here = format!("{path}.{k}");
                        let sv = s
                            .get(k)
                            .unwrap_or_else(|| panic!("{here} missing in rules.toml"));
                        subset(v, sv, &here);
                    }
                }
                (d, s) => assert_eq!(d, s, "{path} differs from doc 14"),
            }
        }
        subset(&doc_v, &shipped_v, "");
    }

    #[test]
    fn missing_rules_value_fails_loudly() {
        let text = std::fs::read_to_string(repo_config().join(RULES_FILE))
            .unwrap()
            .replace("\r\n", "\n");
        let text = text.replace("history_window          = 20\n", "");
        let err = Rules::from_toml_str(&text, Path::new("rules.toml")).unwrap_err();
        assert!(err.to_string().contains("history_window"), "{err}");
    }

    #[test]
    fn unknown_rules_key_fails() {
        let text = std::fs::read_to_string(repo_config().join(RULES_FILE)).unwrap();
        let text = text.replace("[overlay]", "[overlay]\nmystery = 1");
        assert!(Rules::from_toml_str(&text, Path::new("rules.toml")).is_err());
    }

    #[test]
    fn defaults_load_without_a_user_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        let s = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        assert_eq!(s.llm.llm_provider, LlmProvider::Off);
        assert_eq!(s.account.manual_account, "");
        assert!(s.steam.extra_library_paths.is_empty());
        assert_eq!(s.steam.deadlock_app_id, 1422450);
    }

    #[test]
    fn user_file_overrides_key_by_key() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        std::fs::create_dir_all(&p.config_dir).unwrap();
        std::fs::write(p.user_settings_path(), "[llm]\nllm_provider = \"local\"\n").unwrap();
        let s = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        assert_eq!(s.llm.llm_provider, LlmProvider::Local);
        assert_eq!(
            s.steam.deadlock_app_id, 1422450,
            "untouched keys keep their default"
        );
    }

    #[test]
    fn broken_user_file_is_an_error_not_a_silent_default() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        std::fs::create_dir_all(&p.config_dir).unwrap();
        std::fs::write(p.user_settings_path(), "this is [not toml").unwrap();
        assert!(Settings::load_from(&p.defaults_path(), &p.user_settings_path()).is_err());
    }

    #[test]
    fn save_then_load_roundtrips_and_writes_only_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        let mut s = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        s.account.manual_account = "22202".into();
        s.steam.extra_library_paths = vec![PathBuf::from("/mnt/games")];
        s.save(&p.defaults_path(), &p.config_dir).unwrap();
        let text = std::fs::read_to_string(p.user_settings_path()).unwrap();
        assert!(!text.contains("llm_provider"), "{text}");
        assert!(!text.contains("deadlock_app_id"), "{text}");
        let again = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        assert_eq!(again, s);
    }

    #[test]
    fn saving_defaults_writes_an_empty_file() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        let s = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        s.save(&p.defaults_path(), &p.config_dir).unwrap();
        assert_eq!(
            std::fs::read_to_string(p.user_settings_path())
                .unwrap()
                .trim(),
            ""
        );
    }

    #[tokio::test]
    async fn tc_25_1_secret_is_not_in_settings_toml() {
        let tmp = tempfile::tempdir().unwrap();
        let p = paths_in(tmp.path());
        let store = MemorySecretStore::new();
        let secret = "sk-test-0123456789-SECRET";
        store.set(secret_keys::LLM_OWN_KEY, secret).await.unwrap();
        let mut s = Settings::load_from(&p.defaults_path(), &p.user_settings_path()).unwrap();
        s.llm.llm_provider = LlmProvider::OwnKey;
        s.save(&p.defaults_path(), &p.config_dir).unwrap();
        let text = std::fs::read_to_string(p.user_settings_path()).unwrap();
        assert!(text.contains("own_key"));
        assert!(!text.contains(secret));
        assert_eq!(
            store
                .get(secret_keys::LLM_OWN_KEY)
                .await
                .unwrap()
                .as_deref(),
            Some(secret)
        );
    }

    #[tokio::test]
    async fn memory_store_set_get_delete() {
        let store = MemorySecretStore::new();
        assert_eq!(store.get("k").await.unwrap(), None);
        store.set("k", "v1").await.unwrap();
        store.set("k", "v2").await.unwrap();
        assert_eq!(store.get("k").await.unwrap().as_deref(), Some("v2"));
        store.delete("k").await.unwrap();
        store.delete("k").await.unwrap();
        assert_eq!(store.get("k").await.unwrap(), None);
    }

    #[test]
    fn share_dir_lookup_order_and_failure() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        for d in [&a, &b] {
            std::fs::create_dir_all(d.path().join("config")).unwrap();
            std::fs::write(d.path().join("config").join(RULES_FILE), "").unwrap();
        }
        let missing = PathBuf::from("/definitely/not/here");
        let got = pick_config_dir(&[missing.clone(), b.path().into(), a.path().into()]).unwrap();
        assert_eq!(got, b.path().join("config"));
        assert!(matches!(
            pick_config_dir(&[missing]),
            Err(SettingsError::NoShareDir { .. })
        ));
    }

    #[test]
    fn dev_build_finds_repo_config() {
        // Runs in a debug build; the env var may be set by the developer, so only check it resolves.
        let dir = find_shipped_config_dir().unwrap();
        assert!(dir.join(RULES_FILE).is_file());
    }
}
