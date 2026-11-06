//! M-02 Account resolver (PRG-02). See 04-data-sources-and-ingestion.md.
//!
//! Reads local Steam files only (FP-3). Never calls the Steam Web API (NFR-FAIR-005), so vanity
//! profile URLs (`/id/<name>`) are rejected: resolving them would need that API.

use crate::types::AccountId;
use keyvalues_parser::{Obj, Value, Vdf};
use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

pub const STEAMID64_BASE: u64 = 76_561_197_960_265_728;

/// Folder name of the Deadlock install under `steamapps/common` (doc 04).
const DEADLOCK_DIR_NAME: &str = "Deadlock";

#[derive(Debug, PartialEq, thiserror::Error)]
pub enum AccountError {
    #[error(
        "empty input: enter a SteamID64, an Account ID or a steamcommunity.com/profiles/ link"
    )]
    Empty,
    #[error(
        "vanity profile links (steamcommunity.com/id/<name>) can't be resolved without the Steam Web API, \
         which this app never calls. Open your profile in a browser and copy the numeric link \
         (steamcommunity.com/profiles/<number>) instead"
    )]
    VanityUrl,
    #[error("not a SteamID64, an Account ID or a steamcommunity.com/profiles/ link: {0:?}")]
    Unrecognized(String),
    #[error("{0} is not a valid Steam ID (SteamID64 values start at 76561197960265728)")]
    OutOfRange(u64),
}

pub fn account_id_from_steamid64(id64: u64) -> Option<u32> {
    id64.checked_sub(STEAMID64_BASE)
        .and_then(|v| u32::try_from(v).ok())
}

/// Manual entry (FR-DATA-006): a SteamID64, a 32-bit Account ID, or a `steamcommunity.com/profiles/<id64>` URL.
pub fn parse_manual_account(input: &str) -> Result<AccountId, AccountError> {
    let s = input.trim();
    if s.is_empty() {
        return Err(AccountError::Empty);
    }
    if s.bytes().all(|b| b.is_ascii_digit()) {
        let n: u64 = s
            .parse()
            .map_err(|_| AccountError::Unrecognized(s.into()))?;
        // A 32-bit value is an Account ID; anything larger must be a SteamID64.
        return if n <= u64::from(u32::MAX) {
            Ok(n as u32)
        } else {
            account_id_from_steamid64(n).ok_or(AccountError::OutOfRange(n))
        };
    }
    let lower = s.to_ascii_lowercase();
    let after_host = lower
        .split_once("steamcommunity.com/")
        .map(|(_, rest)| rest)
        .ok_or_else(|| AccountError::Unrecognized(s.into()))?;
    let mut parts = after_host.split(['/', '?', '#']);
    match (parts.next(), parts.next()) {
        (Some("id"), _) => Err(AccountError::VanityUrl),
        (Some("profiles"), Some(id))
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) =>
        {
            let n: u64 = id
                .parse()
                .map_err(|_| AccountError::Unrecognized(s.into()))?;
            account_id_from_steamid64(n).ok_or(AccountError::OutOfRange(n))
        }
        _ => Err(AccountError::Unrecognized(s.into())),
    }
}

pub fn candidate_roots(home: &Path) -> Vec<PathBuf> {
    #[cfg(target_os = "linux")]
    return vec![
        home.join(".local/share/Steam"),
        home.join(".steam/steam"),
        home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"),
    ];
    #[cfg(windows)]
    {
        let _ = home;
        return vec![PathBuf::from(r"C:\Program Files (x86)\Steam")];
    }
    #[cfg(not(any(target_os = "linux", windows)))]
    {
        let _ = home;
        Vec::new()
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SteamInfo {
    /// Steam roots that exist on this machine (duplicates through symlinks removed).
    pub roots: Vec<PathBuf>,
    /// Library folders: the roots, the extra ones from settings, and everything in `libraryfolders.vdf`.
    pub libraries: Vec<PathBuf>,
    pub deadlock_dir: Option<PathBuf>,
    pub console_log: Option<PathBuf>,
    /// From `loginusers.vdf`; `None` means ask the user (FR-DATA-006).
    pub account_id: Option<AccountId>,
}

/// Find Steam, the Deadlock install and the user's Account ID.
/// `extra_libraries` comes from settings (NFR-PLAT-002).
pub fn resolve(home: &Path, extra_libraries: &[PathBuf]) -> SteamInfo {
    resolve_from_roots(&candidate_roots(home), extra_libraries)
}

pub fn resolve_from_roots(candidates: &[PathBuf], extra_libraries: &[PathBuf]) -> SteamInfo {
    let mut info = SteamInfo::default();
    let mut seen: Vec<PathBuf> = Vec::new();
    for root in candidates {
        let Ok(canon) = root.canonicalize() else {
            continue;
        };
        if seen.contains(&canon) {
            continue; // e.g. ~/.steam/steam is a symlink to ~/.local/share/Steam
        }
        seen.push(canon);
        info.roots.push(root.clone());
    }

    let mut libs: Vec<PathBuf> = Vec::new();
    let mut add = |p: PathBuf| {
        if !libs.contains(&p) {
            libs.push(p);
        }
    };
    for root in &info.roots {
        add(root.clone());
        if info.account_id.is_none() {
            info.account_id = read_text(&root.join("config/loginusers.vdf"))
                .and_then(|t| most_recent_account_id(&t));
        }
        if let Some(t) = read_text(&root.join("steamapps/libraryfolders.vdf")) {
            for p in library_paths(&t) {
                add(p);
            }
        }
    }
    for p in extra_libraries {
        add(p.clone());
    }

    info.deadlock_dir = libs
        .iter()
        .map(|l| l.join("steamapps/common").join(DEADLOCK_DIR_NAME))
        .find(|d| d.is_dir());
    info.console_log = info
        .deadlock_dir
        .as_ref()
        .map(|d| d.join("game/citadel/console.log")); // VERIFY OQ-03
    info.libraries = libs;
    info
}

fn read_text(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Case-insensitive child lookup. Steam is inconsistent about key case across files and versions.
fn child<'a>(obj: &'a Obj<'a>, name: &str) -> Option<&'a Value<'a>> {
    obj.iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .and_then(|(_, v)| v.first())
}

fn child_obj<'a>(obj: &'a Obj<'_>, name: &str) -> Option<&'a Obj<'a>> {
    child(obj, name)?.get_obj()
}

fn child_str<'a>(obj: &'a Obj<'_>, name: &str) -> Option<&'a str> {
    child(obj, name)?.get_str()
}

/// The Account ID of the `loginusers.vdf` entry with `MostRecent = "1"`.
pub fn most_recent_account_id(loginusers: &str) -> Option<AccountId> {
    let vdf: Vdf = keyvalues_parser::parse(loginusers).ok()?.into_vdf();
    let users = vdf.value.get_obj()?;
    users
        .iter()
        .filter_map(|(id, vals)| Some((id, vals.first()?.get_obj()?)))
        .find(|(_, u)| child_str(u, "MostRecent") == Some("1"))
        .and_then(|(id, _)| id.parse::<u64>().ok())
        .and_then(account_id_from_steamid64)
}

/// Library paths in `libraryfolders.vdf`. Handles the current format (`"0" { "path" "..." }`)
/// and the old one (`"1" "/path"`).
pub fn library_paths(libraryfolders: &str) -> Vec<PathBuf> {
    let Ok(vdf) = keyvalues_parser::parse(libraryfolders) else {
        return Vec::new();
    };
    let vdf = vdf.into_vdf();
    let Some(root) = vdf.value.get_obj() else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (key, vals) in root.iter() {
        if key.is_empty() || !key.bytes().all(|b| b.is_ascii_digit()) {
            continue; // skips "contentstatsid" and similar
        }
        for v in vals {
            let path = match v {
                Value::Str(s) => Some(s.as_ref()),
                Value::Obj(o) => child_str(o, "path"),
            };
            if let Some(p) = path.filter(|p| !p.is_empty()) {
                out.push(PathBuf::from(p));
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------------------------
// First-run -condebug check (T-018, FR-SET-003)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum ConsoleLogState {
    Missing,
    Present {
        len: u64,
        modified: Option<SystemTime>,
    },
}

pub fn console_log_state(path: &Path) -> ConsoleLogState {
    match std::fs::metadata(path) {
        Ok(m) if m.is_file() => ConsoleLogState::Present {
            len: m.len(),
            modified: m.modified().ok(),
        },
        _ => ConsoleLogState::Missing,
    }
}

/// Deadlock's launch options text from `localconfig.vdf`, if the file and the entry exist.
pub fn launch_options(localconfig: &str, app_id: u32) -> Option<String> {
    let vdf = keyvalues_parser::parse(localconfig).ok()?.into_vdf();
    let mut node = vdf.value.get_obj()?;
    for name in ["Software", "Valve", "Steam", "apps"] {
        node = child_obj(node, name)?;
    }
    let app = child_obj(node, &app_id.to_string())?;
    child_str(app, "LaunchOptions").map(String::from)
}

/// Does Deadlock's Steam launch option contain `-condebug`? Reads
/// `<steam_root>/userdata/<account_id>/config/localconfig.vdf` (FP-3 allows reading Steam config).
/// `None` means it can't be told (no file, or no entry); the setup then falls back to checking
/// whether `console.log` appears.
pub fn launch_options_have_condebug(
    steam_root: &Path,
    account_id: AccountId,
    deadlock_app_id: u32,
) -> Option<bool> {
    let path = steam_root
        .join("userdata")
        .join(account_id.to_string())
        .join("config/localconfig.vdf");
    let text = read_text(&path)?;
    // Parsed but no entry for the app means no launch options are set at all.
    let _ = keyvalues_parser::parse(&text).ok()?;
    Some(
        launch_options(&text, deadlock_app_id)
            .map(|o| {
                o.split_whitespace()
                    .any(|w| w.eq_ignore_ascii_case("-condebug"))
            })
            .unwrap_or(false),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn id64(account: u32) -> u64 {
        STEAMID64_BASE + u64::from(account)
    }

    fn loginusers(users: &[(u32, bool)]) -> String {
        let mut s = String::from("\"users\"\n{\n");
        for (acc, recent) in users {
            s += &format!(
                "\t\"{}\"\n\t{{\n\t\t\"AccountName\"\t\"x\"\n\t\t\"MostRecent\"\t\"{}\"\n\t}}\n",
                id64(*acc),
                if *recent { 1 } else { 0 }
            );
        }
        s + "}\n"
    }

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    fn library_vdf(paths: &[&Path]) -> String {
        let mut s = String::from("\"libraryfolders\"\n{\n");
        for (i, p) in paths.iter().enumerate() {
            s += &format!(
                "\t\"{i}\"\n\t{{\n\t\t\"path\"\t\"{}\"\n\t\t\"label\"\t\"\"\n\t\t\"apps\"\n\t\t{{\n\t\t\t\"1422450\"\t\"1\"\n\t\t}}\n\t}}\n",
                p.display()
            );
        }
        s + "}\n"
    }

    #[test]
    fn tc_02_1_known_steamid64() {
        assert_eq!(account_id_from_steamid64(76561197960287930), Some(22202));
    }

    #[test]
    fn tc_02_2_small_value_is_none() {
        assert_eq!(account_id_from_steamid64(1), None);
        assert_eq!(
            account_id_from_steamid64(STEAMID64_BASE + u64::from(u32::MAX) + 1),
            None
        );
    }

    // Uses the Linux candidate roots under a temp home.
    #[cfg(target_os = "linux")]
    #[test]
    fn tc_02_3_most_recent_user_wins() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".local/share/Steam");
        write(
            &root.join("config/loginusers.vdf"),
            &loginusers(&[(111, false), (22202, true), (333, false)]),
        );
        let info = resolve(tmp.path(), &[]);
        assert_eq!(info.account_id, Some(22202));
    }

    #[test]
    fn no_most_recent_user_gives_none() {
        assert_eq!(
            most_recent_account_id(&loginusers(&[(1, false), (2, false)])),
            None
        );
        assert_eq!(most_recent_account_id("not vdf {{{"), None);
    }

    // Uses the Linux candidate roots under a temp home.
    #[cfg(target_os = "linux")]
    #[test]
    fn tc_02_4_deadlock_in_second_library() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".local/share/Steam");
        let second = tmp.path().join("games");
        write(
            &root.join("config/loginusers.vdf"),
            &loginusers(&[(5, true)]),
        );
        write(
            &root.join("steamapps/libraryfolders.vdf"),
            &library_vdf(&[&root, &second]),
        );
        fs::create_dir_all(second.join("steamapps/common/Deadlock/game/citadel")).unwrap();
        let info = resolve(tmp.path(), &[]);
        assert_eq!(
            info.deadlock_dir,
            Some(second.join("steamapps/common/Deadlock"))
        );
        assert_eq!(
            info.console_log,
            Some(second.join("steamapps/common/Deadlock/game/citadel/console.log"))
        );
        assert!(info.libraries.contains(&second));
    }

    // Uses the Linux candidate roots under a temp home.
    #[cfg(target_os = "linux")]
    #[test]
    fn tc_02_5_flatpak_root_only() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp
            .path()
            .join(".var/app/com.valvesoftware.Steam/.local/share/Steam");
        write(
            &root.join("config/loginusers.vdf"),
            &loginusers(&[(22202, true)]),
        );
        fs::create_dir_all(root.join("steamapps/common/Deadlock")).unwrap();
        let info = resolve(tmp.path(), &[]);
        assert_eq!(info.roots, vec![root.clone()]);
        assert_eq!(info.account_id, Some(22202));
        assert_eq!(
            info.deadlock_dir,
            Some(root.join("steamapps/common/Deadlock"))
        );
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_roots_are_listed_once() {
        let tmp = tempfile::tempdir().unwrap();
        let real = tmp.path().join(".local/share/Steam");
        fs::create_dir_all(&real).unwrap();
        fs::create_dir_all(tmp.path().join(".steam")).unwrap();
        std::os::unix::fs::symlink(&real, tmp.path().join(".steam/steam")).unwrap();
        assert_eq!(resolve(tmp.path(), &[]).roots, vec![real]);
    }

    // Uses the Linux candidate roots under a temp home.
    #[cfg(target_os = "linux")]
    #[test]
    fn nothing_installed_resolves_to_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(resolve(tmp.path(), &[]), SteamInfo::default());
    }

    // Uses the Linux candidate roots under a temp home.
    #[cfg(target_os = "linux")]
    #[test]
    fn extra_library_from_settings_is_searched() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path().join(".local/share/Steam");
        fs::create_dir_all(&root).unwrap();
        let extra = tmp.path().join("mnt/steam");
        fs::create_dir_all(extra.join("steamapps/common/Deadlock")).unwrap();
        let info = resolve(tmp.path(), std::slice::from_ref(&extra));
        assert_eq!(
            info.deadlock_dir,
            Some(extra.join("steamapps/common/Deadlock"))
        );
    }

    #[test]
    fn old_style_library_folders_are_read() {
        let vdf = "\"LibraryFolders\"\n{\n\t\"TimeNextStatsReport\"\t\"1\"\n\t\"contentstatsid\"\t\"2\"\n\t\"1\"\t\"/mnt/old\"\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from("/mnt/old")]);
    }

    #[test]
    fn windows_paths_unescape() {
        let vdf =
            "\"libraryfolders\"\n{\n\t\"0\"\n\t{\n\t\t\"path\"\t\"D:\\\\SteamLibrary\"\n\t}\n}\n";
        assert_eq!(library_paths(vdf), vec![PathBuf::from(r"D:\SteamLibrary")]);
    }

    // Manual entry (FR-DATA-006, TST-015)

    #[test]
    fn manual_entry_accepts_three_forms() {
        assert_eq!(parse_manual_account("22202"), Ok(22202));
        assert_eq!(parse_manual_account(" 76561197960287930 "), Ok(22202));
        assert_eq!(
            parse_manual_account("https://steamcommunity.com/profiles/76561197960287930/"),
            Ok(22202)
        );
        assert_eq!(
            parse_manual_account("steamcommunity.com/profiles/76561197960287930?x=1"),
            Ok(22202)
        );
    }

    #[test]
    fn manual_entry_rejects_vanity_urls_clearly() {
        let e = parse_manual_account("https://steamcommunity.com/id/gaben").unwrap_err();
        assert!(matches!(e, AccountError::VanityUrl));
        assert!(e.to_string().contains("Steam Web API"));
    }

    #[test]
    fn manual_entry_rejects_junk() {
        for bad in [
            "",
            "   ",
            "abc",
            "https://example.com/profiles/765",
            "-5",
            "1e5",
            "steamcommunity.com/profiles/abc",
        ] {
            assert!(parse_manual_account(bad).is_err(), "{bad:?}");
        }
        // Too big for a u32 and below the SteamID64 base is impossible; above u32 but below base:
        assert!(matches!(
            parse_manual_account("5000000000"),
            Err(AccountError::OutOfRange(_))
        ));
    }

    #[test]
    fn manual_entry_account_id_boundary() {
        assert_eq!(parse_manual_account("4294967295"), Ok(u32::MAX));
    }

    // -condebug check (T-018)

    fn localconfig(options: Option<&str>) -> String {
        let opt = options
            .map(|o| format!("\t\t\t\t\t\"LaunchOptions\"\t\"{o}\"\n"))
            .unwrap_or_default();
        format!(
            "\"UserLocalConfigStore\"\n{{\n\t\"Software\"\n\t{{\n\t\t\"Valve\"\n\t\t{{\n\t\t\t\"Steam\"\n\t\t\t{{\n\t\t\t\t\"apps\"\n\t\t\t\t{{\n\t\t\t\t\t\"1422450\"\n\t\t\t\t\t{{\n{opt}\t\t\t\t\t}}\n\t\t\t\t\t\"730\"\n\t\t\t\t\t{{\n\t\t\t\t\t\t\"LaunchOptions\"\t\"-condebug\"\n\t\t\t\t\t}}\n\t\t\t\t}}\n\t\t\t}}\n\t\t}}\n\t}}\n}}\n"
        )
    }

    fn check(options: Option<&str>) -> Option<bool> {
        let tmp = tempfile::tempdir().unwrap();
        write(
            &tmp.path().join("userdata/22202/config/localconfig.vdf"),
            &localconfig(options),
        );
        launch_options_have_condebug(tmp.path(), 22202, 1422450)
    }

    #[test]
    fn condebug_detected_and_not_confused_with_other_apps() {
        assert_eq!(check(Some("-condebug")), Some(true));
        assert_eq!(check(Some("-novid -condebug +fps_max 0")), Some(true));
        assert_eq!(check(Some("-novid")), Some(false));
        assert_eq!(check(Some("-condebugger")), Some(false));
        assert_eq!(
            check(None),
            Some(false),
            "app 730 has the flag, Deadlock doesn't"
        );
    }

    #[test]
    fn condebug_unknown_without_file() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(
            launch_options_have_condebug(tmp.path(), 22202, 1422450),
            None
        );
    }

    #[test]
    fn console_log_state_reports_missing_then_present() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path().join("console.log");
        assert_eq!(console_log_state(&p), ConsoleLogState::Missing);
        fs::write(&p, "abc").unwrap();
        match console_log_state(&p) {
            ConsoleLogState::Present { len, modified } => {
                assert_eq!(len, 3);
                assert!(modified.is_some());
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            console_log_state(tmp.path()),
            ConsoleLogState::Missing,
            "a directory isn't a log"
        );
    }
}
