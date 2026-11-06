//! Append-only JSONL storage, one file per UTC day, rolled off by `keep_days`.

use super::Inner;
use crate::MatchId;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use time::macros::format_description;
use time::{Date, Duration, OffsetDateTime};

const PREFIX: &str = "diag-";
const SUFFIX: &str = ".jsonl";

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct MatchDiag {
    pub match_id: MatchId,
    /// Log line to event (TST-010).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detect_latency_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broadcast_located: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub broadcast_connect_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub delay_s: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fragment_errors: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item_resolve_pct: Option<f32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub roster_complete_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scouting_lookups: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scouting_failures: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rate_limit_waits: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_card_ms: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_attempts: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub review_result: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub purchase_events: Option<u32>,
}

impl MatchDiag {
    /// Fields that are `Some` in `new` replace the ones here.
    fn merge(&mut self, new: &MatchDiag) {
        macro_rules! m {
            ($($f:ident),*) => { $( if new.$f.is_some() { self.$f = new.$f.clone(); } )* };
        }
        m!(
            detect_latency_ms,
            broadcast_located,
            broadcast_connect_ms,
            delay_s,
            fragment_errors,
            item_resolve_pct,
            roster_complete_ms,
            scouting_lookups,
            scouting_failures,
            rate_limit_waits,
            first_card_ms,
            review_attempts,
            review_result,
            purchase_events
        );
    }
}

/// CPU % of one core (can pass 100 on several threads) and resident memory.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PerfSample {
    pub cpu_pct: f32,
    pub rss_mb: u32,
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "k", rename_all = "snake_case")]
pub(crate) enum Body {
    Check {
        id: String,
        ok: bool,
        detail: String,
    },
    Match {
        #[serde(flatten)]
        rec: MatchDiag,
    },
    Perf {
        cpu_pct: f32,
        rss_mb: u32,
    },
    Usage {
        key: String,
    },
    Marker {
        note: String,
    },
}

#[derive(Serialize, Deserialize)]
pub(crate) struct Line {
    pub t: String,
    #[serde(flatten)]
    pub body: Body,
}

#[derive(Default)]
pub(crate) struct Writer {
    /// Day for which roll-off last ran.
    last_day: Option<Date>,
}

fn today() -> Date {
    OffsetDateTime::now_utc().date()
}

pub(crate) fn date_str(d: Date) -> String {
    d.format(format_description!("[year]-[month]-[day]"))
        .unwrap_or_default()
}

pub(crate) fn now_str() -> String {
    OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default()
}

fn file_date(name: &str) -> Option<Date> {
    let s = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
    Date::parse(s, format_description!("[year]-[month]-[day]")).ok()
}

/// Deletes day files older than `keep_days`.
pub(crate) fn roll_off(inner: &Inner) {
    let Ok(rd) = std::fs::read_dir(&inner.dir) else {
        return;
    };
    let oldest = today() - Duration::days(i64::from(inner.keep_days));
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if file_date(&name).is_some_and(|d| d < oldest) {
            if let Err(err) = std::fs::remove_file(e.path()) {
                tracing::warn!("diagnostics: cannot remove {name}: {err}");
            }
        }
    }
}

/// Appends one line. Does nothing when disabled. Errors are logged and dropped.
pub(crate) fn append(inner: &Inner, body: Body) {
    if !inner.enabled.load(std::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let line = Line { t: now_str(), body };
    let Ok(mut json) = serde_json::to_string(&line) else {
        return;
    };
    json.push('\n');
    let mut w = inner.writer.lock().unwrap_or_else(|e| e.into_inner());
    let day = today();
    if w.last_day != Some(day) {
        w.last_day = Some(day);
        roll_off(inner);
    }
    let res = std::fs::create_dir_all(&inner.dir).and_then(|_| {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(inner.dir.join(format!("{PREFIX}{}{SUFFIX}", date_str(day))))
            .and_then(|mut f| f.write_all(json.as_bytes()))
    });
    if let Err(e) = res {
        tracing::warn!("diagnostics: write failed: {e}");
    }
}

/// Everything on disk, folded: latest check per id, merged matches, all samples and markers.
#[derive(Default)]
pub(crate) struct Folded {
    pub checks: BTreeMap<String, (String, bool, String)>,
    pub matches: Vec<MatchDiag>,
    pub perf: Vec<(String, PerfSample)>,
    pub usage: BTreeMap<String, u64>,
    pub markers: Vec<(String, String)>,
    pub oldest: Option<String>,
    pub bytes: u64,
}

pub(crate) fn read_all(inner: &Inner) -> Folded {
    let mut f = Folded::default();
    let Ok(rd) = std::fs::read_dir(&inner.dir) else {
        return f;
    };
    let mut files: Vec<(Date, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let d = file_date(&e.file_name().to_string_lossy())?;
            Some((d, e.path()))
        })
        .collect();
    files.sort();
    f.oldest = files.first().map(|(d, _)| date_str(*d));
    let mut index: BTreeMap<MatchId, usize> = BTreeMap::new();
    for (_, p) in files {
        let Ok(text) = std::fs::read_to_string(&p) else {
            continue;
        };
        f.bytes += text.len() as u64;
        for l in text.lines() {
            let Ok(line) = serde_json::from_str::<Line>(l) else {
                continue; // a torn last line after a crash
            };
            match line.body {
                Body::Check { id, ok, detail } => {
                    f.checks.insert(id, (line.t, ok, detail));
                }
                Body::Match { rec } => match index.get(&rec.match_id) {
                    Some(&i) => f.matches[i].merge(&rec),
                    None => {
                        index.insert(rec.match_id, f.matches.len());
                        f.matches.push(rec);
                    }
                },
                Body::Perf { cpu_pct, rss_mb } => {
                    f.perf.push((line.t, PerfSample { cpu_pct, rss_mb }))
                }
                Body::Usage { key } => *f.usage.entry(key).or_insert(0) += 1,
                Body::Marker { note } => f.markers.push((line.t, note)),
            }
        }
    }
    f
}
