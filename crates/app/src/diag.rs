//! Alpha diagnostics wiring (FR-SET-005, DEC-070, T-019a): the app's own log file, the panic hook,
//! the facts about this install, and the save step behind Settings.
//!
//! What is recorded and how personal data is removed live in `stalemate_core::diagnostics`. This
//! module only gathers the app's own facts and passes them on. Nothing here reads game data.
//! Nothing is uploaded: the tester saves one zip and sends it by hand (NFR-PRIV-001).

use crate::i18n::I18n;
use stalemate_core::{
    account::SteamInfo,
    diagnostics::{About, Diagnostics, ExportError, ExportOptions, ExportSummary},
    settings::Settings,
    AccountId,
};
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use time::format_description::well_known::Rfc3339;
use tracing::{field::Visit, span, Event, Level, Metadata, Subscriber};

/// The version, plus where this build came from. Set at build time (`build.rs`).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub fn commit() -> &'static str {
    option_env!("STALEMATE_COMMIT").unwrap_or("unknown")
}

// ---- the log file --------------------------------------------------------------------------

/// Writes `tracing` events at info level and above to `<data>/logs/stalemate-YYYY-MM-DD.log`, one
/// file per day. Lines look like `2026-11-06T12:00:00Z  INFO target: message`, so the export's
/// level filter can read them.
pub struct FileLogger {
    dir: PathBuf,
    open: Mutex<(String, Option<File>)>,
    next_span: AtomicU64,
}

impl FileLogger {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            open: Mutex::new((String::new(), None)),
            next_span: AtomicU64::new(1),
        }
    }

    fn write_line(&self, line: &str) {
        let now = time::OffsetDateTime::now_utc();
        let stamp = now.format(&Rfc3339).unwrap_or_default();
        let day = stamp.get(..10).unwrap_or("unknown").to_string();
        let mut guard = self.open.lock().unwrap_or_else(|e| e.into_inner());
        if guard.0 != day || guard.1.is_none() {
            let _ = std::fs::create_dir_all(&self.dir);
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.dir.join(format!("stalemate-{day}.log")))
                .ok();
            *guard = (day, file);
        }
        if let Some(f) = guard.1.as_mut() {
            let _ = writeln!(f, "{stamp} {line}");
        }
    }
}

#[derive(Default)]
struct Fields {
    message: String,
    rest: String,
}

impl Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.message = format!("{value:?}");
        } else {
            self.rest.push_str(&format!(" {}={value:?}", field.name()));
        }
    }
}

impl Subscriber for FileLogger {
    fn enabled(&self, m: &Metadata<'_>) -> bool {
        // ERROR, WARN and INFO. Debug and trace never reach the file.
        *m.level() <= Level::INFO
    }
    fn new_span(&self, _: &span::Attributes<'_>) -> span::Id {
        span::Id::from_u64(self.next_span.fetch_add(1, Ordering::Relaxed))
    }
    fn record(&self, _: &span::Id, _: &span::Record<'_>) {}
    fn record_follows_from(&self, _: &span::Id, _: &span::Id) {}
    fn event(&self, e: &Event<'_>) {
        let mut f = Fields::default();
        e.record(&mut f);
        self.write_line(&format!(
            "{:>5} {}: {}{}",
            e.metadata().level(),
            e.metadata().target(),
            f.message,
            f.rest
        ));
    }
    fn enter(&self, _: &span::Id) {}
    fn exit(&self, _: &span::Id) {}
}

/// Starts file logging and the panic hook. Safe to call once; a second call changes nothing.
pub fn install_logging(data_dir: &Path) {
    let logger = FileLogger::new(data_dir.join("logs"));
    if tracing::subscriber::set_global_default(logger).is_err() {
        return;
    }
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // The backtrace is captured even when RUST_BACKTRACE is unset.
        let trace = std::backtrace::Backtrace::force_capture();
        tracing::error!("panic: {info}\n{trace}");
        previous(info);
    }));
}

/// Deletes log files older than `keep_days` (`[diagnostics] keep_days`).
pub fn prune_logs(dir: &Path, keep_days: u32) {
    let limit = std::time::Duration::from_secs(u64::from(keep_days) * 86_400);
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    for e in rd.flatten() {
        let old = e
            .metadata()
            .and_then(|m| m.modified())
            .ok()
            .and_then(|t| t.elapsed().ok())
            .is_some_and(|age| age > limit);
        if old && e.file_type().is_ok_and(|t| t.is_file()) {
            let _ = std::fs::remove_file(e.path());
        }
    }
}

// ---- facts about this install --------------------------------------------------------------

/// How this build was packaged. `FLATPAK_ID` means the Flatpak. Otherwise a one-word file
/// `package_format` next to the shipped `config/` folder (the packages write it: aur, deb, rpm,
/// appimage, tarball, msi, zip). Otherwise "dev".
pub fn package_format(shipped_config_dir: Option<&Path>) -> String {
    if std::env::var_os("FLATPAK_ID").is_some_and(|v| !v.is_empty()) {
        return "flatpak".into();
    }
    let word = shipped_config_dir
        .and_then(Path::parent)
        .and_then(|share| std::fs::read_to_string(share.join("package_format")).ok())
        .map(|t| t.trim().to_lowercase())
        .filter(|t| !t.is_empty() && t.len() <= 16 && t.chars().all(|c| c.is_ascii_alphanumeric()));
    word.unwrap_or_else(|| "dev".into())
}

/// "native" or "flatpak" from the Steam folder the app found, "not found" when there is none.
pub fn steam_kind(steam: &SteamInfo) -> String {
    match steam.roots.first() {
        None => "not found",
        Some(root) if root.to_string_lossy().contains("com.valvesoftware.Steam") => "flatpak",
        Some(_) => "native",
    }
    .into()
}

fn os_version() -> String {
    #[cfg(windows)]
    {
        // `ver` prints "Microsoft Windows [Version 10.0.22631.4602]".
        std::process::Command::new("cmd")
            .args(["/c", "ver"])
            .output()
            .ok()
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "unknown".into())
    }
    #[cfg(not(windows))]
    {
        // In the Flatpak, /etc/os-release is the runtime's; the host's is under /run/host.
        ["/run/host/os-release", "/etc/os-release"]
            .iter()
            .find_map(|p| std::fs::read_to_string(p).ok())
            .and_then(|t| {
                t.lines()
                    .find_map(|l| l.strip_prefix("PRETTY_NAME="))
                    .map(|v| v.trim_matches('"').to_string())
            })
            .unwrap_or_else(|| "unknown".into())
    }
}

fn desktop_session() -> String {
    if cfg!(windows) {
        return "Windows".into();
    }
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let mut chars = session.chars();
    let session = chars
        .next()
        .map(|c| c.to_uppercase().collect::<String>() + chars.as_str())
        .unwrap_or_default();
    let text = format!("{desktop} {session}").trim().to_string();
    if text.is_empty() {
        "unknown".into()
    } else {
        text
    }
}

pub fn about(shipped_config_dir: Option<&Path>, steam: &SteamInfo, scale: Option<f32>) -> About {
    About {
        version: VERSION.into(),
        commit: commit().into(),
        package_format: package_format(shipped_config_dir),
        os: std::env::consts::OS.into(),
        os_version: os_version(),
        desktop_session: desktop_session(),
        display_scale: scale,
        steam_kind: steam_kind(steam),
    }
}

/// `stalemate --version` output (TST-094).
pub fn version_line(shipped_config_dir: Option<&Path>) -> String {
    format!(
        "stalemate {VERSION} ({})",
        package_format(shipped_config_dir)
    )
}

// ---- the setting -----------------------------------------------------------------------------

const SETTING_FILE: &str = "diagnostics_enabled";

/// Moves the old one-word `<data>/diagnostics_enabled` file ("on" or "off") into the settings.
///
/// Runs once at start. If the setting is still unset, it takes the file's value and `save` writes
/// the settings; the file goes only after that save worked. A file next to an already chosen
/// setting is stale and is just removed. Returns true when the settings changed.
pub fn migrate_legacy_flag(
    data_dir: &Path,
    settings: &mut Settings,
    save: impl FnOnce(&Settings) -> bool,
) -> bool {
    let path = data_dir.join(SETTING_FILE);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return false;
    };
    let value = match text.trim().to_lowercase().as_str() {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    };
    let mut changed = false;
    if settings.diagnostics.enabled.is_none() {
        if let Some(v) = value {
            settings.diagnostics.enabled = Some(v);
            if !save(settings) {
                // Keep the file so the next start tries again.
                return true;
            }
            changed = true;
        }
    }
    let _ = std::fs::remove_file(&path);
    changed
}

// ---- saving ------------------------------------------------------------------------------------

/// Where the zip goes: the Downloads folder when it exists and can be written, otherwise a folder
/// the app can always write (`<data>/exports`). The Flatpak has no Downloads access, so it lands
/// in its own data folder.
pub fn export_dir(data_dir: &Path) -> PathBuf {
    if let Some(d) = dirs::download_dir().filter(|d| d.is_dir()) {
        let probe = d.join(".stalemate-write-test");
        if File::create(&probe).is_ok() {
            let _ = std::fs::remove_file(&probe);
            return d;
        }
    }
    data_dir.join("exports")
}

/// Everything `Diagnostics::export` needs besides the note and the Account ID choice.
#[derive(Clone)]
pub struct SaveEnv {
    pub diag: Diagnostics,
    pub log_dir: PathBuf,
    pub out_dir: PathBuf,
    pub about: About,
    pub api_key: Option<String>,
    pub me: Option<AccountId>,
}

impl SaveEnv {
    pub fn save(&self, note: &str, include_account_id: bool) -> Result<PathBuf, ExportError> {
        self.diag.export(
            &self.out_dir,
            &ExportOptions {
                note: note.to_string(),
                include_account_id,
                me: self.me,
                about: self.about.clone(),
                log_dir: self.log_dir.clone(),
                api_key: self.api_key.clone(),
                home_dir: dirs::home_dir(),
            },
        )
    }
}

/// What the Settings screen shows after pressing Save.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub enum SaveResult {
    #[default]
    None,
    Saved(PathBuf),
    /// The scan found forbidden data. Holds the kind only, never the data.
    Leak(String),
    Failed,
}

impl From<Result<PathBuf, ExportError>> for SaveResult {
    fn from(r: Result<PathBuf, ExportError>) -> Self {
        match r {
            Ok(p) => SaveResult::Saved(p),
            Err(ExportError::Leak { kind, .. }) => SaveResult::Leak(kind),
            Err(e) => {
                tracing::warn!("could not save the diagnostics file: {e}");
                SaveResult::Failed
            }
        }
    }
}

/// The preview lines: what the file will hold, in plain words.
pub fn preview_lines(i18n: &I18n, s: &ExportSummary) -> Vec<String> {
    let mut lines = vec![
        i18n.t("diag-preview-about"),
        i18n.t_n("diag-preview-checks", "n", s.checks as i64),
        i18n.t_n("diag-preview-matches", "n", s.matches as i64),
        i18n.t_n("diag-preview-perf", "n", s.perf_samples as i64),
        i18n.t_n("diag-preview-usage", "n", s.usage_actions as i64),
        i18n.t_n("diag-preview-markers", "n", s.markers as i64),
        i18n.t("diag-preview-log"),
    ];
    if let Some(d) = &s.oldest {
        lines.push(i18n.t_args(
            "diag-preview-oldest",
            &[("date", fluent_bundle::FluentValue::from(d.clone()))],
        ));
    }
    lines
}

/// The id a Home or setup check is stored under in `checks.json`.
pub fn check_key(id: crate::view_model::CheckId) -> &'static str {
    use crate::view_model::CheckId;
    match id {
        CheckId::Steam => "steam",
        CheckId::Condebug => "condebug",
        CheckId::AccountId => "account_id",
        CheckId::Broadcast => "broadcast",
        CheckId::DeadlockApi => "deadlock_api",
        CheckId::MatchWatcher => "match_watcher",
    }
}

/// The key `Diagnostics::usage` counts for a main view being opened. Counts only, no content.
pub fn view_key(view: i32) -> &'static str {
    // In the order of the left menu.
    const VIEWS: [&str; 6] = [
        "view-home",
        "view-scouting",
        "view-live",
        "view-reviews",
        "view-builds",
        "view-settings",
    ];
    usize::try_from(view)
        .ok()
        .and_then(|i| VIEWS.get(i))
        .copied()
        .unwrap_or("view-other")
}

/// "14:05 UTC", for the "Marked at" confirmation.
pub fn clock_utc() -> String {
    let t = time::OffsetDateTime::now_utc();
    format!("{:02}:{:02} UTC", t.hour(), t.minute())
}

/// Opens a folder in the file manager. Errors are ignored: the path is shown on screen too.
pub fn open_folder(path: &Path) {
    #[cfg(windows)]
    let (program, args): (&str, Vec<&std::ffi::OsStr>) = ("explorer", vec![path.as_os_str()]);
    #[cfg(not(windows))]
    let (program, args): (&str, Vec<&std::ffi::OsStr>) = ("xdg-open", vec![path.as_os_str()]);
    let _ = std::process::Command::new(program)
        .args(args)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_settings() -> Settings {
        let cfg = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../config");
        Settings::load_from(
            &cfg.join("settings.default.toml"),
            Path::new("/nonexistent/settings.toml"),
        )
        .unwrap()
    }

    #[test]
    fn log_file_holds_info_and_above_with_readable_level_words() {
        let dir = tempfile::tempdir().unwrap();
        let logger = FileLogger::new(dir.path().to_path_buf());
        tracing::subscriber::with_default(logger, || {
            tracing::info!("hello {}", 1);
            tracing::warn!("careful");
            tracing::debug!("hidden");
        });
        let file = std::fs::read_dir(dir.path())
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        assert!(file
            .file_name()
            .unwrap()
            .to_string_lossy()
            .starts_with("stalemate-"));
        let text = std::fs::read_to_string(file).unwrap();
        assert!(
            text.contains(" INFO ") && text.contains("hello 1"),
            "{text}"
        );
        assert!(text.contains(" WARN "), "{text}");
        assert!(!text.contains("hidden"), "{text}");
    }

    #[test]
    fn package_format_follows_the_contract() {
        let dir = tempfile::tempdir().unwrap();
        let share = dir.path().join("share");
        let config = share.join("config");
        std::fs::create_dir_all(&config).unwrap();
        // The Flatpak variable is not set in the test environment.
        if std::env::var_os("FLATPAK_ID").is_none() {
            assert_eq!(package_format(Some(&config)), "dev");
            std::fs::write(share.join("package_format"), "AppImage\n").unwrap();
            assert_eq!(package_format(Some(&config)), "appimage");
            std::fs::write(share.join("package_format"), "not a word at all\n").unwrap();
            assert_eq!(package_format(Some(&config)), "dev");
        }
        assert!(!package_format(None).is_empty());
    }

    #[test]
    fn version_line_names_version_and_format() {
        let line = version_line(None);
        assert!(
            line.starts_with(&format!("stalemate {VERSION} (")),
            "{line}"
        );
        assert!(line.ends_with(')'));
    }

    #[test]
    fn steam_kind_reads_the_root() {
        let mut s = SteamInfo::default();
        assert_eq!(steam_kind(&s), "not found");
        s.roots = vec!["/home/u/.local/share/Steam".into()];
        assert_eq!(steam_kind(&s), "native");
        s.roots = vec!["/home/u/.var/app/com.valvesoftware.Steam/.local/share/Steam".into()];
        assert_eq!(steam_kind(&s), "flatpak");
    }

    #[test]
    fn legacy_file_moves_into_settings_once_and_is_deleted() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(SETTING_FILE);
        std::fs::write(&file, "off").unwrap();
        let mut st = test_settings();
        let mut saved = None;
        assert!(migrate_legacy_flag(dir.path(), &mut st, |s| {
            saved = s.diagnostics.enabled;
            true
        }));
        assert_eq!(st.diagnostics.enabled, Some(false));
        assert_eq!(saved, Some(false));
        assert!(!file.exists());
        // Nothing left to do on the next start.
        assert!(!migrate_legacy_flag(dir.path(), &mut st, |_| panic!(
            "no save"
        )));
    }

    #[test]
    fn legacy_file_does_not_override_a_chosen_setting() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(SETTING_FILE);
        std::fs::write(&file, "off").unwrap();
        let mut st = test_settings();
        st.diagnostics.enabled = Some(true);
        assert!(!migrate_legacy_flag(dir.path(), &mut st, |_| panic!(
            "no save"
        )));
        assert_eq!(st.diagnostics.enabled, Some(true));
        assert!(!file.exists());
    }

    #[test]
    fn legacy_file_stays_when_the_save_fails() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(SETTING_FILE);
        std::fs::write(&file, "on").unwrap();
        let mut st = test_settings();
        migrate_legacy_flag(dir.path(), &mut st, |_| false);
        assert!(file.exists());
    }

    #[test]
    fn unreadable_legacy_file_leaves_the_setting_unset() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join(SETTING_FILE), "maybe").unwrap();
        let mut st = test_settings();
        assert!(!migrate_legacy_flag(dir.path(), &mut st, |_| panic!(
            "no save"
        )));
        assert_eq!(st.diagnostics.enabled, None);
    }
}
