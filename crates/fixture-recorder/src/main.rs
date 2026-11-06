//! `stalemate-fixture-recorder` (PRG-27, T-013). See README.md.

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::{anyhow, bail, Context, Result};
use stalemate_core::{
    api_client::{ApiClient, ApiConfig, ApiError, Endpoint},
    broadcast::{LocateError, Locator, UrlLocator},
    settings::Rules,
    storage::Storage,
};
use stalemate_fixture_recorder::{
    download::{DownloadOpts, Downloader},
    mapping_from_events, meta_json, pseudonymize_api_body, pseudonymize_events, replay_relay,
};

const USAGE: &str = "\
stalemate-fixture-recorder: records your own matches as test fixtures (T-013)

  record    --match-id ID --out DIR [--url URL] [--wait-secs N] [--idle-secs N] [--rules FILE]
  normalize --raw DIR --match-id ID --me ACCOUNT_ID [--out DIR] [--rules FILE]
  api       --raw DIR --match-id ID --me ACCOUNT_ID [--out DIR] [--rules FILE]

record saves raw relay files into --out (keep it outside the repo or under fixtures-raw/, which is
git-ignored). normalize and api write pseudonymized files, safe to commit, under
tests/fixtures/ by default. See crates/fixture-recorder/README.md.";

/// File name, endpoint, path, query.
type Call = (String, Endpoint, String, Vec<(&'static str, String)>);

struct Args(Vec<String>);

impl Args {
    fn get(&self, flag: &str) -> Option<&str> {
        let i = self.0.iter().position(|a| a == flag)?;
        self.0.get(i + 1).map(String::as_str)
    }

    fn need(&self, flag: &str) -> Result<&str> {
        self.get(flag)
            .ok_or_else(|| anyhow!("missing {flag}\n\n{USAGE}"))
    }

    fn num<T: std::str::FromStr>(&self, flag: &str) -> Result<Option<T>> {
        self.get(flag)
            .map(|s| s.parse().map_err(|_| anyhow!("{flag} must be a number")))
            .transpose()
    }

    fn rules(&self) -> Result<Rules> {
        let path = self.get("--rules").unwrap_or("config/rules.toml");
        Rules::load(Path::new(path)).map_err(|e| anyhow!("{e}"))
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.is_empty() || argv[0] == "-h" || argv[0] == "--help" {
        println!("{USAGE}");
        return Ok(());
    }
    let cmd = argv.remove(0);
    let args = Args(argv);
    match cmd.as_str() {
        "record" => record(&args).await,
        "normalize" => normalize(&args).await,
        "api" => api(&args).await,
        _ => bail!("unknown subcommand {cmd:?}\n\n{USAGE}"),
    }
}

fn api_client(rules: &Rules) -> Result<ApiClient> {
    ApiClient::new(Storage::open_in_memory()?, &rules.api, ApiConfig::default())
        .map_err(|e| anyhow!("{e}"))
}

/// Find the relay URL: from `--url`, or through core's locator, waiting up to `--wait-secs`.
async fn locate(args: &Args, rules: &Rules, match_id: u64) -> Result<String> {
    if let Some(u) = args.get("--url") {
        return Ok(u.to_string());
    }
    let locator = Locator::new(Arc::new(api_client(rules)?));
    let deadline =
        std::time::Instant::now() + Duration::from_secs(args.num("--wait-secs")?.unwrap_or(600));
    loop {
        match locator.locate(match_id).await {
            Ok(url) => return Ok(url),
            Err(LocateError::NotAvailable) => eprintln!("no Broadcast listed for the match yet"),
            Err(LocateError::Busy { retry_after_s }) => {
                eprintln!("the locator asked us to wait {retry_after_s}s");
            }
            Err(e) => eprintln!("locator: {e}"),
        }
        if std::time::Instant::now() >= deadline {
            bail!("no Broadcast found in time; start the match first, or pass --url");
        }
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

async fn record(args: &Args) -> Result<()> {
    // Recording happens only when a directory is given.
    let out = PathBuf::from(args.need("--out")?);
    let match_id: u64 = args.need("--match-id")?.parse().context("--match-id")?;
    let rules = args.rules()?;
    let url = locate(args, &rules, match_id).await?;
    let opts = DownloadOpts {
        idle_timeout: Duration::from_secs(args.num("--idle-secs")?.unwrap_or(90)),
        request_timeout: Duration::from_secs(rules.broadcast.request_timeout_s),
        ..DownloadOpts::default()
    };
    let relay = out.join("relay");
    let mut d = Downloader::new(&url, &relay, opts)?;
    eprintln!("recording to {} (Ctrl-C to stop)", relay.display());
    while let Some(saved) = d.step().await? {
        eprintln!("saved {saved:?} ({} files, {} bytes)", d.files(), d.bytes());
    }
    eprintln!("done: {} files, {} bytes", d.files(), d.bytes());
    Ok(())
}

fn started_unix(relay: &Path) -> u64 {
    std::fs::metadata(relay.join("sync"))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or_else(
            || {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs())
            },
            |d| d.as_secs(),
        )
}

async fn normalize(args: &Args) -> Result<()> {
    let raw = PathBuf::from(args.need("--raw")?);
    let match_id: u64 = args.need("--match-id")?.parse().context("--match-id")?;
    let me: u32 = args.need("--me")?.parse().context("--me")?;
    let out = args.get("--out").map_or_else(
        || PathBuf::from(format!("tests/fixtures/fx-bc/{match_id}")),
        PathBuf::from,
    );
    if out.starts_with(&raw) || raw.starts_with(&out) {
        bail!("--out must not be the raw recording directory");
    }
    let rules = args.rules()?;
    let relay = raw.join("relay");
    let events = replay_relay(&relay, match_id, me, &rules.broadcast).await?;
    let p = mapping_from_events(me, &events)?;
    let text = pseudonymize_events(&p, &events)?;
    std::fs::create_dir_all(&out)?;
    std::fs::write(out.join("events.jsonl"), text)?;
    std::fs::write(
        out.join("meta.json"),
        serde_json::to_string_pretty(&meta_json(match_id, me, started_unix(&relay)))?,
    )?;
    eprintln!("wrote {} events to {}", events.len(), out.display());
    Ok(())
}

async fn api(args: &Args) -> Result<()> {
    let raw = PathBuf::from(args.need("--raw")?);
    let match_id: u64 = args.need("--match-id")?.parse().context("--match-id")?;
    let me: u32 = args.need("--me")?.parse().context("--me")?;
    let out = args.get("--out").map_or_else(
        || PathBuf::from(format!("tests/fixtures/fx-api/{match_id}")),
        PathBuf::from,
    );
    let rules = args.rules()?;
    let events = replay_relay(&raw.join("relay"), match_id, me, &rules.broadcast).await?;
    let mut p = mapping_from_events(me, &events)?;
    let ids = p.human_ids();
    let client = api_client(&rules)?;
    std::fs::create_dir_all(&out)?;

    // (file name, endpoint, path, query). All calls go through the client's limiter and refuse
    // force_refetch and refresh. Nothing here asks Valve about another player.
    let csv = ids.iter().map(u32::to_string).collect::<Vec<_>>().join(",");
    let mut calls: Vec<Call> = Vec::new();
    for &id in &ids {
        let shown = p.id(id).ok_or_else(|| anyhow!("unmapped player"))?;
        calls.push((
            format!("match-history-{shown}.json"),
            Endpoint::MatchHistory,
            format!("/v1/players/{id}/match-history"),
            vec![],
        ));
    }
    let batch = vec![("account_ids", csv)];
    calls.push((
        "rank.json".into(),
        Endpoint::PlayerRank,
        "/v1/players/rank".into(),
        batch.clone(),
    ));
    calls.push((
        "hero-stats.json".into(),
        Endpoint::PlayerHeroStats,
        "/v1/players/hero-stats".into(),
        batch.clone(),
    ));
    calls.push((
        "steam.json".into(),
        Endpoint::SteamProfiles,
        "/v1/players/steam".into(),
        batch,
    ));
    calls.push((
        "metadata.json".into(),
        Endpoint::MatchMetadataSingle,
        format!("/v1/matches/{match_id}/metadata"),
        vec![("disable_steam", "true".to_string())],
    ));

    let mut saved = 0;
    for (name, ep, path, query) in calls {
        match client.fetch(ep, &path, &query).await {
            Ok(body) => {
                std::fs::write(out.join(&name), pseudonymize_api_body(&mut p, &body)?)?;
                saved += 1;
            }
            // An opted-out player or a match without data: skipped, never retried or worked around.
            Err(ApiError::NoPublicData) => eprintln!("{name}: no public data, skipped"),
            Err(e) => eprintln!("{name}: {e}, skipped"),
        }
    }
    eprintln!("saved {saved} responses to {}", out.display());
    Ok(())
}
