use std::{collections::VecDeque, sync::Arc, time::Duration};

use stalemate_core::{
    broadcast::{async_trait, Connector, Session, SessionError, Step},
    settings::BroadcastRules,
    types::{MatchId, ParserEvent, RosterEntry},
};

use crate::{
    collect_events,
    download::{DownloadOpts, Downloader},
    mapping_from_events,
    pseudo::SYNTHETIC_BASE,
    pseudonymize_api_body, pseudonymize_events,
    serve::ReplayServer,
};

const ME: u32 = 100;
const ALLY: u32 = 2_000_111;
const FOE: u32 = 3_000_222;

fn roster() -> ParserEvent {
    let p = |account_id, name: &str, team, hero_id| RosterEntry {
        account_id,
        name: name.into(),
        team,
        hero_id,
    };
    ParserEvent::Roster {
        players: vec![
            p(ME, "Hatch", 0, 1),
            p(ALLY, "SecretAlly", 0, 2),
            p(FOE, "Enemy Name", 1, 3),
            p(0, "Bot Jane", 1, 4),
        ],
    }
}

fn events() -> Vec<ParserEvent> {
    vec![
        roster(),
        ParserEvent::Tick { t_game: 5 },
        ParserEvent::Kda {
            account_id: FOE,
            kills: 1,
            deaths: 0,
            assists: 2,
        },
        ParserEvent::Chat {
            account_id: ALLY,
            team_only: false,
            text: "hello there".into(),
        },
        ParserEvent::Damage {
            attacker: Some(FOE),
            victim: ME,
            amount: 10,
            source: "x".into(),
        },
        ParserEvent::Inventory {
            account_id: ALLY,
            items: vec![7],
        },
        ParserEvent::End { winning_team: 0 },
    ]
}

#[test]
fn mapping_is_stable_and_follows_team_then_id() {
    let a = mapping_from_events(ME, &events()).unwrap();
    let mut shuffled = events();
    shuffled.reverse();
    let b = mapping_from_events(ME, &shuffled).unwrap();
    assert_eq!(a.id(ALLY), Some(SYNTHETIC_BASE + 1));
    assert_eq!(a.id(FOE), Some(SYNTHETIC_BASE + 2));
    assert_eq!(a.id(ALLY), b.id(ALLY));
    assert_eq!(a.id(FOE), b.id(FOE));
    assert_eq!(a.id(ME), Some(ME), "the developer keeps their ID");
    assert_eq!(a.id(0), Some(0), "bots stay 0");
}

#[test]
fn output_has_no_original_ids_names_or_chat() {
    let p = mapping_from_events(ME, &events()).unwrap();
    let text = pseudonymize_events(&p, &events()).unwrap();
    for bad in [
        "2000111",
        "3000222",
        "SecretAlly",
        "Enemy Name",
        "Bot Jane",
        "hello there",
        "Chat",
    ] {
        assert!(!text.contains(bad), "{bad} leaked:\n{text}");
    }
    assert!(text.contains("Player 1") && text.contains("Player 2"));
    assert!(
        text.contains("\"Hatch\""),
        "the developer's name stays (DEC-060)"
    );
    assert!(text.contains(&(SYNTHETIC_BASE + 2).to_string()));
    // The damage line keeps the relation attacker -> victim.
    let dmg = text.lines().find(|l| l.contains("Damage")).unwrap();
    let v: serde_json::Value = serde_json::from_str(dmg).unwrap();
    assert_eq!(v["attacker"], SYNTHETIC_BASE + 2);
    assert_eq!(v["victim"], ME);
    // Every line is still a ParserEvent.
    for l in text.lines() {
        serde_json::from_str::<ParserEvent>(l).unwrap();
    }
}

#[test]
fn leak_scan_fails_closed() {
    let p = mapping_from_events(ME, &events()).unwrap();
    assert!(!p.find_leaks("{\"x\": 3000222}").is_empty());
    assert!(!p.find_leaks("{\"x\": \"SecretAlly\"}").is_empty());
    assert!(p.find_leaks("{\"x\": 30002220}").is_empty());
}

#[test]
fn api_bodies_share_the_mapping_and_drop_profile_links() {
    let mut p = mapping_from_events(ME, &events()).unwrap();
    let body = serde_json::json!([
        {"account_id": FOE, "personaname": "Enemy Name", "avatar": "http://x/a.jpg",
         "profileurl": "https://steamcommunity.com/profiles/76561198960531950"},
        {"account_id": 555, "personaname": "Stranger"}
    ]);
    let out = pseudonymize_api_body(&mut p, body.to_string().as_bytes()).unwrap();
    let text = String::from_utf8(out).unwrap();
    assert!(!text.contains("Enemy Name") && !text.contains("Stranger"));
    assert!(!text.contains("steamcommunity") && !text.contains("avatar"));
    assert!(text.contains(&(SYNTHETIC_BASE + 2).to_string()));
    // The extra player is numbered after the roster players.
    assert_eq!(p.id(555), Some(SYNTHETIC_BASE + 3));
}

struct Mock(Vec<ParserEvent>);

struct MockSession(VecDeque<Step>);

#[async_trait]
impl Session for MockSession {
    async fn next_step(&mut self) -> Result<Option<Step>, SessionError> {
        Ok(self.0.pop_front())
    }
}

#[async_trait]
impl Connector for Mock {
    async fn connect(&self, _: &str, _: MatchId) -> Result<Box<dyn Session>, SessionError> {
        let step = Step {
            events: self.0.clone(),
            ..Step::default()
        };
        Ok(Box::new(MockSession(VecDeque::from([step]))))
    }
}

fn rules() -> BroadcastRules {
    BroadcastRules {
        backoff_s: vec![2, 4],
        stale_delay_s: 30,
        request_timeout_s: 1,
    }
}

#[tokio::test]
async fn normalize_on_a_tiny_synthetic_input() {
    let got = collect_events(Arc::new(Mock(events())), "http://unused/tv/1", 1, &rules()).await;
    assert!(got.contains(&ParserEvent::End { winning_team: 0 }));
    let p = mapping_from_events(ME, &got).unwrap();
    let text = pseudonymize_events(&p, &got).unwrap();
    assert!(!text.contains("SecretAlly") && !text.contains("hello there"));
    assert!(text.lines().count() >= 5);
    let meta = crate::meta_json(1, ME, 1_700_000_000);
    assert_eq!(meta["me"], ME);
}

fn write_relay(dir: &std::path::Path) {
    std::fs::write(
        dir.join("sync"),
        br#"{"fragment":2,"signup_fragment":0,"keyframe_interval":1,"rtdelay":3}"#,
    )
    .unwrap();
    std::fs::write(dir.join("0_start"), b"start-bytes").unwrap();
    std::fs::write(dir.join("2_full"), b"full-bytes").unwrap();
    std::fs::write(dir.join("2_delta"), b"d2").unwrap();
    std::fs::write(dir.join("3_delta"), b"d3").unwrap();
}

#[tokio::test]
async fn the_replay_server_answers_like_the_relay() {
    let dir = tempfile::tempdir().unwrap();
    write_relay(dir.path());
    let server = ReplayServer::start(dir.path()).await.unwrap();
    let base = server.base_url(1);
    let get = |p: &str| {
        let url = format!("{base}/{p}");
        async move { reqwest::get(url).await.unwrap() }
    };
    let sync: serde_json::Value = get("sync").await.json().await.unwrap();
    assert_eq!(sync["fragment"], 2);
    assert_eq!(
        get("0/start").await.bytes().await.unwrap().as_ref(),
        b"start-bytes"
    );
    assert_eq!(get("9/delta").await.status().as_u16(), 404);
    assert!(server.fragment_bytes_served() > 0);
}

#[tokio::test]
async fn the_downloader_saves_what_the_replay_server_serves() {
    let src = tempfile::tempdir().unwrap();
    write_relay(src.path());
    let server = ReplayServer::start(src.path()).await.unwrap();
    let dst = tempfile::tempdir().unwrap();
    let opts = DownloadOpts {
        idle_timeout: Duration::from_millis(600),
        poll: Some(Duration::from_millis(50)),
        request_timeout: Duration::from_secs(2),
    };
    let mut d = Downloader::new(&server.base_url(1), dst.path(), opts).unwrap();
    while d.step().await.unwrap().is_some() {}
    for f in ["sync", "0_start", "2_full", "2_delta", "3_delta"] {
        assert!(dst.path().join(f).exists(), "{f} missing");
    }
    assert_eq!(std::fs::read(dst.path().join("3_delta")).unwrap(), b"d3");
}
