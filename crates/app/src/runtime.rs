//! The real pipeline behind the main window (PRG-20, T-017, T-018).
//!
//! One controller task owns all state. Slint callbacks, the Match Watcher, the Broadcast client,
//! scouting and Review tasks all send it a `Cmd`; it updates the view models and pushes them to
//! the window on the UI thread. Heavy work (API calls, polling) runs in spawned tasks that report
//! back with a `Cmd`.
//!
//! Fair play (doc 03):
//! - The Live view and scouting read `LiveSafeState` from `live_safe::project_state`, never the
//!   raw `GameState` (NFR-FAIR-003, FP-4). See `vm_build`.
//! - Chat is never read here. No fight tracking. Game files are only read for `console.log` and
//!   Steam's own config (FP-3).
//! - Scouting cards stay hidden until `scouting_visible` (FP-10).
//! - Only the user's own match is recorded and persisted as a Review (FR-DATA-005).

use crate::diag::{self, SaveEnv, SaveResult};
use crate::i18n::I18n;
use crate::ui::{AppState, MainWindow};
use crate::vm_build::{self, CardKey, CheckInput, Fallbacks, LiveInput, ScoutInput};
use crate::{builder::BuilderDraft, ui_map, view_model::*};
use anyhow::{Context, Result};
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use stalemate_core::{
    account,
    api_client::{
        ApiClient, ApiConfig, ApiError, AssetCatalog, AssetDirs, CommunityBuild, MetadataOptions,
    },
    broadcast::{self, BroadcastDeps, BroadcastEvent, BroadcastKind},
    builds::{Build, BuildLibrary, BuildSection, ImportedBuild, InGameCommunity},
    diagnostics::{sample_self, Diagnostics, MatchDiag},
    game_state::GameState,
    live_safe::{self, LiveSafeState, Whitelist},
    match_watcher::{self, MatchEvent, Patterns, Timed},
    purchase_log::PurchaseLog,
    review::{build_stats_review, persist_stats_review},
    scouting::{scouting_visible, CardState, ScoutingCard, ScoutingService},
    settings::{Config, KeyringSecretStore},
    storage::{DataSource, GameMode, MatchPlayerRow, MatchRow, Storage},
    AccountId, MatchId, ParserEvent, RosterEntry,
};
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime},
};
use time::format_description::well_known::Rfc3339;
use tokio::{sync::mpsc, task::JoinHandle};

thread_local! {
    // Fluent bundles are not Send, so each thread that needs words makes its own.
    static I18N: I18n = I18n::en_us();
}

fn tr(key: &str) -> String {
    I18N.with(|i| i.t(key))
}

fn fallbacks() -> Fallbacks {
    Fallbacks {
        hero: tr("hero-unknown"),
        item: tr("item-unknown"),
        player: tr("player-unnamed"),
    }
}

fn rank_text(badge: u32) -> String {
    let (tier, step) = vm_build::badge_parts(badge);
    I18N.with(|i| {
        i.t_args(
            "rank-badge",
            &[
                ("tier", i64::from(tier).into()),
                ("step", i64::from(step).into()),
            ],
        )
    })
}

fn now_rfc3339() -> String {
    time::OffsetDateTime::now_utc()
        .format(&Rfc3339)
        .unwrap_or_default()
}

// ---- pushing to the window -----------------------------------------------------------------

#[derive(Clone)]
struct Ui {
    weak: slint::Weak<MainWindow>,
}

impl Ui {
    fn run(&self, f: impl FnOnce(&MainWindow, &I18n) + Send + 'static) {
        let weak = self.weak.clone();
        let _ = slint::invoke_from_event_loop(move || {
            if let Some(w) = weak.upgrade() {
                I18N.with(|i| f(&w, i));
            }
        });
    }
    fn home(&self, vm: HomeVm, offline: bool) {
        self.run(move |w, i| {
            w.global::<AppState>().set_offline(offline);
            ui_map::apply_home(w, &vm, i);
        });
    }
    fn scouting(&self, vm: ScoutingVm) {
        self.run(move |w, i| ui_map::apply_scouting(w, &vm, i));
    }
    fn live(&self, vm: LiveVm) {
        self.run(move |w, i| ui_map::apply_live(w, &vm, i));
    }
    fn reviews(&self, vm: ReviewsVm) {
        self.run(move |w, i| ui_map::apply_reviews(w, &vm, i));
    }
    fn builds(&self, vm: BuildsVm) {
        self.run(move |w, i| ui_map::apply_builds(w, &vm, i));
    }
    fn settings(&self, vm: SettingsVm) {
        self.run(move |w, i| ui_map::apply_settings(w, &vm, i));
    }
    fn builder(&self, draft: BuilderDraft, error: Option<crate::builder::BuilderError>) {
        self.run(move |w, i| ui_map::apply_builder(w, &draft, error, i));
    }
    fn builder_results(&self, found: Vec<NamedOption>) {
        self.run(move |w, i| {
            let chips: Vec<_> = found
                .iter()
                .map(|o| ui_map::map_option_chip(i, o))
                .collect();
            w.global::<AppState>()
                .set_builder_results(ModelRc::new(VecModel::from(chips)));
        });
    }
    fn mark_confirm(&self, key: &'static str) {
        self.run(move |w, i| {
            let text = i.t_args(key, &[("time", diag::clock_utc().into())]);
            w.global::<AppState>().set_mark_confirm(text.into());
        });
    }
    fn watch_error(&self, key: Option<&'static str>) {
        self.run(move |w, i| {
            w.global::<AppState>()
                .set_watch_error(key.map(|k| i.t(k)).unwrap_or_default().into());
        });
    }
    fn after_builder_saved(&self) {
        self.run(|w, _| {
            let s = w.global::<AppState>();
            s.set_builder_name(SharedString::new());
            s.set_builder_ability(SharedString::new());
            s.set_builder_results(ModelRc::default());
            s.set_builds_tab(crate::ui::BuildsTab::Library);
        });
    }
    fn show_library(&self) {
        self.run(|w, _| {
            w.global::<AppState>()
                .set_builds_tab(crate::ui::BuildsTab::Library);
        });
    }
}

// ---- commands ------------------------------------------------------------------------------

enum Cmd {
    CheckAgain,
    Retry(i32),
    WatchMatch(String),
    SaveAccount(String),
    SelectReview(i32),
    RetryReview(i32),
    SetActive(String),
    SearchCommunity(i32),
    ImportCommunity(i32),
    BuilderSearch(String),
    BuilderAddSection,
    BuilderRename(i32, String),
    BuilderAddItem {
        section: i32,
        found: i32,
    },
    BuilderMove(i32, i32, i32),
    BuilderRemove(i32, i32),
    BuilderSave {
        hero: i32,
        name: String,
    },
    SetDiagEnabled(bool),
    OpenDiagSave,
    CloseDiagSave,
    SaveDiag {
        note: String,
        include_id: bool,
    },
    DiagSaved(SaveResult),
    OpenDiagFolder,
    MarkProblem(String),
    ViewOpened(i32),
    PerfSample,
    Watcher(Timed),
    Broadcast(u64, BroadcastEvent),
    Scout(u64, Box<ScoutingCard>),
    ReviewDone {
        match_id: MatchId,
        ready: bool,
        attempts: u32,
        result: &'static str,
    },
    Catalog(Result<Arc<AssetCatalog>, String>),
    Community(Result<Vec<CommunityBuild>, String>),
    Imported(Result<Box<ImportedBuild>, String>),
    Tick,
}

type CmdTx = mpsc::UnboundedSender<Cmd>;

/// Keeps the tokio runtime alive. Dropping it stops every task.
pub struct Runtime {
    rt: Option<tokio::runtime::Runtime>,
}

impl Drop for Runtime {
    fn drop(&mut self) {
        if let Some(rt) = self.rt.take() {
            rt.shutdown_background();
        }
    }
}

/// Starts the core and connects it to the window.
pub fn start(window: &MainWindow, utc_offset: time::UtcOffset) -> Result<Runtime> {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .thread_name("stalemate-core")
        .build()
        .context("could not start the async runtime")?;
    let (tx, rx) = mpsc::unbounded_channel::<Cmd>();
    wire_callbacks(window, &tx);
    let scale = window.window().scale_factor();
    let ui = Ui {
        weak: window.as_weak(),
    };
    let tick_tx = tx.clone();
    rt.spawn(async move {
        let mut core = match Core::init(ui.clone(), tx, Some(scale), utc_offset).await {
            Ok(c) => c,
            Err(e) => {
                tracing::error!("could not start the core: {e:#}");
                let vm = HomeVm {
                    phase: Phase::Stopped,
                    ..Default::default()
                };
                ui.home(vm, false);
                return;
            }
        };
        core.run(rx, tick_tx).await;
    });
    Ok(Runtime { rt: Some(rt) })
}

fn wire_callbacks(window: &MainWindow, tx: &CmdTx) {
    let s = window.global::<AppState>();
    macro_rules! send {
        ($tx:expr, $cmd:expr) => {{
            let _ = $tx.send($cmd);
        }};
    }
    let t = tx.clone();
    s.on_check_again(move || send!(t, Cmd::CheckAgain));
    let t = tx.clone();
    s.on_retry(move |v| send!(t, Cmd::Retry(v)));
    let t = tx.clone();
    s.on_watch_match(move |text| send!(t, Cmd::WatchMatch(text.to_string())));
    let t = tx.clone();
    s.on_save_account_id(move |text| send!(t, Cmd::SaveAccount(text.to_string())));
    let t = tx.clone();
    s.on_select_review(move |i| send!(t, Cmd::SelectReview(i)));
    let t = tx.clone();
    s.on_retry_review(move |i| send!(t, Cmd::RetryReview(i)));
    let t = tx.clone();
    s.on_set_active(move |id| send!(t, Cmd::SetActive(id.to_string())));
    let t = tx.clone();
    s.on_search_community(move |h| send!(t, Cmd::SearchCommunity(h)));
    let t = tx.clone();
    s.on_import_community(move |i| send!(t, Cmd::ImportCommunity(i)));
    let t = tx.clone();
    s.on_builder_search(move |q| send!(t, Cmd::BuilderSearch(q.to_string())));
    let t = tx.clone();
    s.on_builder_add_section(move || send!(t, Cmd::BuilderAddSection));
    let t = tx.clone();
    s.on_builder_rename_section(move |i, title| send!(t, Cmd::BuilderRename(i, title.to_string())));
    let t = tx.clone();
    let weak = window.as_weak();
    s.on_builder_add_item(move |found| {
        let section = weak
            .upgrade()
            .map_or(0, |w| w.global::<AppState>().get_builder_target());
        send!(t, Cmd::BuilderAddItem { section, found });
    });
    let t = tx.clone();
    s.on_builder_move_item(move |a, b, c| send!(t, Cmd::BuilderMove(a, b, c)));
    let t = tx.clone();
    s.on_builder_remove_item(move |a, b| send!(t, Cmd::BuilderRemove(a, b)));
    let t = tx.clone();
    let weak = window.as_weak();
    s.on_builder_save(move || {
        if let Some(w) = weak.upgrade() {
            let st = w.global::<AppState>();
            send!(
                t,
                Cmd::BuilderSave {
                    hero: st.get_builder_hero(),
                    name: st.get_builder_name().to_string(),
                }
            );
        }
    });
    let t = tx.clone();
    s.on_set_diag_enabled(move |on| send!(t, Cmd::SetDiagEnabled(on)));
    let t = tx.clone();
    s.on_open_diag_save(move || send!(t, Cmd::OpenDiagSave));
    let t = tx.clone();
    s.on_close_diag_save(move || send!(t, Cmd::CloseDiagSave));
    let t = tx.clone();
    s.on_save_diag(move |note, include_id| {
        send!(
            t,
            Cmd::SaveDiag {
                note: note.to_string(),
                include_id
            }
        )
    });
    let t = tx.clone();
    s.on_open_diag_folder(move || send!(t, Cmd::OpenDiagFolder));
    let t = tx.clone();
    s.on_mark_problem(move |note| send!(t, Cmd::MarkProblem(note.to_string())));
    let t = tx.clone();
    s.on_view_opened(move |v| send!(t, Cmd::ViewOpened(v)));
    let weak = window.as_weak();
    s.on_set_text_scale(move |pct| {
        if let Some(w) = weak.upgrade() {
            w.global::<AppState>()
                .set_text_scale_pct(pct.clamp(75, 200));
        }
    });
}

// ---- one match -----------------------------------------------------------------------------

struct Session {
    gen: u64,
    match_id: MatchId,
    /// Found by the Match Watcher (true) or typed in by the user (false).
    detected: bool,
    gs: GameState,
    plog: PurchaseLog,
    ls: Option<LiveSafeState>,
    delay_seen: bool,
    connected: bool,
    lost: bool,
    broadcast_ended: bool,
    console_ended: bool,
    /// The match is over and its Review (if it is the user's) has been started.
    finished: bool,
    /// The user is in this match's roster, so it is saved and reviewed.
    mine: bool,
    match_row_saved: bool,
    purchases_saved: usize,
    scouted: bool,
    roster: Vec<RosterEntry>,
    cards: HashMap<CardKey, ScoutingCard>,
    live_dirty: bool,
    scout_dirty: bool,
    tasks: Vec<JoinHandle<()>>,
    bcast: Option<JoinHandle<broadcast::BroadcastOutcome>>,
    bcast_stats: broadcast::BroadcastStats,
    /// The API client's rate-limit wait count when this match began (the count is per run).
    waits_base: u64,
    // Diagnostics (FR-SET-005): what this match has shown so far, and when things started.
    diag: MatchDiag,
    diag_dirty: bool,
    started: Instant,
    scout_started: Option<Instant>,
    ever_connected: bool,
}

impl Drop for Session {
    fn drop(&mut self) {
        for t in &self.tasks {
            t.abort();
        }
        if let Some(b) = &self.bcast {
            b.abort();
        }
    }
}

struct Core {
    ui: Ui,
    tx: CmdTx,
    cfg: Arc<Config>,
    storage: Storage,
    api: Arc<ApiClient>,
    library: BuildLibrary,
    whitelist: Whitelist,
    asset_dirs: AssetDirs,
    catalog: Option<Arc<AssetCatalog>>,
    steam: account::SteamInfo,
    me: Option<AccountId>,
    vm: AppVm,
    session: Option<Session>,
    next_gen: u64,
    reviews_running: u32,
    match_started_seen: bool,
    watcher_ok: bool,
    utc_offset: time::UtcOffset,
    latency: LatencyTracker,
    watcher: Option<JoinHandle<()>>,
    broadcast_check: CheckState,
    api_check: CheckState,
    builds: Vec<Build>,
    community: Vec<CommunityBuild>,
    draft: BuilderDraft,
    found: Vec<NamedOption>,
    items: Vec<NamedOption>,
    saved_reviews: HashMap<MatchId, ReviewReportVm>,
    home_dirty: bool,
    reviews_dirty: bool,
    builds_dirty: bool,
    settings_dirty: bool,
    diag: Diagnostics,
    save_env: SaveEnv,
    /// The last result of each check written to the diagnostics, so only changes are written.
    checks_logged: Vec<(CheckId, CheckState)>,
}

/// Adds to one match's diagnostics. For events with no session at hand (a Review retry).
fn note(diag: &Diagnostics, match_id: MatchId, f: impl FnOnce(&mut MatchDiag)) {
    let mut d = MatchDiag {
        match_id,
        ..Default::default()
    };
    f(&mut d);
    diag.match_record(&d);
}

/// Milliseconds from a console line's own time to the moment this app read it (TST-010).
/// Never negative: a small clock or zone mismatch counts as zero.
fn latency_ms(line_time: Option<SystemTime>, read_at: SystemTime) -> Option<u32> {
    let d = read_at.duration_since(line_time?).unwrap_or(Duration::ZERO);
    Some(u32::try_from(d.as_millis()).unwrap_or(u32::MAX))
}

/// Finds the detection latency of one match: from the `Started` line, or failing that the first
/// event of the match whose line carried a time. It is known before the match id arrives, so it
/// waits here until the session starts.
#[derive(Debug)]
struct LatencyTracker {
    value: Option<u32>,
    /// Still looking: the match has no value yet.
    open: bool,
}

impl Default for LatencyTracker {
    fn default() -> Self {
        Self {
            value: None,
            open: true,
        }
    }
}

impl LatencyTracker {
    fn observe(&mut self, started: bool, latency: Option<u32>) {
        if started {
            self.value = latency;
            self.open = latency.is_none();
        } else if self.open && latency.is_some() {
            self.value = latency;
            self.open = false;
        }
    }

    /// The value for the session that is starting. The next match starts fresh.
    fn take(&mut self) -> Option<u32> {
        self.open = true;
        self.value.take()
    }
}

impl Session {
    /// Copies the Broadcast error count and this match's rate-limit waits into the diagnostics.
    fn sync_counters(&mut self, waits_now: u64) {
        let frag = u32::try_from(self.bcast_stats.fragment_errors()).unwrap_or(u32::MAX);
        let waits = u32::try_from(waits_now.saturating_sub(self.waits_base)).unwrap_or(u32::MAX);
        if self.diag.fragment_errors != Some(frag) {
            self.diag.fragment_errors = Some(frag);
            self.diag_dirty = true;
        }
        if self.diag.rate_limit_waits != Some(waits) {
            self.diag.rate_limit_waits = Some(waits);
            self.diag_dirty = true;
        }
    }
}

/// Writes the session's diagnostics if anything changed since the last write.
fn flush_diag(diag: &Diagnostics, s: &mut Session, waits_now: u64) {
    s.sync_counters(waits_now);
    if std::mem::take(&mut s.diag_dirty) {
        diag.match_record(&s.diag);
    }
}

fn ms(since: Instant) -> u32 {
    u32::try_from(since.elapsed().as_millis()).unwrap_or(u32::MAX)
}

impl Core {
    async fn init(
        ui: Ui,
        tx: CmdTx,
        scale: Option<f32>,
        utc_offset: time::UtcOffset,
    ) -> Result<Core> {
        let mut config = stalemate_core::settings::load().context("loading settings")?;
        // DEC-070: the old one-word diagnostics file now lives in settings.toml.
        let mut migrated = config.settings.clone();
        diag::migrate_legacy_flag(&config.paths.data_dir, &mut migrated, |s| {
            let mut c = config.clone();
            c.settings = s.clone();
            match c.save_settings() {
                Ok(()) => true,
                Err(e) => {
                    tracing::warn!("could not save the diagnostics setting: {e}");
                    false
                }
            }
        });
        config.settings = migrated;
        let cfg = Arc::new(config);
        let storage = match Storage::open(cfg.paths.db_path()) {
            Ok(s) => s,
            Err(e) => {
                // The window still works without a saved history.
                tracing::error!("could not open the database, using memory only: {e}");
                Storage::open_in_memory().context("in-memory database")?
            }
        };
        let whitelist = match Whitelist::load(&cfg.paths.shipped_config_dir.join("live_safe.toml"))
        {
            Ok(w) => w,
            Err(e) => {
                // The safest whitelist is the empty one (DATA-003).
                tracing::error!("live_safe.toml: {e}; Live-Safe extras stay off");
                Whitelist::default()
            }
        };
        let store = KeyringSecretStore::new();
        let api_cfg = tokio::time::timeout(
            Duration::from_secs(u64::from(cfg.rules.ui.secret_timeout_s)),
            ApiConfig::default().with_key_from(&store),
        )
        .await
        .unwrap_or_default();
        // The key is only held for the diagnostics leak scan, which refuses to save it.
        let api_key = api_cfg.api_key.clone();
        let api = Arc::new(
            ApiClient::new(storage.clone(), &cfg.rules.api, api_cfg).context("API client")?,
        );
        let library = BuildLibrary::new(storage.clone());
        let asset_dirs = AssetDirs::new(&cfg.paths.data_dir);

        if let Err(e) = broadcast::cleanup_recordings(
            &cfg.paths.recordings_dir(),
            &cfg.rules.retention,
            SystemTime::now(),
        ) {
            tracing::warn!("recording cleanup failed: {e}");
        }

        diag::prune_logs(
            &cfg.paths.data_dir.join("logs"),
            cfg.rules.diagnostics.keep_days,
        );
        let diag_on = cfg.settings.diagnostics_enabled(&cfg.rules);
        let diag = Diagnostics::open(
            &cfg.paths.data_dir.join("diagnostics"),
            &cfg.rules.diagnostics,
            diag_on,
        );
        let save_env = SaveEnv {
            diag: diag.clone(),
            log_dir: cfg.paths.data_dir.join("logs"),
            out_dir: diag::export_dir(&cfg.paths.data_dir),
            about: diag::about(
                Some(&cfg.paths.shipped_config_dir),
                &account::SteamInfo::default(),
                scale,
            ),
            api_key,
            me: None,
        };

        let mut core = Core {
            ui,
            tx,
            cfg,
            storage,
            api,
            library,
            whitelist,
            asset_dirs,
            catalog: None,
            steam: account::SteamInfo::default(),
            me: None,
            vm: AppVm::default(),
            session: None,
            next_gen: 0,
            reviews_running: 0,
            match_started_seen: false,
            watcher_ok: false,
            utc_offset,
            latency: LatencyTracker::default(),
            watcher: None,
            broadcast_check: CheckState::Pending,
            api_check: CheckState::Pending,
            builds: Vec::new(),
            community: Vec::new(),
            draft: BuilderDraft::new(),
            found: Vec::new(),
            items: Vec::new(),
            saved_reviews: HashMap::new(),
            home_dirty: true,
            reviews_dirty: true,
            builds_dirty: true,
            settings_dirty: true,
            diag,
            save_env,
            checks_logged: Vec::new(),
        };
        core.vm.settings.version = env!("CARGO_PKG_VERSION").into();
        core.vm.settings.diag_enabled = diag_on;
        core.vm.builds.state = DataState::Loading;
        core.resolve_account();
        core.start_watcher();
        core.load_reviews();
        core.load_builds();
        core.refresh_home();
        core.start_catalog_load();
        core.clear_live();
        core.ui.builder(core.draft.clone(), None);
        core.flush();
        Ok(core)
    }

    async fn run(&mut self, mut rx: mpsc::UnboundedReceiver<Cmd>, tx: CmdTx) {
        let flush = Duration::from_millis(u64::from(self.cfg.rules.ui.flush_ms.max(50)));
        let perf_every =
            Duration::from_secs(u64::from(self.cfg.rules.diagnostics.perf_sample_s.max(1)));
        let perf_tx = tx.clone();
        let perf_ticker = tokio::spawn(async move {
            let mut t = tokio::time::interval(perf_every);
            loop {
                t.tick().await;
                if perf_tx.send(Cmd::PerfSample).is_err() {
                    break;
                }
            }
        });
        let ticker = tokio::spawn(async move {
            let mut t = tokio::time::interval(flush);
            loop {
                t.tick().await;
                if tx.send(Cmd::Tick).is_err() {
                    break;
                }
            }
        });
        while let Some(cmd) = rx.recv().await {
            self.handle(cmd);
            self.flush();
        }
        ticker.abort();
        perf_ticker.abort();
    }

    // ---- setup and checks (T-018) ----

    fn home_dir() -> PathBuf {
        std::env::var_os("HOME")
            .or_else(|| std::env::var_os("USERPROFILE"))
            .map(PathBuf::from)
            .unwrap_or_default()
    }

    /// Finds Steam and the Account ID. A valid manual ID wins over Steam's (FR-DATA-006).
    fn resolve_account(&mut self) {
        self.steam = account::resolve(
            &Self::home_dir(),
            &self.cfg.settings.steam.extra_library_paths,
        );
        let manual = self.cfg.settings.account.manual_account.trim();
        let manual_id = (!manual.is_empty())
            .then(|| account::parse_manual_account(manual).ok())
            .flatten();
        self.me = manual_id.or(self.steam.account_id);
        self.save_env.me = self.me;
        self.save_env.about.steam_kind = diag::steam_kind(&self.steam);
        self.vm.settings.account_id = self.me;
        self.settings_dirty = true;
    }

    fn start_watcher(&mut self) {
        if let Some(w) = self.watcher.take() {
            w.abort();
        }
        self.watcher_ok = false;
        let patterns = match Patterns::from_rules(&self.cfg.rules) {
            Ok(p) => p,
            Err(e) => {
                // Match detection is off; the rest of the app works (manual "Watch match").
                tracing::warn!("{e}");
                return;
            }
        };
        let Some(path) = self.steam.console_log.clone() else {
            return;
        };
        self.watcher_ok = true;
        let (mtx, mut mrx) = mpsc::channel(32);
        let poll = Duration::from_secs(self.cfg.rules.match_watcher.poll_s.max(1));
        let handle = match_watcher::spawn_timed(path, patterns, mtx, poll, self.utc_offset);
        let tx = self.tx.clone();
        self.watcher = Some(tokio::spawn(async move {
            while let Some(ev) = mrx.recv().await {
                if tx.send(Cmd::Watcher(ev)).is_err() {
                    break;
                }
            }
            handle.abort();
        }));
    }

    fn refresh_home(&mut self) {
        let log = self
            .steam
            .console_log
            .as_deref()
            .map(account::console_log_state);
        let option = match (self.steam.roots.first(), self.me) {
            (Some(root), Some(me)) => account::launch_options_have_condebug(
                root,
                me,
                self.cfg.settings.steam.deadlock_app_id,
            ),
            _ => None,
        };
        let patterns_ok = Patterns::from_rules(&self.cfg.rules).is_ok();
        self.vm.home.checks = vm_build::setup_checks(&CheckInput {
            steam_found: !self.steam.roots.is_empty(),
            condebug_option: option,
            console_log: log.as_ref(),
            account_id: self.me,
            match_watcher: if patterns_ok && self.watcher_ok {
                CheckState::Pass
            } else {
                CheckState::Fail
            },
            broadcast: self.broadcast_check,
            deadlock_api: self.api_check,
        });
        self.vm.home.setup_result = self
            .vm
            .home
            .checks
            .iter()
            .find(|c| c.id == CheckId::Condebug)
            .map_or(CheckState::Pending, |c| c.state);
        self.home_dirty = true;
    }

    fn check_again(&mut self) {
        self.resolve_account();
        if self.watcher.is_none() || !self.watcher_ok {
            self.start_watcher();
        }
        if self.catalog.is_none() {
            self.start_catalog_load();
        }
        self.refresh_home();
        self.load_reviews();
    }

    // ---- catalog (API-15), builds ----

    fn start_catalog_load(&mut self) {
        let api = self.api.clone();
        let tx = self.tx.clone();
        let dirs = self.asset_dirs.clone();
        tokio::spawn(async move {
            match api.load_catalog().await {
                Ok(cat) => {
                    let cat = Arc::new(cat);
                    let _ = tx.send(Cmd::Catalog(Ok(cat.clone())));
                    // Art is loaded at runtime into the cache and never bundled (doc 13).
                    let (ok, failed) = api.download_catalog_images(&cat, &dirs).await;
                    tracing::info!("catalog images: {ok} present, {failed} failed");
                    let _ = dirs.remove_other_patches(cat.patch());
                }
                Err(e) => {
                    let offline = matches!(e, ApiError::Net(_));
                    let _ = tx.send(Cmd::Catalog(Err(if offline {
                        "offline".into()
                    } else {
                        e.to_string()
                    })));
                }
            }
        });
    }

    fn on_catalog(&mut self, res: Result<Arc<AssetCatalog>, String>) {
        match res {
            Ok(cat) => {
                self.api_check = CheckState::Pass;
                self.vm.offline = false;
                self.vm.builds.state = DataState::Loaded;
                self.vm.builds.heroes = cat
                    .heroes()
                    .iter()
                    .filter(|h| h.player_selectable && !h.disabled && !h.in_development)
                    .map(|h| NamedOption {
                        id: h.id,
                        name: h.name.clone(),
                        category: None,
                    })
                    .collect();
                self.vm.builds.heroes.sort_by(|a, b| a.name.cmp(&b.name));
                self.items = cat
                    .items()
                    .iter()
                    .filter(|i| i.kind == "upgrade" && i.shopable && !i.disabled)
                    .map(|i| NamedOption {
                        id: i.id,
                        name: i.name.clone(),
                        category: vm_build::category_from_slot(i.slot.as_deref().unwrap_or("")),
                    })
                    .collect();
                // FR-BUILD-006: a new patch can remove items from saved builds.
                if let Err(e) = self.library.revalidate_all(&cat) {
                    tracing::warn!("build check after patch change failed: {e}");
                }
                self.catalog = Some(cat);
                self.load_builds();
                self.load_reviews();
            }
            Err(msg) => {
                self.api_check = CheckState::Fail;
                self.vm.offline = msg == "offline";
                self.vm.builds.state = DataState::Error {
                    source: "deadlock-api".into(),
                };
                self.builds_dirty = true;
            }
        }
        self.refresh_home();
    }

    fn load_builds(&mut self) {
        let cat = self.catalog.clone();
        self.builds = self.library.list(None).unwrap_or_default();
        let fb = fallbacks();
        let active: Vec<(u32, String)> = self.storage.active_build_plans().unwrap_or_default();
        self.vm.builds.library = self
            .builds
            .iter()
            .map(|b| {
                let is_active = active
                    .iter()
                    .any(|(h, id)| *h == b.hero_id && *id == b.build_id);
                vm_build::build_vm(b, cat.as_deref(), is_active, &fb)
            })
            .collect();
        self.builds_dirty = true;
    }

    fn search_community(&mut self, hero_index: i32) {
        let Some(hero) = usize::try_from(hero_index)
            .ok()
            .and_then(|i| self.vm.builds.heroes.get(i))
            .cloned()
        else {
            return;
        };
        if self.catalog.is_none() {
            self.vm.builds.community_state = DataState::Error {
                source: "deadlock-api".into(),
            };
            self.builds_dirty = true;
            return;
        }
        self.vm.builds.community_state = DataState::Loading;
        self.vm.builds.community.clear();
        self.community.clear();
        self.builds_dirty = true;
        let api = self.api.clone();
        let tx = self.tx.clone();
        let (sort, limit) = (
            self.cfg.rules.builds.community_sort.clone(),
            self.cfg.rules.builds.community_limit,
        );
        tokio::spawn(async move {
            let res = api
                .community_builds(hero.id, &sort, limit)
                .await
                .map_err(|e| e.to_string());
            let _ = tx.send(Cmd::Community(res));
        });
    }

    fn on_community(&mut self, res: Result<Vec<CommunityBuild>, String>) {
        match res {
            Ok(list) => {
                let fb = fallbacks();
                let cat = self.catalog.clone();
                self.vm.builds.community = list
                    .iter()
                    .map(|b| CommunityBuildVm {
                        name: b.hero_build.name.clone(),
                        hero_name: vm_build::hero_name(cat.as_deref(), b.hero_build.hero_id, &fb),
                        usage: b.num_favorites.unwrap_or(0),
                        sections: vm_build::community_sections(b, cat.as_deref(), &fb),
                    })
                    .collect();
                self.vm.builds.community_state = if list.is_empty() {
                    DataState::Empty
                } else {
                    DataState::Loaded
                };
                self.community = list;
            }
            Err(_) => {
                self.vm.builds.community_state = DataState::Error {
                    source: "deadlock-api".into(),
                };
            }
        }
        self.builds_dirty = true;
    }
}

impl Core {
    fn handle(&mut self, cmd: Cmd) {
        if let Some(key) = usage_key(&cmd) {
            self.diag.usage(key);
        }
        match cmd {
            Cmd::Tick => self.tick(),
            Cmd::CheckAgain => self.check_again(),
            Cmd::Retry(view) => self.retry(view),
            Cmd::WatchMatch(text) => self.watch_match(&text),
            Cmd::SaveAccount(text) => self.save_account(&text),
            Cmd::SelectReview(i) => self.select_review(i),
            Cmd::RetryReview(i) => self.retry_review(i),
            Cmd::SetActive(id) => self.set_active(&id),
            Cmd::SearchCommunity(h) => self.search_community(h),
            Cmd::ImportCommunity(i) => self.import_community(i),
            Cmd::BuilderSearch(q) => self.builder_search(&q),
            Cmd::BuilderAddSection => {
                self.draft.add_section();
                self.ui.builder(self.draft.clone(), None);
            }
            Cmd::BuilderRename(i, title) => {
                // No redraw: the text box already shows what was typed.
                let _ = self.draft.rename_section(i.max(0) as usize, &title);
            }
            Cmd::BuilderAddItem { section, found } => {
                let err = self
                    .found
                    .get(found.max(0) as usize)
                    .cloned()
                    .and_then(|item| self.draft.add_item(section.max(0) as usize, item).err());
                self.ui.builder(self.draft.clone(), err);
            }
            Cmd::BuilderMove(s, i, d) => {
                self.draft
                    .move_item(s.max(0) as usize, i.max(0) as usize, d);
                self.ui.builder(self.draft.clone(), None);
            }
            Cmd::BuilderRemove(s, i) => {
                self.draft.remove_item(s.max(0) as usize, i.max(0) as usize);
                self.ui.builder(self.draft.clone(), None);
            }
            Cmd::BuilderSave { hero, name } => self.builder_save(hero, name),
            Cmd::Watcher(t) => {
                let started = matches!(t.event, MatchEvent::Started);
                self.latency
                    .observe(started, latency_ms(t.line_time, t.read_at));
                self.on_match_event(t.event);
                // A session that began before its latency was known gets it now.
                if let Some(s) = self.session.as_mut() {
                    if !s.finished && s.diag.detect_latency_ms.is_none() {
                        if let Some(v) = self.latency.value {
                            s.diag.detect_latency_ms = Some(v);
                            s.diag_dirty = true;
                        }
                    }
                }
            }
            Cmd::Broadcast(gen, ev) => self.on_broadcast(gen, ev),
            Cmd::Scout(gen, card) => self.on_scout(gen, *card),
            Cmd::ReviewDone {
                match_id,
                ready,
                attempts,
                result,
            } => self.on_review_done(match_id, ready, attempts, result),
            Cmd::SetDiagEnabled(on) => self.set_diag_enabled(on),
            Cmd::OpenDiagSave => self.open_diag_save(),
            Cmd::CloseDiagSave => {
                self.vm.settings.diag_open = false;
                self.settings_dirty = true;
            }
            Cmd::SaveDiag { note, include_id } => self.save_diag(note, include_id),
            Cmd::DiagSaved(result) => {
                self.vm.settings.diag_result = result;
                self.settings_dirty = true;
            }
            Cmd::OpenDiagFolder => diag::open_folder(&self.save_env.out_dir),
            Cmd::MarkProblem(text) => self.mark_problem(&text),
            Cmd::ViewOpened(_) => {}
            Cmd::PerfSample => self.perf_sample(),
            Cmd::Catalog(res) => self.on_catalog(res),
            Cmd::Community(res) => self.on_community(res),
            Cmd::Imported(res) => self.on_imported(res),
        }
    }
}

/// The count a command adds to the diagnostics `usage.json`: views opened and the main actions.
/// Counts only, no content.
fn usage_key(cmd: &Cmd) -> Option<&'static str> {
    Some(match cmd {
        Cmd::ViewOpened(v) => diag::view_key(*v),
        Cmd::CheckAgain => "check-again",
        Cmd::Retry(_) => "retry",
        Cmd::RetryReview(_) => "retry-review",
        Cmd::WatchMatch(_) => "watch-match",
        Cmd::SaveAccount(_) => "save-account",
        Cmd::SetActive(_) => "set-active-build",
        Cmd::SearchCommunity(_) => "search-community",
        Cmd::ImportCommunity(_) => "import-build",
        Cmd::BuilderSave { .. } => "builder-save",
        Cmd::SaveDiag { .. } => "save-diagnostics",
        Cmd::MarkProblem(_) => "mark-problem",
        _ => return None,
    })
}

// ---- diagnostics (FR-SET-005) ----------------------------------------------------------------

impl Core {
    fn set_diag_enabled(&mut self, on: bool) {
        self.diag.set_enabled(on);
        let mut cfg = (*self.cfg).clone();
        cfg.settings.diagnostics.enabled = Some(on);
        if let Err(e) = cfg.save_settings() {
            tracing::warn!("could not save the diagnostics setting: {e}");
        }
        self.cfg = Arc::new(cfg);
        self.vm.settings.diag_enabled = on;
        self.settings_dirty = true;
    }

    fn open_diag_save(&mut self) {
        let lines = I18N.with(|i| diag::preview_lines(i, &self.diag.summary()));
        self.vm.settings.diag_open = true;
        self.vm.settings.diag_preview = lines;
        self.vm.settings.diag_result = SaveResult::None;
        self.settings_dirty = true;
    }

    /// Zips on a blocking thread: the file can be several megabytes.
    fn save_diag(&mut self, note: String, include_id: bool) {
        let (env, tx) = (self.save_env.clone(), self.tx.clone());
        tokio::task::spawn_blocking(move || {
            let result = env.save(&note, include_id).into();
            let _ = tx.send(Cmd::DiagSaved(result));
        });
    }

    fn mark_problem(&mut self, note: &str) {
        if self.diag.is_enabled() {
            self.diag.marker(note.trim());
            self.ui.mark_confirm("mark-confirmed");
        } else {
            self.ui.mark_confirm("mark-off");
        }
    }

    /// CPU and memory, during Live only (NFR-PERF-001, NFR-PERF-002).
    fn perf_sample(&mut self) {
        let live = self
            .session
            .as_ref()
            .is_some_and(|s| s.connected && !s.finished && !s.lost);
        if live {
            if let Some(sample) = sample_self() {
                self.diag.perf(sample);
            }
        }
    }

    /// Writes each check whose result changed since it was last written.
    fn log_checks(&mut self) {
        for c in &self.vm.home.checks {
            let prev = self
                .checks_logged
                .iter()
                .find(|(id, _)| *id == c.id)
                .map(|(_, s)| *s);
            if prev == Some(c.state) {
                continue;
            }
            let word = match c.state {
                CheckState::Pass => "pass",
                CheckState::Fail => "fail",
                CheckState::Pending => "pending",
            };
            // The result only. Values such as the Account ID or a folder are not written.
            self.diag
                .check(diag::check_key(c.id), c.state == CheckState::Pass, word);
            self.checks_logged.retain(|(id, _)| *id != c.id);
            self.checks_logged.push((c.id, c.state));
        }
    }
}

// ---- actions from the window -----------------------------------------------------------------

impl Core {
    fn tick(&mut self) {
        let dirty = self
            .session
            .as_ref()
            .is_some_and(|s| s.live_dirty || s.scout_dirty);
        if dirty {
            self.refresh_live();
        }
        let waits = self.api.rate_limit_waits();
        if let Some(s) = self.session.as_mut() {
            flush_diag(&self.diag, s, waits);
        }
    }

    fn retry(&mut self, view: i32) {
        match view {
            1 => {
                if let Some(s) = self.session.as_mut() {
                    s.cards.clear();
                    s.scouted = false;
                    s.live_dirty = true;
                }
            }
            2 => {
                let again = self
                    .session
                    .as_ref()
                    .filter(|s| s.lost)
                    .map(|s| (s.match_id, s.detected));
                if let Some((id, detected)) = again {
                    self.start_session(id, detected);
                }
            }
            3 => self.load_reviews(),
            4 => {
                if self.catalog.is_none() {
                    self.vm.builds.state = DataState::Loading;
                    self.builds_dirty = true;
                    self.start_catalog_load();
                } else {
                    self.load_builds();
                }
            }
            _ => {}
        }
    }

    fn watch_match(&mut self, text: &str) {
        let text = text.trim();
        let id = (!text.is_empty() && text.bytes().all(|b| b.is_ascii_digit()))
            .then(|| text.parse::<MatchId>().ok())
            .flatten()
            .filter(|id| *id != 0);
        match id {
            Some(id) => {
                self.ui.watch_error(None);
                self.start_session(id, false);
            }
            None => self.ui.watch_error(Some("watch-invalid")),
        }
    }

    fn save_account(&mut self, text: &str) {
        match account::parse_manual_account(text) {
            Ok(id) => {
                let mut cfg = (*self.cfg).clone();
                cfg.settings.account.manual_account = id.to_string();
                if let Err(e) = cfg.save_settings() {
                    tracing::warn!("could not save the Account ID: {e}");
                }
                self.cfg = Arc::new(cfg);
                self.me = Some(id);
                self.vm.settings.account_id = Some(id);
                self.vm.settings.account_invalid = false;
                self.refresh_home();
                self.load_reviews();
            }
            Err(_) => self.vm.settings.account_invalid = true,
        }
        self.settings_dirty = true;
    }

    // ---- reviews ----

    fn lane_check_secs(&self) -> u32 {
        self.cfg.rules.counter.lane_phase_end_s
    }

    /// Reads the saved matches and their Reviews from storage (FR-REV-001).
    fn load_reviews(&mut self) {
        let fb = fallbacks();
        let now = time::OffsetDateTime::now_utc();
        let cat = self.catalog.clone();
        let rows = match self.storage.list_matches(None) {
            Ok(r) => r,
            Err(e) => {
                tracing::warn!("could not read saved matches: {e}");
                self.vm.reviews.state = DataState::Error {
                    source: "storage".into(),
                };
                self.reviews_dirty = true;
                return;
            }
        };
        let mut list = Vec::new();
        let mut reports = HashMap::new();
        for m in &rows {
            let id = m.match_id as i64;
            let json: Option<String> = self
                .storage
                .with_conn(|c| {
                    c.prepare("SELECT report_json FROM reviews WHERE match_id = ?1")?
                        .query_map([id], |r| r.get::<_, String>(0))?
                        .next()
                        .transpose()
                })
                .ok()
                .flatten();
            let item = vm_build::review_list_item(m, json.is_some(), cat.as_deref(), &fb, now);
            if !item.pending {
                let report = json
                    .as_deref()
                    .and_then(|j| {
                        vm_build::report_from_json(j, cat.as_deref(), &fb, self.lane_check_secs())
                    })
                    .unwrap_or_else(|| {
                        vm_build::record_only_report(m, cat.as_deref(), &fb, self.lane_check_secs())
                    });
                reports.insert(m.match_id, report);
            }
            list.push(item);
        }
        self.vm.home.last_match = list.iter().find(|i| !i.pending).map(|i| MatchSummary {
            outcome: i.outcome,
            hero_name: i.hero_name.clone(),
            duration_secs: i.duration_secs,
            ended_ago_secs: i.ended_ago_secs,
        });
        // Keep the same match selected when the list reloads.
        let selected_id = self
            .vm
            .reviews
            .selected
            .and_then(|i| self.vm.reviews.list.get(i))
            .map(|r| r.match_id);
        self.vm.reviews.selected =
            selected_id.and_then(|id| list.iter().position(|r| r.match_id == id));
        self.vm.reviews.report = self
            .vm
            .reviews
            .selected
            .and_then(|i| list.get(i))
            .and_then(|r| reports.get(&r.match_id).cloned());
        self.vm.reviews.state = if list.is_empty() {
            DataState::Empty
        } else {
            DataState::Loaded
        };
        self.vm.reviews.list = list;
        self.saved_reviews = reports;
        self.reviews_dirty = true;
        self.home_dirty = true;
    }

    fn select_review(&mut self, i: i32) {
        let Some(idx) = usize::try_from(i)
            .ok()
            .filter(|i| *i < self.vm.reviews.list.len())
        else {
            return;
        };
        let id = self.vm.reviews.list[idx].match_id;
        self.vm.reviews.selected = Some(idx);
        self.vm.reviews.report = self.saved_reviews.get(&id).cloned();
        self.reviews_dirty = true;
    }

    fn retry_review(&mut self, i: i32) {
        let id = usize::try_from(i)
            .ok()
            .and_then(|i| self.vm.reviews.list.get(i))
            .map(|r| r.match_id);
        if let Some(id) = id {
            // One immediate attempt. The long wait schedule is for right after a match.
            self.start_review(id, vec![0]);
        }
    }

    fn start_review(&mut self, match_id: MatchId, waits_s: Vec<u64>) {
        let Some(me) = self.me.filter(|m| *m != 0) else {
            return;
        };
        self.reviews_running += 1;
        self.home_dirty = true;
        let (api, storage, cfg, tx) = (
            self.api.clone(),
            self.storage.clone(),
            self.cfg.clone(),
            self.tx.clone(),
        );
        tokio::spawn(async move {
            let mut ready = false;
            let mut attempts = 0u32;
            let mut result = "not_found";
            'waits: for wait in waits_s {
                if wait > 0 {
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                }
                attempts += 1;
                // API-05. A match deadlock-api doesn't have yet gives an empty list or an error;
                // either way we wait for the next slot (FR-REV-001).
                let found = match api
                    .match_metadata_bulk(&[match_id], &MetadataOptions::default())
                    .await
                {
                    Ok(f) => f,
                    Err(_) => {
                        result = "api_error";
                        continue;
                    }
                };
                let Some(meta) = found.into_iter().next() else {
                    result = "not_found";
                    continue;
                };
                let Some(review) = build_stats_review(&meta, me, &cfg.rules) else {
                    result = "not_in_match";
                    break 'waits; // the user isn't in this match: nothing to review
                };
                let patch = api.patch().unwrap_or_else(|| "unknown".into());
                match persist_stats_review(&storage, &meta, me, &review, &patch) {
                    Ok(()) => {
                        ready = true;
                        result = "ready";
                    }
                    Err(e) => {
                        result = "save_failed";
                        tracing::warn!("could not save the Review: {e}");
                    }
                }
                break;
            }
            let _ = tx.send(Cmd::ReviewDone {
                match_id,
                ready,
                attempts,
                result,
            });
        });
    }

    fn on_review_done(&mut self, match_id: MatchId, _ready: bool, attempts: u32, result: &str) {
        // Replaces the earlier values when the tester presses "Try again" later.
        note(&self.diag, match_id, |d| {
            d.review_attempts = Some(attempts);
            d.review_result = Some(result.to_string());
        });
        self.reviews_running = self.reviews_running.saturating_sub(1);
        self.load_reviews();
    }

    // ---- builds ----

    fn set_active(&mut self, build_id: &str) {
        let Some(hero) = self
            .builds
            .iter()
            .find(|b| b.build_id == build_id)
            .map(|b| b.hero_id)
        else {
            return;
        };
        if let Err(e) = self.library.set_active(hero, build_id) {
            tracing::warn!("could not set the active build: {e}");
        }
        self.load_builds();
    }

    fn import_community(&mut self, i: i32) {
        let Some(id) = usize::try_from(i)
            .ok()
            .and_then(|i| self.community.get(i))
            .map(|b| b.hero_build.hero_build_id)
        else {
            return;
        };
        let Some(cat) = self.catalog.clone() else {
            return;
        };
        let (api, tx) = (self.api.clone(), self.tx.clone());
        tokio::spawn(async move {
            let res = InGameCommunity::new(api, cat)
                .import_build_id(id)
                .await
                .map_err(|e| e.to_string())
                .and_then(|o| o.map(Box::new).ok_or_else(|| "not found".to_string()));
            let _ = tx.send(Cmd::Imported(res));
        });
    }

    fn on_imported(&mut self, res: Result<Box<ImportedBuild>, String>) {
        let (Ok(imported), Some(cat)) = (res, self.catalog.clone()) else {
            self.vm.builds.community_state = DataState::Error {
                source: "deadlock-api".into(),
            };
            self.builds_dirty = true;
            return;
        };
        match self.library.add(imported.build, &cat) {
            Ok(_) => {
                self.load_builds();
                self.ui.show_library();
            }
            Err(e) => tracing::warn!("could not store the imported build: {e}"),
        }
    }

    fn builder_search(&mut self, query: &str) {
        let q = query.trim().to_lowercase();
        self.found = if q.is_empty() {
            Vec::new()
        } else {
            self.items
                .iter()
                .filter(|i| i.name.to_lowercase().contains(&q))
                .take(self.cfg.rules.ui.item_search_results as usize)
                .cloned()
                .collect()
        };
        self.ui.builder_results(self.found.clone());
    }

    fn builder_save(&mut self, hero_index: i32, name: String) {
        use crate::builder::BuilderError;
        self.draft.hero = usize::try_from(hero_index)
            .ok()
            .and_then(|i| self.vm.builds.heroes.get(i))
            .cloned();
        self.draft.name = name;
        if let Err(e) = self.draft.validate() {
            self.ui.builder(self.draft.clone(), Some(e));
            return;
        }
        let (Some(cat), Some(hero)) = (self.catalog.clone(), self.draft.hero.clone()) else {
            self.ui
                .builder(self.draft.clone(), Some(BuilderError::SaveFailed));
            return;
        };
        let sections: Vec<BuildSection> = self
            .draft
            .sections
            .iter()
            .filter(|s| !s.items.is_empty())
            .enumerate()
            .map(|(n, s)| BuildSection {
                title: if s.title.trim().is_empty() {
                    I18N.with(|i| i.t_n("builder-section-default", "n", n as i64 + 1))
                } else {
                    s.title.trim().to_owned()
                },
                item_ids: s.items.iter().map(|i| i.id).collect(),
            })
            .collect();
        // The ability order box is free text; builds store ability IDs, which need the ability
        // names from API-15. Not stored until the catalog exposes them.
        let build = Build::new_manual(hero.id, self.draft.name.trim(), cat.patch(), sections, None);
        match self.library.add(build, &cat) {
            Ok(_) => {
                self.draft = BuilderDraft::new();
                self.found.clear();
                self.ui.after_builder_saved();
                self.ui.builder(self.draft.clone(), None);
                self.load_builds();
            }
            Err(e) => {
                tracing::warn!("could not save the build: {e}");
                self.ui
                    .builder(self.draft.clone(), Some(BuilderError::SaveFailed));
            }
        }
    }
}

// ---- a match: Match Watcher, Broadcast, game state, scouting, Review -------------------------

impl Core {
    fn on_match_event(&mut self, ev: MatchEvent) {
        match ev {
            MatchEvent::Started => {
                self.match_started_seen = true;
                self.home_dirty = true;
            }
            MatchEvent::MatchId(id) => {
                if self.session.as_ref().map(|s| s.match_id) != Some(id) {
                    self.start_session(id, true);
                }
            }
            MatchEvent::Ended => {
                if let Some(s) = self.session.as_mut() {
                    s.console_ended = true;
                }
                self.maybe_finish();
                self.home_dirty = true;
            }
            MatchEvent::GameExited => {
                // Game closed. If the match never reported an end, keep what we have: the
                // saved match shows as pending and offers a retry.
                if let Some(s) = self.session.as_mut() {
                    s.console_ended = true;
                    s.lost = true;
                }
                self.maybe_finish();
                self.refresh_live();
                if let Some(s) = self.session.as_mut() {
                    flush_diag(&self.diag, s, self.api.rate_limit_waits());
                }
                self.session = None;
                self.match_started_seen = false;
                self.clear_live();
                self.home_dirty = true;
            }
        }
    }

    fn clear_live(&mut self) {
        self.vm.live = LiveVm {
            state: DataState::Empty,
            ..Default::default()
        };
        self.vm.scouting = ScoutingVm {
            state: DataState::Empty,
            ..Default::default()
        };
        self.ui.live(self.vm.live.clone());
        self.ui.scouting(self.vm.scouting.clone());
    }

    fn start_session(&mut self, match_id: MatchId, detected: bool) {
        self.session = None; // drops the old tasks
        self.next_gen += 1;
        let gen = self.next_gen;
        let mut tasks = Vec::new();

        // FR-DATA-005: only a match the Match Watcher found is recorded, which is the user's own.
        let recordings = detected.then(|| self.cfg.paths.recordings_dir());
        let deps = BroadcastDeps::live(
            self.api.clone(),
            &self.cfg.rules.broadcast,
            self.me,
            recordings,
        );
        let (btx, mut brx) = mpsc::channel::<BroadcastEvent>(64);
        let (bcast, bcast_stats) = broadcast::spawn_with_stats(match_id, deps, btx);
        let tx = self.tx.clone();
        tasks.push(tokio::spawn(async move {
            while let Some(ev) = brx.recv().await {
                if tx.send(Cmd::Broadcast(gen, ev)).is_err() {
                    break;
                }
            }
        }));

        let detect_latency_ms = if detected { self.latency.take() } else { None };
        self.broadcast_check = CheckState::Pending;
        self.match_started_seen = true;
        self.session = Some(Session {
            gen,
            match_id,
            detected,
            gs: GameState::new(match_id, self.me.unwrap_or(0), SystemTime::now()),
            plog: PurchaseLog::new(match_id),
            ls: None,
            delay_seen: false,
            connected: false,
            lost: false,
            broadcast_ended: false,
            console_ended: false,
            finished: false,
            mine: false,
            match_row_saved: false,
            purchases_saved: 0,
            scouted: false,
            roster: Vec::new(),
            cards: HashMap::new(),
            live_dirty: true,
            scout_dirty: true,
            tasks,
            bcast: Some(bcast),
            bcast_stats,
            waits_base: self.api.rate_limit_waits(),
            diag: MatchDiag {
                match_id,
                detect_latency_ms,
                ..Default::default()
            },
            diag_dirty: detect_latency_ms.is_some(),
            started: Instant::now(),
            scout_started: None,
            ever_connected: false,
        });
        self.vm.live = LiveVm {
            state: DataState::Loading,
            ..Default::default()
        };
        self.vm.scouting = ScoutingVm {
            state: DataState::Loading,
            ..Default::default()
        };
        self.ui.live(self.vm.live.clone());
        self.ui.scouting(self.vm.scouting.clone());
        self.home_dirty = true;
    }

    fn on_broadcast(&mut self, gen: u64, ev: BroadcastEvent) {
        let Some(s) = self.session.as_mut().filter(|s| s.gen == gen) else {
            return;
        };
        match ev.kind {
            BroadcastKind::Connected { .. } => {
                s.connected = true;
                s.lost = false;
                s.live_dirty = true;
                self.broadcast_check = CheckState::Pass;
                if !s.ever_connected {
                    s.ever_connected = true;
                    s.diag.broadcast_located = Some(true);
                    s.diag.broadcast_connect_ms = Some(ms(s.started));
                    s.diag_dirty = true;
                    flush_diag(&self.diag, s, self.api.rate_limit_waits());
                }
            }
            BroadcastKind::Parser(pe) => {
                if !matches!(pe, ParserEvent::End { .. }) {
                    s.gs.apply(&pe, SystemTime::now());
                    s.live_dirty = true;
                }
            }
            BroadcastKind::Delay { secs } => {
                s.gs.set_delay(secs);
                s.delay_seen = true;
                s.live_dirty = true;
                // The latest value goes to the file with the next write, not on every fragment.
                s.diag.delay_s = Some(secs.max(0.0).round() as u32);
                s.diag_dirty = true;
            }
            BroadcastKind::Lost => {
                s.lost = true;
                s.live_dirty = true;
                self.broadcast_check = CheckState::Fail;
                if !s.ever_connected {
                    // The relay was never found or never answered.
                    s.diag.broadcast_located = Some(false);
                    s.diag_dirty = true;
                    flush_diag(&self.diag, s, self.api.rate_limit_waits());
                }
            }
            BroadcastKind::Ended => {
                s.broadcast_ended = true;
                s.live_dirty = true;
            }
        }
        self.maybe_finish();
        // Only the Broadcast check changes here. The Steam files are not read again.
        let state = self.broadcast_check;
        if let Some(c) = self
            .vm
            .home
            .checks
            .iter_mut()
            .find(|c| c.id == CheckId::Broadcast)
        {
            if c.state != state {
                c.state = state;
                self.home_dirty = true;
            }
        }
    }

    fn on_scout(&mut self, gen: u64, card: ScoutingCard) {
        if let Some(s) = self.session.as_mut().filter(|s| s.gen == gen) {
            let key = vm_build::card_key(card.account_id, card.team, card.hero_id);
            // A lookup is done once its card leaves Loading; the first one is the time to the
            // first card. Updates to a card already counted don't count again.
            let done = !matches!(card.state, CardState::Loading | CardState::HeroOnly);
            let counted = s
                .cards
                .get(&key)
                .is_some_and(|c| !matches!(c.state, CardState::Loading | CardState::HeroOnly));
            if done && !counted {
                s.diag.scouting_lookups = Some(s.diag.scouting_lookups.unwrap_or(0) + 1);
                if matches!(card.state, CardState::Error { .. }) {
                    s.diag.scouting_failures = Some(s.diag.scouting_failures.unwrap_or(0) + 1);
                }
                if s.diag.first_card_ms.is_none() {
                    s.diag.first_card_ms = s.scout_started.map(ms);
                }
                s.diag_dirty = true;
            }
            s.cards.insert(key, card);
            s.scout_dirty = true;
        }
    }

    /// The match is over once the console says so and the Broadcast has ended or given up, or when
    /// the Broadcast reports the end itself. Then the Review starts (FR-REV-001).
    fn maybe_finish(&mut self) {
        let Some(s) = self.session.as_ref() else {
            return;
        };
        if s.finished {
            return;
        }
        let bcast_done = s.broadcast_ended || s.lost;
        let over = s.console_ended || s.broadcast_ended;
        if !(over && (bcast_done || !s.connected)) {
            return;
        }
        self.refresh_live();
        let Some(s) = self.session.as_mut() else {
            return;
        };
        s.finished = true;
        s.diag.purchase_events = Some(s.plog.events().len() as u32);
        s.diag_dirty = true;
        flush_diag(&self.diag, s, self.api.rate_limit_waits());
        let (id, mine) = (s.match_id, s.mine);
        if let Some(b) = s.bcast.take() {
            b.abort();
        }
        if mine {
            let waits = self.cfg.rules.review.api_poll_s.clone();
            self.start_review(id, waits);
        }
        self.home_dirty = true;
    }

    /// Rebuilds the Live and Scouting view models from the Live-Safe state only.
    fn refresh_live(&mut self) {
        let Some(mut s) = self.session.take() else {
            return;
        };
        let ls = live_safe::project_state(&s.gs, &self.whitelist);
        s.roster = vm_build::roster_from_live_safe(&ls);
        self.note_roster(&mut s, &ls);
        if s.live_dirty {
            s.mine = self
                .me
                .filter(|m| *m != 0)
                .is_some_and(|m| s.roster.iter().any(|e| e.account_id == m));
            if let Some(cat) = self.catalog.clone() {
                let new = s.plog.update(&ls, cat.as_ref());
                if s.mine {
                    self.save_match_row(&mut s, &ls);
                    if s.match_row_saved && !new.is_empty() {
                        if let Err(e) = self.storage.insert_purchase_events(&new) {
                            tracing::warn!("could not save purchases: {e}");
                        } else {
                            s.purchases_saved += new.len();
                        }
                    }
                }
            } else if s.mine {
                self.save_match_row(&mut s, &ls);
            }
        }
        if !s.scouted && !s.roster.is_empty() {
            self.spawn_scouting(&mut s);
        }
        let data_state = if s.lost && !s.finished {
            DataState::Error {
                source: "Broadcast".into(),
            }
        } else if !s.connected {
            DataState::Loading
        } else {
            DataState::Loaded
        };
        let fb = fallbacks();
        self.vm.live = vm_build::live_vm(
            &LiveInput {
                state: &ls,
                me: self.me,
                purchases: s.plog.events(),
                catalog: self.catalog.as_deref(),
                dirs: Some(&self.asset_dirs),
                delay_seen: s.delay_seen,
                stale_delay_s: self.cfg.rules.broadcast.stale_delay_s,
                data_state,
            },
            &fb,
        );
        // FP-10: cards are built from the start but stay hidden until the game has begun.
        self.vm.scouting = vm_build::scouting_vm(
            &ScoutInput {
                roster: &s.roster,
                cards: &s.cards,
                me: self.me,
                visible: scouting_visible(ls.t_game_s, &self.cfg.rules),
                history_window: self.cfg.rules.scouting.history_window,
                catalog: self.catalog.as_deref(),
            },
            &fb,
            &rank_text,
        );
        self.ui.live(self.vm.live.clone());
        self.ui.scouting(self.vm.scouting.clone());
        s.ls = Some(ls);
        s.live_dirty = false;
        s.scout_dirty = false;
        self.session = Some(s);
    }

    /// Gives the diagnostics what the roster shows: the other players (for the leak scan only),
    /// when the roster was complete, and how many items resolved against the catalog.
    fn note_roster(&self, s: &mut Session, ls: &LiveSafeState) {
        for p in ls.players.iter().filter(|p| Some(p.account_id) != self.me) {
            self.diag
                .redact_also(p.account_id, p.name.as_deref().unwrap_or(""));
        }
        // Written at a milestone (roster complete, first item rate). The later values go out with
        // the next milestone and at the end of the match, not on every update.
        let mut milestone = false;
        if s.diag.roster_complete_ms.is_none()
            && !ls.players.is_empty()
            && ls.players.iter().all(|p| p.hero_id != 0)
        {
            s.diag.roster_complete_ms = Some(ms(s.started));
            milestone = true;
        }
        if let Some(cat) = self.catalog.as_deref() {
            let ids: Vec<_> = ls
                .players
                .iter()
                .filter_map(|p| p.items.as_ref())
                .flatten()
                .collect();
            if !ids.is_empty() {
                let found = ids.iter().filter(|id| cat.item(***id).is_some()).count();
                let pct = (found as f32 * 1000.0 / ids.len() as f32).round() / 10.0;
                milestone |= s.diag.item_resolve_pct.is_none();
                s.diag.item_resolve_pct = Some(pct);
            }
        }
        if milestone {
            s.diag_dirty = true;
            flush_diag(&self.diag, s, self.api.rate_limit_waits());
        }
    }

    fn spawn_scouting(&self, s: &mut Session) {
        s.scouted = true;
        s.scout_started = Some(Instant::now());
        let (utx, mut urx) = mpsc::unbounded_channel::<ScoutingCard>();
        let gen = s.gen;
        let tx = self.tx.clone();
        s.tasks.push(tokio::spawn(async move {
            while let Some(card) = urx.recv().await {
                if tx.send(Cmd::Scout(gen, Box::new(card))).is_err() {
                    break;
                }
            }
        }));
        let service = ScoutingService::new(self.api.clone());
        let (roster, me, cfg) = (s.roster.clone(), self.me.unwrap_or(0), self.cfg.clone());
        s.tasks.push(tokio::spawn(async move {
            service.scout(&roster, me, &cfg.rules, utx).await;
        }));
    }

    /// Saves the match row once the roster has the user in it, so purchases and the Review have
    /// something to attach to (FR-REV-001). API-05 fills in the rest later.
    fn save_match_row(&self, s: &mut Session, ls: &LiveSafeState) {
        if s.match_row_saved {
            return;
        }
        let Some(me) = self.me.filter(|m| *m != 0) else {
            return;
        };
        let Some(mine) = ls.players.iter().find(|p| p.account_id == me) else {
            return;
        };
        let row = MatchRow {
            match_id: s.match_id,
            started_at: now_rfc3339(),
            duration_s: None,
            game_mode: GameMode::Other("unknown".into()),
            match_mode: None,
            patch: self.api.patch().unwrap_or_else(|| "unknown".into()),
            user_team: mine.team,
            user_hero_id: mine.hero_id,
            result: None,
            data_source: DataSource::Broadcast,
            recording_path: s.detected.then(|| {
                broadcast::recording_path(&self.cfg.paths.recordings_dir(), s.match_id)
                    .to_string_lossy()
                    .into_owned()
            }),
        };
        // Bots share account 0 and the table keys on the account, so they are left out.
        let players: Vec<MatchPlayerRow> = ls
            .players
            .iter()
            .filter(|p| p.account_id != 0)
            .map(|p| MatchPlayerRow {
                account_id: p.account_id,
                team: p.team,
                hero_id: p.hero_id,
                assigned_lane: p.lane,
                party_id: None,
            })
            .collect();
        match self.storage.insert_match(&row, &players) {
            Ok(()) => s.match_row_saved = true,
            Err(e) => tracing::warn!("could not save the match: {e}"),
        }
    }

    // ---- state shown on Home ----

    fn recompute_phase(&mut self) {
        let phase = match &self.session {
            None if self.reviews_running > 0 => Phase::Reviewing,
            None if self.match_started_seen => Phase::MatchDetected,
            None => Phase::Idle,
            Some(s) if s.finished => {
                if self.reviews_running > 0 {
                    Phase::Reviewing
                } else {
                    Phase::Idle
                }
            }
            Some(s) if s.console_ended => Phase::AwaitingBroadcastEnd,
            Some(s) if s.lost => Phase::LiveNoBroadcast,
            Some(s) if s.connected => Phase::Live,
            Some(_) => Phase::MatchDetected,
        };
        if phase != self.vm.home.phase {
            self.vm.home.phase = phase;
            self.home_dirty = true;
        }
    }

    fn flush(&mut self) {
        self.recompute_phase();
        self.log_checks();
        if std::mem::take(&mut self.home_dirty) {
            self.ui.home(self.vm.home.clone(), self.vm.offline);
        }
        if std::mem::take(&mut self.reviews_dirty) {
            self.ui.reviews(self.vm.reviews.clone());
        }
        if std::mem::take(&mut self.builds_dirty) {
            self.ui.builds(self.vm.builds.clone());
        }
        if std::mem::take(&mut self.settings_dirty) {
            self.ui.settings(self.vm.settings.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session(stats: broadcast::BroadcastStats, waits_base: u64) -> Session {
        Session {
            gen: 1,
            match_id: 1,
            detected: true,
            gs: GameState::new(1, 0, SystemTime::now()),
            plog: PurchaseLog::new(1),
            ls: None,
            delay_seen: false,
            connected: false,
            lost: false,
            broadcast_ended: false,
            console_ended: false,
            finished: false,
            mine: false,
            match_row_saved: false,
            purchases_saved: 0,
            scouted: false,
            roster: Vec::new(),
            cards: HashMap::new(),
            live_dirty: false,
            scout_dirty: false,
            tasks: Vec::new(),
            bcast: None,
            bcast_stats: stats,
            waits_base,
            diag: MatchDiag {
                match_id: 1,
                ..Default::default()
            },
            diag_dirty: false,
            started: Instant::now(),
            scout_started: None,
            ever_connected: false,
        }
    }

    #[test]
    fn latency_is_read_minus_line_time_and_never_negative() {
        let t = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        assert_eq!(
            latency_ms(Some(t), t + Duration::from_millis(1500)),
            Some(1500)
        );
        assert_eq!(latency_ms(Some(t + Duration::from_secs(3)), t), Some(0));
        assert_eq!(latency_ms(None, t), None);
    }

    #[test]
    fn latency_comes_from_started_else_the_first_timed_event() {
        let mut l = LatencyTracker::default();
        l.observe(true, Some(900));
        l.observe(false, Some(50));
        assert_eq!(l.take(), Some(900));
        // Next match: Started had no usable time, so the next event with one is used.
        l.observe(true, None);
        l.observe(false, None);
        l.observe(false, Some(70));
        l.observe(false, Some(80));
        assert_eq!(l.take(), Some(70));
        // A Started line resets whatever was left over.
        l.observe(true, Some(10));
        l.observe(true, None);
        assert_eq!(l.take(), None);
    }

    #[test]
    fn counters_are_this_matchs_difference_not_the_lifetime_total() {
        let stats = broadcast::BroadcastStats::default();
        let mut s = test_session(stats, 7);
        s.sync_counters(10);
        assert_eq!(s.diag.rate_limit_waits, Some(3));
        assert_eq!(s.diag.fragment_errors, Some(0));
        assert!(std::mem::take(&mut s.diag_dirty));
        s.sync_counters(10);
        assert!(!s.diag_dirty, "unchanged counters do not rewrite the file");
        s.sync_counters(12);
        assert_eq!(s.diag.rate_limit_waits, Some(5));
    }

    /// NFR-FAIR-003 and FP-4: the window code never touches the raw log, chat, positions or
    /// fight events. The only way to the Live view is `live_safe::project_state`.
    #[test]
    fn window_code_never_reads_raw_game_data() {
        for (name, src) in [
            ("runtime.rs", include_str!("runtime.rs")),
            ("vm_build.rs", include_str!("vm_build.rs")),
        ] {
            let production = src.split("#[cfg(test)]").next().unwrap();
            for line in production
                .lines()
                .filter(|l| !l.trim_start().starts_with("//"))
            {
                for banned in [
                    "review_log",
                    "ParserEvent::Chat",
                    "ParserEvent::Position",
                    "ParserEvent::Damage",
                    "ParserEvent::Death",
                    "gs.players",
                    "self_state",
                ] {
                    assert!(!line.contains(banned), "{name}: `{banned}` in `{line}`");
                }
            }
        }
    }

    #[test]
    fn rank_text_shows_tier_and_step() {
        assert_eq!(
            super::rank_text(64),
            "Tier 6, step 4".replace('\u{2068}', "")
        );
    }
}
