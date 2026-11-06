//! Downloads a live relay one fragment at a time and saves the raw bodies (PRG-27, DATA-008).
//!
//! The order is the one haste uses (`BroadcastHttp`), and what the game's own client does:
//!
//! ```text
//! GET /sync                 -> saved as relay/sync (the first reply only)
//! GET /<signup>/start       -> relay/<signup>_start
//! GET /<f>/full             -> relay/<f>_full        (f = sync.fragment)
//! GET /<f>/delta            -> relay/<f>_delta
//! GET /<f+1>/delta, ...     -> relay/<n>_delta
//! ```
//!
//! Pace and politeness:
//! - One request at a time. No parallel requests.
//! - A delta is requested only when `/sync` says its fragment exists. When it does not (or the
//!   relay answers 404 or 405 for it: "not ready yet"), the downloader waits one poll interval
//!   (the relay's `keyframe_interval`, clamped to 1 to 10 s, unless the options set one) and asks
//!   `/sync` again. Nothing is retried faster than that.
//! - Any other failure (5xx, network) waits the same interval.
//! - It stops when nothing new was saved for `idle_timeout`. That is how the end of the match shows:
//!   the relay stops publishing.
//!
//! Bodies are saved exactly as `reqwest` hands them over. The relay may gzip them; reqwest decodes
//! that before we see the bytes, the same as the app's own recorder. The files are then what the
//! parser reads.

use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use serde::Deserialize;

#[derive(Debug, Clone)]
pub struct DownloadOpts {
    /// Stop after this long without saving a new file.
    pub idle_timeout: Duration,
    /// Wait between attempts. `None` uses the relay's `keyframe_interval` (1 to 10 s).
    pub poll: Option<Duration>,
    pub request_timeout: Duration,
}

impl Default for DownloadOpts {
    fn default() -> Self {
        Self {
            idle_timeout: Duration::from_secs(90),
            poll: None,
            request_timeout: Duration::from_secs(10),
        }
    }
}

/// What one [`Downloader::step`] saved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Saved {
    Sync,
    Start(u64),
    Full(u64),
    Delta(u64),
}

/// The parts of the relay's `/sync` reply we need.
#[derive(Debug, Clone, Deserialize)]
pub struct SyncInfo {
    pub fragment: u64,
    pub signup_fragment: u64,
    #[serde(default)]
    pub keyframe_interval: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Phase {
    Sync,
    Start,
    Full,
    Deltas,
}

enum Fetched {
    Body(Vec<u8>),
    /// 404 or 405: the fragment is not published yet.
    NotReady,
}

pub struct Downloader {
    client: reqwest::Client,
    base: String,
    dir: PathBuf,
    opts: DownloadOpts,
    phase: Phase,
    first: Option<SyncInfo>,
    live: u64,
    next_delta: u64,
    stale_live: bool,
    last_saved: Instant,
    files: usize,
    bytes: u64,
}

impl Downloader {
    /// `base` is the relay URL (`.../tv/<match>`); files go to `dir` (created).
    pub fn new(base: &str, dir: &Path, opts: DownloadOpts) -> Result<Self> {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        Ok(Self {
            client: reqwest::Client::builder()
                .user_agent(stalemate_core::api_client::user_agent())
                .timeout(opts.request_timeout)
                .build()?,
            base: base.trim_end_matches('/').to_string(),
            dir: dir.to_path_buf(),
            opts,
            phase: Phase::Sync,
            first: None,
            live: 0,
            next_delta: 0,
            stale_live: false,
            last_saved: Instant::now(),
            files: 0,
            bytes: 0,
        })
    }

    /// Files saved so far.
    pub fn files(&self) -> usize {
        self.files
    }

    /// Bytes saved so far.
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    fn poll(&self) -> Duration {
        self.opts.poll.unwrap_or_else(|| {
            Duration::from_secs(
                self.first
                    .as_ref()
                    .map_or(3, |s| s.keyframe_interval)
                    .clamp(1, 10),
            )
        })
    }

    /// Wait one poll interval. `false` once the idle timeout is used up.
    async fn pause(&self) -> bool {
        if self.last_saved.elapsed() >= self.opts.idle_timeout {
            return false;
        }
        tokio::time::sleep(self.poll()).await;
        self.last_saved.elapsed() < self.opts.idle_timeout
    }

    async fn get(&self, path: &str) -> Result<Fetched> {
        let url = format!("{}/{path}", self.base);
        let resp = self.client.get(&url).send().await?;
        let status = resp.status();
        if status.as_u16() == 404 || status.as_u16() == 405 {
            return Ok(Fetched::NotReady);
        }
        if !status.is_success() {
            bail!("{path}: http {}", status.as_u16());
        }
        Ok(Fetched::Body(resp.bytes().await?.to_vec()))
    }

    async fn save(&mut self, name: &str, body: &[u8]) -> Result<()> {
        tokio::fs::write(self.dir.join(name), body)
            .await
            .with_context(|| format!("writing {name}"))?;
        self.files += 1;
        self.bytes += body.len() as u64;
        self.last_saved = Instant::now();
        Ok(())
    }

    async fn read_sync(&self) -> Result<(SyncInfo, Vec<u8>)> {
        match self.get("sync").await? {
            Fetched::Body(b) => {
                let info = serde_json::from_slice(&b).context("the relay's /sync is not JSON")?;
                Ok((info, b))
            }
            Fetched::NotReady => bail!("sync: not found"),
        }
    }

    /// Save the next file. `Ok(None)` means the idle timeout passed and the recording is over.
    /// `Err` is a problem that waiting will not fix (a file could not be written).
    pub async fn step(&mut self) -> Result<Option<Saved>> {
        loop {
            match self.phase {
                Phase::Sync => match self.read_sync().await {
                    Ok((info, raw)) => {
                        self.live = info.fragment;
                        self.first = Some(info);
                        self.save("sync", &raw).await?;
                        self.phase = Phase::Start;
                        return Ok(Some(Saved::Sync));
                    }
                    Err(e) => {
                        tracing_note(&format!("waiting for the relay: {e}"));
                        if !self.pause().await {
                            return Ok(None);
                        }
                    }
                },
                Phase::Start => {
                    let n = self
                        .first
                        .as_ref()
                        .ok_or_else(|| anyhow!("no sync"))?
                        .signup_fragment;
                    match self.get(&format!("{n}/start")).await {
                        Ok(Fetched::Body(b)) => {
                            self.save(&format!("{n}_start"), &b).await?;
                            self.phase = Phase::Full;
                            return Ok(Some(Saved::Start(n)));
                        }
                        _ => {
                            if !self.pause().await {
                                return Ok(None);
                            }
                        }
                    }
                }
                Phase::Full => {
                    let n = self
                        .first
                        .as_ref()
                        .ok_or_else(|| anyhow!("no sync"))?
                        .fragment;
                    match self.get(&format!("{n}/full")).await {
                        Ok(Fetched::Body(b)) => {
                            self.save(&format!("{n}_full"), &b).await?;
                            self.next_delta = n;
                            self.phase = Phase::Deltas;
                            return Ok(Some(Saved::Full(n)));
                        }
                        _ => {
                            if !self.pause().await {
                                return Ok(None);
                            }
                        }
                    }
                }
                Phase::Deltas => {
                    if self.stale_live || self.next_delta > self.live {
                        // Ask where the live edge is before asking for a fragment that may not exist.
                        if let Ok((info, _)) = self.read_sync().await {
                            self.live = info.fragment;
                        }
                        self.stale_live = false;
                        if self.next_delta > self.live && !self.pause().await {
                            return Ok(None);
                        }
                        if self.next_delta > self.live {
                            continue;
                        }
                    }
                    let n = self.next_delta;
                    match self.get(&format!("{n}/delta")).await {
                        Ok(Fetched::Body(b)) => {
                            self.save(&format!("{n}_delta"), &b).await?;
                            self.next_delta = n + 1;
                            return Ok(Some(Saved::Delta(n)));
                        }
                        _ => {
                            self.stale_live = true;
                            if !self.pause().await {
                                return Ok(None);
                            }
                        }
                    }
                }
            }
        }
    }
}

fn tracing_note(msg: &str) {
    eprintln!("{msg}");
}
