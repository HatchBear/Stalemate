//! Raw Broadcast recording (FR-DATA-005) and its retention (doc 11).
//!
//! Every fragment received is appended unchanged to `<recordings>/<match_id>.bcast`, in the order
//! received: `/<n>/start`, `/<n>/full`, `/<n>/delta`, and so on. That is the layout haste's
//! `BroadcastFile` reads. The file keeps everything the Broadcast carries, including positions and
//! chat; chat is never displayed (DEC-025).
//!
//! Recording is for the user's own match only. The caller decides that and passes a recordings
//! directory to [`super::BroadcastDeps`] only then. Scouting a match you are not in must not record.
//!
//! After a reconnect the new connection starts with a fresh `start` and `full` fragment, which are
//! appended too. The file then holds a second signup in the middle. Review must treat a repeated
//! `start` fragment as a restart.

use std::{
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

use tokio::io::AsyncWriteExt;

use crate::{settings::RetentionRules, types::MatchId};

pub const RECORDING_EXT: &str = "bcast";

/// `<dir>/<match_id>.bcast`.
pub fn recording_path(dir: &Path, match_id: MatchId) -> PathBuf {
    dir.join(format!("{match_id}.{RECORDING_EXT}"))
}

/// Appends fragments to one match's file. Opens the file on first use.
pub struct Recorder {
    path: PathBuf,
    file: Option<tokio::fs::File>,
    written: u64,
}

impl Recorder {
    pub fn new(dir: &Path, match_id: MatchId) -> Self {
        Self {
            path: recording_path(dir, match_id),
            file: None,
            written: 0,
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Bytes appended by this recorder.
    pub fn written(&self) -> u64 {
        self.written
    }

    /// Append one fragment. The data is flushed to the OS before this returns, so a crash loses at
    /// most the fragment in flight.
    pub async fn append(&mut self, fragment: &[u8]) -> std::io::Result<()> {
        if self.file.is_none() {
            if let Some(parent) = self.path.parent() {
                tokio::fs::create_dir_all(parent).await?;
            }
            self.file = Some(
                tokio::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(&self.path)
                    .await?,
            );
        }
        let f = self.file.as_mut().expect("opened above");
        f.write_all(fragment).await?;
        f.flush().await?;
        self.written += fragment.len() as u64;
        Ok(())
    }
}

/// What [`cleanup_recordings`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub kept: usize,
    pub deleted: Vec<PathBuf>,
}

/// Delete old recordings: keep a file if it is among the newest `recordings_keep_matches`, or was
/// modified within `recordings_keep_days` ("whichever is more", doc 11). Only `*.bcast` files in
/// `dir` are touched. Call it at startup. A missing directory is not an error.
pub fn cleanup_recordings(
    dir: &Path,
    rules: &RetentionRules,
    now: SystemTime,
) -> std::io::Result<CleanupReport> {
    let mut report = CleanupReport::default();
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(report),
        Err(e) => return Err(e),
    };
    let mut files: Vec<(SystemTime, PathBuf)> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        let is_rec = path.extension().is_some_and(|e| e == RECORDING_EXT);
        if !is_rec || !entry.file_type()?.is_file() {
            continue;
        }
        files.push((entry.metadata()?.modified()?, path));
    }
    files.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1))); // newest first
    let max_age = Duration::from_secs(u64::from(rules.recordings_keep_days) * 86_400);
    for (i, (modified, path)) in files.into_iter().enumerate() {
        let recent_enough = now
            .duration_since(modified)
            .map(|age| age <= max_age)
            .unwrap_or(true); // modified in the future: keep
        if i < rules.recordings_keep_matches as usize || recent_enough {
            report.kept += 1;
        } else {
            std::fs::remove_file(&path)?;
            report.deleted.push(path);
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{File, FileTimes};

    fn make(dir: &Path, name: &str, age_days: u64, now: SystemTime) -> PathBuf {
        let p = dir.join(name);
        let f = File::create(&p).unwrap();
        let t = now - Duration::from_secs(age_days * 86_400);
        f.set_times(FileTimes::new().set_modified(t)).unwrap();
        p
    }

    fn rules(n: u32, days: u32) -> RetentionRules {
        RetentionRules {
            recordings_keep_matches: n,
            recordings_keep_days: days,
        }
    }

    #[tokio::test]
    async fn appends_in_order_and_length_is_the_sum() {
        let dir = tempfile::tempdir().unwrap();
        let mut r = Recorder::new(dir.path(), 77);
        for part in [&b"abc"[..], &[0u8; 1000][..], b"z"] {
            r.append(part).await.unwrap();
        }
        let got = std::fs::read(recording_path(dir.path(), 77)).unwrap();
        assert_eq!(got.len(), 1004);
        assert_eq!(r.written(), 1004);
        assert!(got.starts_with(b"abc") && got.ends_with(b"z"));
        // A second recorder for the same match appends (a reconnect, or an app restart).
        Recorder::new(dir.path(), 77).append(b"!!").await.unwrap();
        assert_eq!(
            std::fs::metadata(recording_path(dir.path(), 77))
                .unwrap()
                .len(),
            1006
        );
    }

    #[test]
    fn keeps_the_newest_n_or_anything_recent_whichever_is_more() {
        let dir = tempfile::tempdir().unwrap();
        let now = SystemTime::now();
        // Ages in days: 1, 2, 40, 50, 60.
        for (i, age) in [1u64, 2, 40, 50, 60].iter().enumerate() {
            make(dir.path(), &format!("{i}.bcast"), *age, now);
        }
        make(dir.path(), "notes.txt", 400, now);
        // Keep the newest 3 (ages 1, 2, 40) plus anything within 30 days: 3 files kept.
        let rep = cleanup_recordings(dir.path(), &rules(3, 30), now).unwrap();
        assert_eq!(rep.kept, 3);
        assert_eq!(rep.deleted.len(), 2);
        assert!(dir.path().join("2.bcast").exists());
        assert!(!dir.path().join("3.bcast").exists());
        assert!(dir.path().join("notes.txt").exists(), "other files stay");
        // Keep only 1 by count, but all within 30 days: the two recent files stay.
        let rep = cleanup_recordings(dir.path(), &rules(1, 30), now).unwrap();
        assert_eq!(rep.kept, 2);
        assert!(!dir.path().join("2.bcast").exists());
    }

    #[test]
    fn missing_directory_is_fine() {
        let dir = tempfile::tempdir().unwrap();
        let rep =
            cleanup_recordings(&dir.path().join("nope"), &rules(1, 1), SystemTime::now()).unwrap();
        assert_eq!(rep, CleanupReport::default());
    }
}
