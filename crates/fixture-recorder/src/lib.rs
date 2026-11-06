//! Fixture recorder (PRG-27, DATA-008, T-013). A developer tool; the app never links it.
//!
//! Three jobs, one per subcommand of the binary:
//!
//! - `record`: save the raw relay files of a live match to a local, git-ignored directory
//!   ([`download`]).
//! - `normalize`: serve those files from a local server and run them through core's Broadcast
//!   client ([`serve`], [`collect_events`]), then write `events.jsonl` and `meta.json` with other
//!   players pseudonymized and chat removed ([`pseudo`], DEC-056, DEC-060).
//! - `api`: save the deadlock-api responses for the same match through core's rate-limited client,
//!   pseudonymized with the same mapping.
//!
//! Raw relay files hold chat and positions. They never go in the repo.

pub mod download;
pub mod pseudo;
pub mod serve;

use std::{path::Path, sync::Arc};

use anyhow::{bail, Context, Result};
use serde_json::Value;
use stalemate_core::{
    broadcast::{
        self, async_trait, BroadcastDeps, BroadcastKind, Connector, LocateError, UrlLocator,
    },
    settings::BroadcastRules,
    types::{MatchId, ParserEvent},
};

use pseudo::{is_chat, Pseudonyms, Seen};

/// A locator that always answers with one URL (the local replay server).
pub struct FixedLocator(pub String);

#[async_trait]
impl UrlLocator for FixedLocator {
    async fn locate(&self, _match_id: MatchId) -> Result<String, LocateError> {
        Ok(self.0.clone())
    }

    fn report_failure(&self, _match_id: MatchId, _wrong_match: bool) {}
}

/// Run core's Broadcast client against `url` through `connector` and collect every parser event
/// until the stream ends. There are no retries: the end of a replay is the end of the files.
pub async fn collect_events(
    connector: Arc<dyn Connector>,
    url: &str,
    match_id: MatchId,
    rules: &BroadcastRules,
) -> Vec<ParserEvent> {
    let mut rules = rules.clone();
    rules.backoff_s = Vec::new();
    let deps = BroadcastDeps {
        locator: Arc::new(FixedLocator(url.to_string())),
        connector,
        rules,
        recordings_dir: None,
    };
    let (tx, mut rx) = tokio::sync::mpsc::channel(1024);
    let task = tokio::spawn(broadcast::run(match_id, deps, tx));
    let mut out = Vec::new();
    while let Some(ev) = rx.recv().await {
        if let BroadcastKind::Parser(p) = ev.kind {
            out.push(p);
        }
    }
    let _ = task.await;
    out
}

/// Parse the files in `<raw>/relay/`-style directory `relay_dir` with the real haste connector.
pub async fn replay_relay(
    relay_dir: &Path,
    match_id: MatchId,
    me: u32,
    rules: &BroadcastRules,
) -> Result<Vec<ParserEvent>> {
    let server = serve::ReplayServer::start(relay_dir)
        .await
        .context("starting the local replay server")?;
    let connector = Arc::new(broadcast::HasteConnector::new(
        Some(me),
        std::time::Duration::from_secs(rules.request_timeout_s),
    ));
    let events = collect_events(connector, &server.base_url(match_id), match_id, rules).await;
    if events.is_empty() {
        bail!(
            "the replay produced no events; check that {} holds a full recording \
             (sync, a start file, a full file and deltas)",
            relay_dir.display()
        );
    }
    Ok(events)
}

/// Build the pseudonym mapping from the rosters in `events`, then add any other Account ID found
/// in the events. `normalize` and `api` both call this on the same replay, so they agree.
pub fn mapping_from_events(me: u32, events: &[ParserEvent]) -> Result<Pseudonyms> {
    let mut seen = Vec::new();
    for e in events {
        if let ParserEvent::Roster { players } = e {
            seen.extend(players.iter().map(|p| Seen {
                team: p.team,
                account_id: p.account_id,
                name: p.name.clone(),
            }));
        }
    }
    let mut p = Pseudonyms::from_roster(me, &seen);
    let mut found = std::collections::BTreeSet::new();
    for e in events.iter().filter(|e| !is_chat_event(e)) {
        Pseudonyms::collect_ids(&serde_json::to_value(e)?, &mut found);
    }
    p.add_unknown(&found);
    Ok(p)
}

fn is_chat_event(e: &ParserEvent) -> bool {
    matches!(e, ParserEvent::Chat { .. })
}

/// Chat out, pseudonyms in, one JSON object per line. Fails closed: if the finished text still
/// holds an original ID or name, nothing is returned.
pub fn pseudonymize_events(p: &Pseudonyms, events: &[ParserEvent]) -> Result<String> {
    let mut out = String::new();
    for e in events {
        let v = serde_json::to_value(e)?;
        if is_chat(&v) {
            continue;
        }
        out.push_str(&serde_json::to_string(&p.apply(&v)?)?);
        out.push('\n');
    }
    let leaks = p.find_leaks(&out);
    if !leaks.is_empty() {
        bail!("refusing to write: the output still identifies a player ({leaks:?})");
    }
    Ok(out)
}

/// `meta.json` for an FX-BC fixture (the layout tests/compliance reads).
pub fn meta_json(match_id: MatchId, me: u32, started_unix: u64) -> Value {
    serde_json::json!({ "match_id": match_id, "me": me, "started_unix": started_unix })
}

/// Pseudonymize one API response body. Profile images and links are dropped: they identify the
/// player. Fails closed like [`pseudonymize_events`].
pub fn pseudonymize_api_body(p: &mut Pseudonyms, body: &[u8]) -> Result<Vec<u8>> {
    let mut v: Value = serde_json::from_slice(body).context("the response is not JSON")?;
    drop_identifying_keys(&mut v);
    let mut found = std::collections::BTreeSet::new();
    Pseudonyms::collect_ids(&v, &mut found);
    p.add_unknown(&found);
    let out = serde_json::to_string_pretty(&p.apply(&v)?)?;
    let leaks = p.find_leaks(&out);
    if !leaks.is_empty() {
        bail!("refusing to write: the response still identifies a player ({leaks:?})");
    }
    Ok(out.into_bytes())
}

fn drop_identifying_keys(v: &mut Value) {
    match v {
        Value::Object(m) => {
            m.retain(|k, _| {
                !matches!(
                    k.as_str(),
                    "avatar"
                        | "avatarmedium"
                        | "avatarfull"
                        | "profileurl"
                        | "steamid"
                        | "steam_id"
                        | "realname"
                        | "countrycode"
                        | "chat"
                )
            });
            m.values_mut().for_each(drop_identifying_keys);
        }
        Value::Array(a) => a.iter_mut().for_each(drop_identifying_keys),
        _ => {}
    }
}

#[cfg(test)]
mod tests;
