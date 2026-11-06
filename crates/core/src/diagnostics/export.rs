//! Building the zip, with the privacy scan in front of every byte written.

use super::privacy::Privacy;
use super::store::{self, read_all};
use super::Inner;
use crate::AccountId;
use serde::Serialize;
use std::io::{Cursor, Write};
use std::path::{Path, PathBuf};
use time::OffsetDateTime;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

/// Facts about the install, filled by the app.
#[derive(Debug, Clone, Default, Serialize)]
pub struct About {
    pub version: String,
    pub commit: String,
    /// Flatpak, AUR, deb, rpm, AppImage, tarball, MSI or zip; set at build time.
    pub package_format: String,
    pub os: String,
    pub os_version: String,
    /// For example "KDE Wayland".
    pub desktop_session: String,
    pub display_scale: Option<f32>,
    /// "native" or "flatpak".
    pub steam_kind: String,
}

#[derive(Debug, Clone, Default)]
pub struct ExportOptions {
    /// The tester's own words. Scanned like everything else.
    pub note: String,
    /// Off by default in the UI.
    pub include_account_id: bool,
    pub me: Option<AccountId>,
    pub about: About,
    /// The app's own `tracing` log folder.
    pub log_dir: PathBuf,
    pub api_key: Option<String>,
    pub home_dir: Option<PathBuf>,
}

/// What will be in the file, for the preview.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ExportSummary {
    pub enabled: bool,
    pub checks: usize,
    pub matches: usize,
    pub perf_samples: usize,
    /// Total of all usage counters.
    pub usage_actions: u64,
    pub markers: usize,
    /// Date of the oldest kept record (YYYY-MM-DD), if any.
    pub oldest: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ExportError {
    /// Forbidden data was still present after rewriting. Nothing was written. The message names
    /// the kind and the entry, never the data.
    #[error("the {kind} was found in {entry}; the file was not saved")]
    Leak { entry: String, kind: String },
    #[error("diagnostics file I/O: {0}")]
    Io(String),
    #[error("the diagnostics file cannot fit in the size limit")]
    TooLarge,
}

impl From<std::io::Error> for ExportError {
    fn from(e: std::io::Error) -> Self {
        ExportError::Io(e.to_string())
    }
}
impl From<zip::result::ZipError> for ExportError {
    fn from(e: zip::result::ZipError) -> Self {
        ExportError::Io(e.to_string())
    }
}

/// Log files, newest first. Lines below info level are dropped.
fn read_logs(dir: &Path) -> Vec<(String, Vec<String>)> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files: Vec<(std::time::SystemTime, String, PathBuf)> = rd
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .map(|e| {
            let m = e
                .metadata()
                .and_then(|m| m.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (m, e.file_name().to_string_lossy().into_owned(), e.path())
        })
        .collect();
    files.sort_by(|a, b| b.cmp(a));
    files
        .into_iter()
        .filter_map(|(_, name, p)| {
            let bytes = std::fs::read(p).ok()?;
            let text = String::from_utf8_lossy(&bytes);
            let lines = text
                .lines()
                .filter(|l| !(l.contains(" DEBUG ") || l.contains(" TRACE ")))
                .map(str::to_string)
                .collect();
            Some((name, lines))
        })
        .collect()
}

pub(crate) fn summary(inner: &Inner) -> ExportSummary {
    let f = read_all(inner);
    ExportSummary {
        enabled: inner.enabled.load(std::sync::atomic::Ordering::Relaxed),
        checks: f.checks.len(),
        matches: f.matches.len(),
        perf_samples: f.perf.len(),
        usage_actions: f.usage.values().sum(),
        markers: f.markers.len(),
        oldest: f.oldest,
    }
}

fn jline<T: Serialize>(v: &T) -> String {
    serde_json::to_string(v).unwrap_or_default() + "\n"
}

pub(crate) fn export(
    inner: &Inner,
    out_dir: &Path,
    opts: &ExportOptions,
) -> Result<PathBuf, ExportError> {
    let f = read_all(inner);

    let mut privacy = Privacy::new(opts.home_dir.as_deref(), opts.api_key.as_deref());
    {
        let o = inner.others.lock().unwrap_or_else(|e| e.into_inner());
        privacy.other_ids = o.ids.iter().copied().collect();
        privacy.other_names = o.names.iter().cloned().collect();
    }
    if !opts.include_account_id {
        privacy.hide_me = opts.me;
    }

    // Fixed entries, as text.
    let mut about = serde_json::to_value(&opts.about).unwrap_or_default();
    if let (true, Some(me), Some(o)) = (opts.include_account_id, opts.me, about.as_object_mut()) {
        o.insert("account_id".into(), me.into());
    }
    let checks: Vec<_> = f
        .checks
        .iter()
        .map(|(id, (t, ok, detail))| serde_json::json!({"id": id, "ok": ok, "detail": detail, "t": t}))
        .collect();
    let mut fixed: Vec<(String, String)> = vec![
        (
            "about.json".into(),
            serde_json::to_string_pretty(&about).unwrap_or_default() + "\n",
        ),
        (
            "checks.json".into(),
            serde_json::to_string_pretty(&checks).unwrap_or_default() + "\n",
        ),
        (
            "matches.jsonl".into(),
            f.matches.iter().map(jline).collect(),
        ),
        (
            "perf.jsonl".into(),
            f.perf
                .iter()
                .map(|(t, s)| {
                    jline(&serde_json::json!({"t": t, "cpu_pct": s.cpu_pct, "rss_mb": s.rss_mb}))
                })
                .collect(),
        ),
        (
            "usage.json".into(),
            serde_json::to_string_pretty(&f.usage).unwrap_or_default() + "\n",
        ),
        (
            "markers.jsonl".into(),
            f.markers
                .iter()
                .map(|(t, n)| jline(&serde_json::json!({"t": t, "note": n})))
                .collect(),
        ),
        ("note.txt".into(), opts.note.clone() + "\n"),
    ];

    // Rewrite, then scan, every entry before anything is written.
    let check = |name: &str, text: &str| -> Result<String, ExportError> {
        let out = privacy.rewrite(text);
        match privacy.scan(&out) {
            Some(kind) => Err(ExportError::Leak {
                entry: name.to_string(),
                kind: kind.to_string(),
            }),
            None => Ok(out),
        }
    };
    for (name, text) in fixed.iter_mut() {
        *text = check(name, text)?;
    }
    let mut logs: Vec<(String, Vec<String>)> = Vec::new();
    for (name, lines) in read_logs(&opts.log_dir) {
        let entry = format!("log/{name}");
        let mut clean = Vec::with_capacity(lines.len());
        for l in &lines {
            clean.push(check(&entry, l)?);
        }
        logs.push((entry, clean));
    }

    // Size: the zip is never larger than its inputs plus a little overhead, so budget the log
    // by uncompressed size and shrink if the result still overshoots.
    let fixed_bytes: u64 = fixed.iter().map(|(_, t)| t.len() as u64).sum();
    let mut log_budget = inner.max_bytes.saturating_sub(fixed_bytes + 4096);
    let bytes = loop {
        let data = build_zip(&fixed, &logs, log_budget)?;
        if data.len() as u64 <= inner.max_bytes {
            break data;
        }
        if log_budget == 0 {
            return Err(ExportError::TooLarge);
        }
        log_budget /= 2;
    };

    std::fs::create_dir_all(out_dir)?;
    let date = store::date_str(OffsetDateTime::now_utc().date());
    let path = out_dir.join(format!("stalemate-diagnostics-{date}.zip"));
    let tmp = out_dir.join(format!("stalemate-diagnostics-{date}.zip.part"));
    std::fs::write(&tmp, &bytes)?;
    std::fs::rename(&tmp, &path)?;
    Ok(path)
}

/// Newest log files first; inside a file the oldest lines go first when over budget.
fn build_zip(
    fixed: &[(String, String)],
    logs: &[(String, Vec<String>)],
    mut budget: u64,
) -> Result<Vec<u8>, ExportError> {
    let opts = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    let mut zw = ZipWriter::new(Cursor::new(Vec::new()));
    for (name, text) in fixed {
        zw.start_file(name.as_str(), opts)?;
        zw.write_all(text.as_bytes())?;
    }
    for (name, lines) in logs {
        if budget == 0 {
            break;
        }
        let mut kept: Vec<&str> = Vec::new();
        let mut used = 0u64;
        for l in lines.iter().rev() {
            let n = l.len() as u64 + 1;
            if used + n > budget {
                break;
            }
            used += n;
            kept.push(l);
        }
        if kept.is_empty() {
            break;
        }
        kept.reverse();
        budget -= used;
        zw.start_file(name.as_str(), opts)?;
        for l in kept {
            zw.write_all(l.as_bytes())?;
            zw.write_all(b"\n")?;
        }
    }
    Ok(zw.finish()?.into_inner())
}
