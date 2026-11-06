//! The real Broadcast source: haste over HTTP (DEC-049). Compiled with the `broadcast` feature.
//!
//! haste's parser is not `Send` (it holds `Rc`s), so each session runs on its own thread with a
//! small current-thread runtime. The thread owns the parser and sends finished [`Step`]s over a
//! channel, which keeps [`HasteSession`] `Send` and the rest of the client ordinary tokio code.
//! Dropping the session closes the channel and the thread exits.
//!
//! What happens per fragment:
//! 1. `BroadcastHttp::next_packet` returns the raw bytes of the next fragment (`start`, then
//!    `full`, then `delta`s). Those bytes are passed on for recording, unchanged.
//! 2. `Parser::run_to_end` applies them to the entity state.
//! 3. The state is read once into a [`GameSnapshot`], which becomes [`ParserEvent`]s. Reading once
//!    per fragment (about every 3 s) means there is no per-tick event stream to thin out.
//! 4. `GET <url>/sync` is polled for `rtdelay`, the live fragment number and `tps`.
//!
//! The HTTP client is ours (reqwest 0.13, wrapped as haste's `HttpClient`), not haste's.

use std::{future::Future, time::Duration};

use async_trait::async_trait;
use bytes::Bytes;
use haste::{
    broadcast::{BroadcastHttp, HttpClient},
    entities::{fkey_from_path, Entity},
    fxhash,
    parser::{NopVisitor, Parser},
};
use serde::Deserialize;
use tokio::sync::mpsc;

use super::{
    normalize::{
        events_from_snapshot, game_time_secs, normalize_team, GameSnapshot, PlayerSnapshot,
    },
    Connector, Session, SessionError, Step,
};
use crate::{
    api_client::user_agent,
    types::{AccountId, ItemId, MatchId},
};

// Serializer names, hashed the way haste hashes them.
const PLAYER_CLASS: u64 = fxhash::hash_bytes(b"CCitadelPlayerController");
const RULES_CLASS: u64 = fxhash::hash_bytes(b"CCitadelGameRulesProxy");

// CCitadelPlayerController. Fields in `m_PlayerDataGlobal` carry the send-node prefix in their key.
const K_ACCOUNT: u64 = fkey_from_path(&["m_unEconAccountID"]);
const K_NAME: u64 = fkey_from_path(&["m_iszPlayerName"]);
const K_TEAM: u64 = fkey_from_path(&["m_iTeamNum"]);
const K_HERO: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_nHeroID"]);
const K_UPGRADES: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_vecUpgrades"]);
const K_KILLS: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iPlayerKills"]);
const K_DEATHS: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iDeaths"]);
const K_ASSISTS: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iPlayerAssists"]);
const K_LEVEL: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iLevel"]);
const K_NET_WORTH: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iGoldNetWorth"]);
const K_GOLD: u64 = fkey_from_path(&["m_PlayerDataGlobal", "m_iGold"]);

// CCitadelGameRulesProxy.
const K_MATCH_ID: u64 = fkey_from_path(&["m_pGameRules", "m_unMatchID"]);
const K_START_TIME: u64 = fkey_from_path(&["m_pGameRules", "m_flGameStartTime"]);
const K_PAUSED_TICKS: u64 = fkey_from_path(&["m_pGameRules", "m_nTotalPausedTicks"]);
const K_GAME_MODE: u64 = fkey_from_path(&["m_pGameRules", "m_eGameMode"]);
const K_MATCH_MODE: u64 = fkey_from_path(&["m_pGameRules", "m_eMatchMode"]);
const K_WINNING_TEAM: u64 = fkey_from_path(&["m_pGameRules", "m_iWinningTeam"]);
const K_EXPECTED: u64 = fkey_from_path(&["m_pGameRules", "m_unExpectedPlayerCount"]);

/// Key of element `i` of a dynamic array whose own key is `base`. Matches `Entity::parse` in haste.
fn element_key(base: u64, i: u64) -> u64 {
    fxhash::add_u64_to_hash(base, fxhash::add_u64_to_hash(0, i))
}

/// haste's `HttpClient` over reqwest 0.13.
#[derive(Clone)]
struct RelayClient(reqwest::Client);

impl HttpClient for RelayClient {
    type Error = reqwest::Error;

    fn execute(
        &self,
        request: http::Request<Bytes>,
    ) -> impl Future<Output = Result<http::Response<Result<Bytes, Self::Error>>, Self::Error>> {
        let (parts, body) = request.into_parts();
        let mut req = self.0.request(parts.method, parts.uri.to_string());
        if !body.is_empty() {
            req = req.body(body);
        }
        async move {
            let resp = req.send().await?;
            let status = resp.status();
            let body = resp.bytes().await;
            let mut out = http::Response::new(body);
            *out.status_mut() = status;
            Ok(out)
        }
    }
}

/// The parts of the relay's `/sync` reply we read.
#[derive(Debug, Clone, Deserialize)]
struct Sync {
    rtdelay: f32,
    fragment: i32,
    tps: i32,
    #[serde(default)]
    keyframe_interval: i32,
}

async fn get_sync(client: &reqwest::Client, base: &str) -> Option<Sync> {
    client
        .get(format!("{base}/sync"))
        .send()
        .await
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .await
        .ok()
}

/// Opens haste sessions. `self_account` is the user's Account ID (for `SelfState`).
#[derive(Debug, Clone)]
pub struct HasteConnector {
    self_account: Option<AccountId>,
    request_timeout: Duration,
}

impl HasteConnector {
    pub fn new(self_account: Option<AccountId>, request_timeout: Duration) -> Self {
        Self {
            self_account,
            request_timeout,
        }
    }
}

enum Msg {
    Ready,
    Step(Box<Step>),
    Closed,
    Failed(SessionError),
}

struct HasteSession {
    rx: mpsc::Receiver<Msg>,
}

#[async_trait]
impl Session for HasteSession {
    async fn next_step(&mut self) -> Result<Option<Step>, SessionError> {
        loop {
            match self.rx.recv().await {
                Some(Msg::Step(s)) => return Ok(Some(*s)),
                Some(Msg::Ready) => {}
                Some(Msg::Closed) | None => return Ok(None),
                Some(Msg::Failed(e)) => return Err(e),
            }
        }
    }
}

#[async_trait]
impl Connector for HasteConnector {
    async fn connect(
        &self,
        url: &str,
        match_id: MatchId,
    ) -> Result<Box<dyn Session>, SessionError> {
        let (tx, mut rx) = mpsc::channel::<Msg>(4);
        let me = self.clone();
        let url = url.trim_end_matches('/').to_string();
        std::thread::Builder::new()
            .name("stalemate-broadcast".into())
            .spawn(move || worker(me, url, match_id, tx))
            .map_err(|e| SessionError::Connect(e.to_string()))?;
        match rx.recv().await {
            Some(Msg::Ready) => Ok(Box::new(HasteSession { rx })),
            Some(Msg::Failed(e)) => Err(e),
            _ => Err(SessionError::Connect("the worker stopped early".into())),
        }
    }
}

fn worker(cfg: HasteConnector, url: String, match_id: MatchId, tx: mpsc::Sender<Msg>) {
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            let _ = tx.blocking_send(Msg::Failed(SessionError::Connect(e.to_string())));
            return;
        }
    };
    rt.block_on(worker_async(cfg, url, match_id, tx));
}

async fn worker_async(cfg: HasteConnector, url: String, match_id: MatchId, tx: mpsc::Sender<Msg>) {
    let fail = |e: SessionError| async {
        let _ = tx.send(Msg::Failed(e)).await;
    };
    let client = match reqwest::Client::builder()
        .user_agent(user_agent())
        .timeout(cfg.request_timeout)
        .build()
    {
        Ok(c) => c,
        Err(e) => return fail(SessionError::Connect(e.to_string())).await,
    };
    let stream =
        match BroadcastHttp::start_streaming(RelayClient(client.clone()), url.clone()).await {
            Ok(s) => s,
            Err(e) => return fail(SessionError::Connect(e.to_string())).await,
        };
    let first = {
        let s = stream.sync_response();
        Sync {
            rtdelay: s.rtdelay,
            fragment: s.fragment,
            tps: s.tps,
            keyframe_interval: s.keyframe_interval,
        }
    };
    let mut parser = match Parser::from_stream(stream) {
        Ok(p) => p,
        Err(e) => return fail(SessionError::Connect(format!("{e:?}"))).await,
    };
    if tx.send(Msg::Ready).await.is_err() {
        return;
    }

    // Packet 1 is `start`, 2 is `full` of fragment F, 3 is `delta` of F, then F+1, F+2, ...
    let mut latest = first.clone();
    let mut packets: i32 = 0;
    loop {
        let next = tokio::select! {
            () = tx.closed() => return,
            n = parser.demo_stream_mut().next_packet() => n,
        };
        let bytes = match next {
            Some(Ok(b)) => b,
            Some(Err(e)) => return fail(SessionError::Stream(e.to_string())).await,
            None => {
                let _ = tx.send(Msg::Closed).await;
                return;
            }
        };
        packets += 1;
        if let Err(e) = parser.run_to_end() {
            return fail(SessionError::Stream(format!("parse: {e:?}"))).await;
        }
        if packets >= 2 {
            if let Some(s) = get_sync(&client, &url).await {
                latest = s;
            }
        }
        let tps = if latest.tps > 0 {
            latest.tps
        } else {
            first.tps
        };
        let snap = snapshot(&parser, tps);
        if snap.match_id.is_some_and(|id| id != match_id) {
            return fail(SessionError::WrongMatch).await;
        }
        let events = events_from_snapshot(&snap, cfg.self_account);

        // Delay: the relay's rtdelay, plus the age of this fragment, which is how many fragments
        // behind the live edge it is, times the fragment length.
        let current = first.fragment + (packets - 3).max(0);
        let behind = (latest.fragment - current).max(0);
        let kfi = if latest.keyframe_interval > 0 {
            latest.keyframe_interval
        } else {
            first.keyframe_interval
        };
        let delay_s = latest.rtdelay + (behind * kfi) as f32;

        let step = Step {
            fragments: vec![bytes],
            events,
            delay_s: Some(delay_s),
        };
        if tx.send(Msg::Step(Box::new(step))).await.is_err() {
            return;
        }
    }
}

fn name_of(e: &Entity) -> String {
    e.get_value::<Box<[u8]>>(&K_NAME)
        .map(|b| String::from_utf8_lossy(&b).into_owned())
        .unwrap_or_default()
}

fn int<T: TryFrom<i64> + Default>(e: &Entity, key: u64) -> T {
    e.get_value::<i64>(&key)
        .and_then(|v| T::try_from(v.max(0)).ok())
        .unwrap_or_default()
}

fn items_of(e: &Entity) -> Vec<ItemId> {
    let count = e.get_value::<u64>(&K_UPGRADES).unwrap_or(0);
    let mut out = Vec::new();
    for i in 0..count {
        // A garbage count ends at the first missing element.
        let Some(token) = e.get_value::<u32>(&element_key(K_UPGRADES, i)) else {
            break;
        };
        if token != 0 {
            out.push(token);
        }
    }
    out
}

fn player_of(e: &Entity) -> Option<PlayerSnapshot> {
    // Spectators and observers (team 1) and anything else are dropped here.
    let team = normalize_team(i64::try_from(e.get_value::<u64>(&K_TEAM)?).ok()?)?;
    Some(PlayerSnapshot {
        account_id: u32::try_from(e.get_value::<u64>(&K_ACCOUNT).unwrap_or(0)).unwrap_or(0),
        name: name_of(e),
        team,
        hero_id: u32::try_from(e.get_value::<u64>(&K_HERO).unwrap_or(0)).unwrap_or(0),
        items: items_of(e),
        kills: int(e, K_KILLS),
        deaths: int(e, K_DEATHS),
        assists: int(e, K_ASSISTS),
        level: int(e, K_LEVEL),
        net_worth: int(e, K_NET_WORTH),
        gold: int(e, K_GOLD),
    })
}

fn snapshot<D: haste::demostream::DemoStream>(
    parser: &Parser<D, NopVisitor>,
    tps: i32,
) -> GameSnapshot {
    let mut snap = GameSnapshot::default();
    let Some(entities) = parser.context().entities() else {
        return snap;
    };
    for (_, e) in entities.iter() {
        if e.serializer_name_heq(PLAYER_CLASS) {
            if let Some(p) = player_of(e) {
                snap.players.push(p);
            }
        } else if e.serializer_name_heq(RULES_CLASS) {
            snap.match_id = e.get_value::<u64>(&K_MATCH_ID).filter(|id| *id != 0);
            snap.expected_players =
                u32::try_from(e.get_value::<u64>(&K_EXPECTED).unwrap_or(0)).unwrap_or(0);
            let mode = |k| u32::try_from(e.get_value::<u64>(&k).unwrap_or(0)).unwrap_or(0);
            snap.modes = Some((mode(K_GAME_MODE), mode(K_MATCH_MODE)));
            let paused =
                i32::try_from(e.get_value::<i64>(&K_PAUSED_TICKS).unwrap_or(0)).unwrap_or(0);
            snap.game_time = game_time_secs(
                parser.context().tick(),
                paused,
                tps,
                e.get_value::<f32>(&K_START_TIME).unwrap_or(0.0),
            );
            // VERIFY: assumes the same 2/3 numbering as player teams; -1 while running.
            snap.winning_team = e.get_value::<i64>(&K_WINNING_TEAM).and_then(normalize_team);
        }
    }
    snap
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hash helpers must agree with haste's own key construction, or every read returns None.
    #[test]
    fn element_keys_and_path_keys_use_haste_hashing() {
        assert_eq!(
            fxhash::add_u64_to_hash(fkey_from_path(&["a"]), fkey_from_path(&["b"])),
            fkey_from_path(&["a", "b"])
        );
        assert_ne!(element_key(K_UPGRADES, 0), element_key(K_UPGRADES, 1));
        assert_ne!(K_HERO, K_ACCOUNT);
    }

    #[tokio::test]
    async fn connecting_to_a_closed_port_fails_cleanly() {
        let c = HasteConnector::new(None, Duration::from_millis(500));
        let r = c.connect("http://127.0.0.1:9/tv/1_1", 1).await;
        assert!(matches!(r, Err(SessionError::Connect(_))));
    }
}
