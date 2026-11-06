//! M-04 Broadcast client (PRG-04). See 04-data-sources-and-ingestion.md.
//!
//! One match, one task. [`spawn`] locates the Broadcast ([`Locator`], DEC-049), connects through a
//! [`Connector`], and turns what it reads into [`BroadcastEvent`]s on a channel:
//!
//! - `Connected { url }` after each successful connection.
//! - `Parser(ParserEvent)` for roster, items, KDA, level, net worth, mode, game time and the end of
//!   the match. Change-only: a reconnect never repeats an event ([`dedup::Dedup`], TC-04-2).
//! - `Delay { secs }`: how far the data is behind real time (FR-UI-008).
//! - `Lost` once the `rules.broadcast.backoff_s` schedule is used up (NFR-REL-001, TC-04-3).
//! - `Ended` after the `End` parser event.
//!
//! Every event carries the wall time it was received (`received_at`), next to the game time that
//! the parser events carry.
//!
//! The fragment source and the parser sit behind [`Connector`] and [`Session`], so reconnect,
//! de-duplication and recording are tested against a mock with synthetic bytes. The real one,
//! [`haste_source::HasteConnector`], is compiled with the `broadcast` feature (default). Without it
//! the module keeps the same API and every connection attempt reports
//! [`SessionError::Unsupported`], which ends the client with `Lost` at once.
//!
//! Fair play (doc 03): reads the public Broadcast only. No game coordinator client, no game
//! memory. Alpha extracts only Live-Safe-relevant fields; the raw recording keeps the rest for the
//! Review (FP-5).

pub mod dedup;
#[cfg(feature = "broadcast")]
pub mod haste_source;
pub mod locator;
pub mod normalize;
pub mod recorder;

#[cfg(not(feature = "broadcast"))]
pub mod haste_source {
    //! Stub used when the crate is built without the `broadcast` feature.
    use super::{async_trait, Connector, Session, SessionError};
    use crate::types::{AccountId, MatchId};

    /// Same name and constructor as the real connector. Every connect fails with `Unsupported`.
    #[derive(Debug, Default)]
    pub struct HasteConnector;

    impl HasteConnector {
        pub fn new(
            _self_account: Option<AccountId>,
            _request_timeout: std::time::Duration,
        ) -> Self {
            Self
        }
    }

    #[async_trait]
    impl Connector for HasteConnector {
        async fn connect(
            &self,
            _url: &str,
            _match_id: MatchId,
        ) -> Result<Box<dyn Session>, SessionError> {
            Err(SessionError::Unsupported)
        }
    }
}

use std::{path::PathBuf, sync::Arc, time::Duration};

pub use async_trait::async_trait;
use bytes::Bytes;
use time::OffsetDateTime;
use tokio::{sync::mpsc, task::JoinHandle};

pub use dedup::Dedup;
pub use haste_source::HasteConnector;
pub use locator::{LocateError, Locator, UrlLocator};
pub use recorder::{cleanup_recordings, recording_path, CleanupReport, Recorder};

use crate::{
    api_client::ApiClient,
    settings::BroadcastRules,
    types::{AccountId, MatchId, ParserEvent},
};

/// What the client sends. `received_at` is the wall time (UTC) the client handled the event.
#[derive(Debug, Clone, PartialEq)]
pub struct BroadcastEvent {
    pub received_at: OffsetDateTime,
    pub kind: BroadcastKind,
}

#[derive(Debug, Clone, PartialEq)]
pub enum BroadcastKind {
    /// Connected (or reconnected) to this URL.
    Connected {
        url: String,
    },
    Parser(ParserEvent),
    /// Seconds the newest data is behind real time: the relay's `rtdelay` plus the age of the
    /// fragment (how many fragments behind the live edge it was when it arrived).
    Delay {
        secs: f32,
    },
    /// Every retry in `backoff_s` failed. The client has stopped. FR-UI-008 shows this.
    Lost,
    /// The match ended (`ParserEvent::End` was sent). The client has stopped.
    Ended,
}

/// Why the client stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BroadcastOutcome {
    Ended,
    Lost,
    /// The receiver was dropped.
    Stopped,
}

/// One read from a session: raw fragments received, plus what they parsed to.
#[derive(Debug, Default, Clone)]
pub struct Step {
    /// Fragments exactly as received, in order. The client records these.
    pub fragments: Vec<Bytes>,
    /// Complete current state as events. The client de-duplicates.
    pub events: Vec<ParserEvent>,
    /// Delay of the newest data, seconds, when known.
    pub delay_s: Option<f32>,
}

#[derive(Debug, thiserror::Error)]
pub enum SessionError {
    #[error("connection failed: {0}")]
    Connect(String),
    #[error("stream failed: {0}")]
    Stream(String),
    /// The Broadcast is for a different match than requested (`m_unMatchID`).
    #[error("the Broadcast belongs to another match")]
    WrongMatch,
    /// Built without the `broadcast` feature. Not retried.
    #[error("Broadcast support is not compiled in")]
    Unsupported,
}

/// A connected stream. Dropping it closes the connection.
#[async_trait]
pub trait Session: Send {
    /// The next fragment(s) and events. `Ok(None)` means the relay closed the stream.
    async fn next_step(&mut self) -> Result<Option<Step>, SessionError>;
}

/// Opens sessions. [`HasteConnector`] is the real one.
#[async_trait]
pub trait Connector: Send + Sync {
    async fn connect(&self, url: &str, match_id: MatchId)
        -> Result<Box<dyn Session>, SessionError>;
}

/// Everything [`spawn`] needs besides the match ID and the channel.
#[derive(Clone)]
pub struct BroadcastDeps {
    pub locator: Arc<dyn UrlLocator>,
    pub connector: Arc<dyn Connector>,
    pub rules: BroadcastRules,
    /// Where to record raw fragments (`<dir>/<match_id>.bcast`), or `None` for no recording.
    ///
    /// FR-DATA-005 records the user's own match only. The caller passes `Some` only for it, and
    /// `None` when scouting another match.
    pub recordings_dir: Option<PathBuf>,
}

impl BroadcastDeps {
    /// The production wiring: deadlock-api locator plus the haste connector.
    ///
    /// `self_account` is the user's Account ID, used for their `SelfState`. `recordings_dir` is
    /// `Some(Paths::recordings_dir())` for the user's own match, `None` otherwise.
    pub fn live(
        api: Arc<ApiClient>,
        rules: &BroadcastRules,
        self_account: Option<AccountId>,
        recordings_dir: Option<PathBuf>,
    ) -> Self {
        Self {
            locator: Arc::new(Locator::new(api)),
            connector: Arc::new(HasteConnector::new(
                self_account,
                Duration::from_secs(rules.request_timeout_s),
            )),
            rules: rules.clone(),
            recordings_dir,
        }
    }
}

/// Start the client for one match. Stops on `Lost`, `Ended`, or when `tx`'s receiver is dropped.
pub fn spawn(
    match_id: MatchId,
    deps: BroadcastDeps,
    tx: mpsc::Sender<BroadcastEvent>,
) -> JoinHandle<BroadcastOutcome> {
    tokio::spawn(run(match_id, deps, tx))
}

async fn emit(tx: &mpsc::Sender<BroadcastEvent>, kind: BroadcastKind) -> bool {
    tx.send(BroadcastEvent {
        received_at: OffsetDateTime::now_utc(),
        kind,
    })
    .await
    .is_ok()
}

/// The client loop (PRG-04). [`spawn`] runs it in a task; tests call it directly.
///
/// ```text
/// failures = 0
/// loop:
///     locate -> connect -> read steps (record, de-duplicate, send)
///     any step resets failures
///     on a failure: if failures == backoff.len() -> Lost; sleep backoff[failures]; failures += 1
/// ```
pub async fn run(
    match_id: MatchId,
    deps: BroadcastDeps,
    tx: mpsc::Sender<BroadcastEvent>,
) -> BroadcastOutcome {
    let backoff = deps.rules.backoff_s.clone();
    let mut dedup = Dedup::default();
    let mut recorder = deps
        .recordings_dir
        .as_deref()
        .map(|d| Recorder::new(d, match_id));
    let mut last_delay: Option<f32> = None;
    let mut failures = 0usize;

    loop {
        let mut wrong_match = false;
        let mut hard_stop = false;
        match deps.locator.locate(match_id).await {
            Err(e) => tracing::debug!("broadcast locate failed: {e}"),
            Ok(url) => match deps.connector.connect(&url, match_id).await {
                Err(e) => {
                    tracing::debug!("broadcast connect failed: {e}");
                    hard_stop = matches!(e, SessionError::Unsupported);
                    wrong_match = matches!(e, SessionError::WrongMatch);
                }
                Ok(mut session) => {
                    if !emit(&tx, BroadcastKind::Connected { url: url.clone() }).await {
                        return BroadcastOutcome::Stopped;
                    }
                    loop {
                        let step = match session.next_step().await {
                            Ok(Some(s)) => s,
                            Ok(None) => break,
                            Err(e) => {
                                tracing::debug!("broadcast stream failed: {e}");
                                wrong_match = matches!(e, SessionError::WrongMatch);
                                break;
                            }
                        };
                        failures = 0;
                        if let Some(rec) = recorder.as_mut() {
                            for f in &step.fragments {
                                if let Err(e) = rec.append(f).await {
                                    // A full disk must not kill the live feed.
                                    tracing::warn!("could not record a fragment: {e}");
                                    recorder = None;
                                    break;
                                }
                            }
                        }
                        if let Some(secs) = step.delay_s {
                            if last_delay != Some(secs)
                                && !emit(&tx, BroadcastKind::Delay { secs }).await
                            {
                                return BroadcastOutcome::Stopped;
                            }
                            last_delay = Some(secs);
                        }
                        for ev in &step.events {
                            if dedup.admit(ev)
                                && !emit(&tx, BroadcastKind::Parser(ev.clone())).await
                            {
                                return BroadcastOutcome::Stopped;
                            }
                        }
                        if dedup.has_ended() {
                            let _ = emit(&tx, BroadcastKind::Ended).await;
                            return BroadcastOutcome::Ended;
                        }
                    }
                }
            },
        }

        deps.locator.report_failure(match_id, wrong_match);
        if hard_stop || failures >= backoff.len() {
            let _ = emit(&tx, BroadcastKind::Lost).await;
            return BroadcastOutcome::Lost;
        }
        tokio::select! {
            () = tokio::time::sleep(Duration::from_secs(backoff[failures])) => {}
            () = tx.closed() => return BroadcastOutcome::Stopped,
        }
        failures += 1;
    }
}

#[cfg(test)]
mod tests;
