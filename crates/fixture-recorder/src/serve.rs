//! A tiny local HTTP server that replays a saved relay (`<dir>/relay/`) the way the real one
//! answers. It is the TC-04-1 replay harness: `broadcast::run` with a fixed-URL locator and the
//! real haste connector talks to it on 127.0.0.1.
//!
//! Paths follow the relay's scheme (haste's `BroadcastHttp`):
//!
//! ```text
//! GET <base>/sync            JSON, see below
//! GET <base>/<n>/start       relay/<n>_start
//! GET <base>/<n>/full        relay/<n>_full
//! GET <base>/<n>/delta       relay/<n>_delta
//! ```
//!
//! A missing file is a 404, which is what haste reads as "not ready yet" and, after its retries, as
//! the end of the stream.
//!
//! `/sync` is built from the recorded `relay/sync` and the files present:
//! - The first call points the client at the first recorded fragment (`fragment` is the lowest
//!   `<n>_full`, `signup_fragment` the lowest `<n>_start`). That is where the client starts.
//! - Later calls report the newest recorded fragment as the live edge, so the client never counts
//!   itself as ahead of it.
//! - `keyframe_interval` is 1 s, so haste's retries at the end of the recording are quick.
//!
//! Every request is logged with the status and body size, so tests can check the order haste asks
//! in and the byte count it received (TC-04-4).

use std::{
    io,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, Mutex,
    },
};

use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    task::JoinHandle,
};

/// One answered request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Served {
    pub path: String,
    pub status: u16,
    pub bytes: usize,
}

pub struct ReplayServer {
    addr: SocketAddr,
    log: Arc<Mutex<Vec<Served>>>,
    task: JoinHandle<()>,
}

impl Drop for ReplayServer {
    fn drop(&mut self) {
        self.task.abort();
    }
}

struct Shared {
    dir: PathBuf,
    log: Arc<Mutex<Vec<Served>>>,
    sync_calls: AtomicUsize,
}

impl ReplayServer {
    /// Listen on 127.0.0.1 (a free port) and serve `relay_dir`.
    pub async fn start(relay_dir: &Path) -> io::Result<Self> {
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let addr = listener.local_addr()?;
        let log = Arc::new(Mutex::new(Vec::new()));
        let shared = Arc::new(Shared {
            dir: relay_dir.to_path_buf(),
            log: log.clone(),
            sync_calls: AtomicUsize::new(0),
        });
        let task = tokio::spawn(async move {
            loop {
                let Ok((sock, _)) = listener.accept().await else {
                    return;
                };
                tokio::spawn(handle(sock, shared.clone()));
            }
        });
        Ok(Self { addr, log, task })
    }

    /// `http://127.0.0.1:<port>/tv/<match_id>`: the base URL to give the client.
    pub fn base_url(&self, match_id: u64) -> String {
        format!("http://{}/tv/{match_id}", self.addr)
    }

    pub fn log(&self) -> Vec<Served> {
        self.log.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Bytes of fragment bodies (`start`, `full`, `delta`) answered with 200.
    pub fn fragment_bytes_served(&self) -> u64 {
        self.log()
            .iter()
            .filter(|s| s.status == 200 && !s.path.ends_with("/sync"))
            .map(|s| s.bytes as u64)
            .sum()
    }
}

/// Which file a request path means. `None` for anything else.
pub fn file_for(path: &str) -> Option<String> {
    let path = path.split('?').next().unwrap_or(path);
    let segs: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
    match segs.as_slice() {
        [.., "sync"] => Some("sync".to_string()),
        [.., n, kind @ ("start" | "full" | "delta")] => {
            let n: u64 = n.parse().ok()?;
            Some(format!("{n}_{kind}"))
        }
        _ => None,
    }
}

/// `(kind, n)` for a relay file name such as `12_delta`.
pub fn parse_name(name: &str) -> Option<(&str, u64)> {
    let (n, kind) = name.split_once('_')?;
    if !matches!(kind, "start" | "full" | "delta") {
        return None;
    }
    Some((kind, n.parse().ok()?))
}

fn sync_body(shared: &Shared) -> Result<Vec<u8>, String> {
    let raw = std::fs::read(shared.dir.join("sync")).map_err(|e| format!("relay/sync: {e}"))?;
    let mut v: serde_json::Value =
        serde_json::from_slice(&raw).map_err(|e| format!("relay/sync is not JSON: {e}"))?;
    let (mut first_start, mut first_full, mut last): (Option<u64>, Option<u64>, Option<u64>) =
        (None, None, None);
    for entry in std::fs::read_dir(&shared.dir)
        .map_err(|e| e.to_string())?
        .flatten()
    {
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some((kind, n)) = parse_name(&name) else {
            continue;
        };
        match kind {
            "start" => first_start = Some(first_start.map_or(n, |x| x.min(n))),
            "full" => first_full = Some(first_full.map_or(n, |x| x.min(n))),
            _ => {}
        }
        if kind != "start" {
            last = Some(last.map_or(n, |x| x.max(n)));
        }
    }
    let (Some(start), Some(full), Some(last)) = (first_start, first_full, last) else {
        return Err("relay/ has no start and full fragment".into());
    };
    let first_call = shared.sync_calls.fetch_add(1, Ordering::SeqCst) == 0;
    let obj = v.as_object_mut().ok_or("relay/sync is not an object")?;
    obj.insert("signup_fragment".into(), start.into());
    obj.insert(
        "fragment".into(),
        if first_call { full } else { last }.into(),
    );
    obj.insert("keyframe_interval".into(), 1.into());
    serde_json::to_vec(&v).map_err(|e| e.to_string())
}

async fn handle(mut sock: TcpStream, shared: Arc<Shared>) {
    let mut buf = Vec::new();
    let mut chunk = [0u8; 2048];
    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
        match sock.read(&mut chunk).await {
            Ok(0) | Err(_) => return,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
        }
        if buf.len() > 16 * 1024 {
            return;
        }
    }
    let head = String::from_utf8_lossy(&buf);
    let line = head.lines().next().unwrap_or("");
    let mut parts = line.split_whitespace();
    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));

    let (status, body): (u16, Vec<u8>) = if method != "GET" {
        (405, Vec::new())
    } else {
        match file_for(path) {
            None => (404, Vec::new()),
            Some(name) if name == "sync" => match sync_body(&shared) {
                Ok(b) => (200, b),
                Err(_) => (404, Vec::new()),
            },
            Some(name) => match tokio::fs::read(shared.dir.join(&name)).await {
                Ok(b) => (200, b),
                Err(_) => (404, Vec::new()),
            },
        }
    };
    shared
        .log
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .push(Served {
            path: path.to_string(),
            status,
            bytes: body.len(),
        });
    let reason = match status {
        200 => "OK",
        405 => "Method Not Allowed",
        _ => "Not Found",
    };
    let head = format!(
        "HTTP/1.1 {status} {reason}\r\nContent-Length: {}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n",
        body.len()
    );
    let _ = sock.write_all(head.as_bytes()).await;
    let _ = sock.write_all(&body).await;
    let _ = sock.shutdown().await;
}
