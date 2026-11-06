//! Alpha diagnostics file (FR-SET-005, DEC-070, TST-108). A local, append-only record of what
//! the app detected, timings, errors and performance. Nothing is uploaded (NFR-PRIV-001): the
//! tester saves one zip and sends it by hand.
//!
//! Fair play (doc 03): this module holds only the app's own observations. No API here takes
//! chat, raw Broadcast data or `console.log` text. Other players' IDs and names enter only
//! through [`Diagnostics::redact_also`], and are used by the leak scan, never written.

mod export;
mod perf;
mod privacy;
mod store;

pub use export::{About, ExportError, ExportOptions, ExportSummary};
pub use perf::sample_self;
pub use store::{MatchDiag, PerfSample};

use crate::settings::DiagnosticsRules;
use crate::AccountId;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Cheap to clone and safe to share between threads.
#[derive(Clone)]
pub struct Diagnostics {
    inner: Arc<Inner>,
}

pub(crate) struct Inner {
    pub dir: PathBuf,
    pub keep_days: u32,
    pub max_bytes: u64,
    pub enabled: AtomicBool,
    pub writer: Mutex<store::Writer>,
    pub others: Mutex<Others>,
}

#[derive(Default)]
pub(crate) struct Others {
    pub ids: HashSet<AccountId>,
    pub names: HashSet<String>,
}

impl Diagnostics {
    /// `dir` is `<data>/diagnostics`. Nothing is created until the first write. Old files roll
    /// off here and again when the day changes.
    pub fn open(dir: &Path, rules: &DiagnosticsRules, enabled: bool) -> Self {
        let _ = sample_self(); // baseline for the first CPU reading
        let d = Diagnostics {
            inner: Arc::new(Inner {
                dir: dir.to_path_buf(),
                keep_days: rules.keep_days,
                max_bytes: u64::from(rules.max_file_mb) * 1024 * 1024,
                enabled: AtomicBool::new(enabled),
                writer: Mutex::new(store::Writer::default()),
                others: Mutex::new(Others::default()),
            }),
        };
        store::roll_off(&d.inner);
        d
    }

    pub fn set_enabled(&self, on: bool) {
        self.inner.enabled.store(on, Ordering::Relaxed);
    }

    pub fn is_enabled(&self) -> bool {
        self.inner.enabled.load(Ordering::Relaxed)
    }

    /// A Home or setup check and its result. The latest result per `id` is kept in the file.
    pub fn check(&self, id: &str, ok: bool, detail: &str) {
        store::append(
            &self.inner,
            store::Body::Check {
                id: id.into(),
                ok,
                detail: detail.into(),
            },
        );
    }

    /// Upsert by `match_id`: fields that are `Some` replace earlier values, `None` keeps them.
    pub fn match_record(&self, rec: &MatchDiag) {
        store::append(&self.inner, store::Body::Match { rec: rec.clone() });
    }

    pub fn perf(&self, s: PerfSample) {
        store::append(
            &self.inner,
            store::Body::Perf {
                cpu_pct: s.cpu_pct,
                rss_mb: s.rss_mb,
            },
        );
    }

    /// Counts one use of `key` (a view opened or a main action). Counts only, no content.
    pub fn usage(&self, key: &str) {
        store::append(&self.inner, store::Body::Usage { key: key.into() });
    }

    /// "Mark a problem": a timestamped note, which may be empty.
    pub fn marker(&self, note: &str) {
        store::append(&self.inner, store::Body::Marker { note: note.into() });
    }

    /// Another player seen in a match. Used only by the leak scan and kept in memory only.
    pub fn redact_also(&self, account_id: AccountId, name: &str) {
        let mut o = self.inner.others.lock().unwrap_or_else(|e| e.into_inner());
        if account_id != 0 {
            o.ids.insert(account_id);
        }
        if !name.is_empty() {
            o.names.insert(name.to_string());
        }
    }

    /// What the file will hold, for the preview before saving.
    pub fn summary(&self) -> ExportSummary {
        export::summary(&self.inner)
    }

    /// Writes `stalemate-diagnostics-<date>.zip` into `out_dir` and returns its path. Fails
    /// closed with [`ExportError::Leak`] and writes nothing if the scan finds forbidden data.
    pub fn export(&self, out_dir: &Path, opts: &ExportOptions) -> Result<PathBuf, ExportError> {
        export::export(&self.inner, out_dir, opts)
    }
}

#[cfg(test)]
mod tests;
