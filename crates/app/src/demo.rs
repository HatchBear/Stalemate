//! Demo data and a demo controller behind `--demo`. All names are made up ("Hero 1",
//! "Item A"). Nothing here is Valve data. The real wiring replaces this with core state.

use crate::builder::{BuilderDraft, BuilderError};
use crate::diag::{self, SaveEnv, SaveResult};
use crate::i18n::I18n;
use crate::ui::{AppState, BuildsTab, MainWindow};
use crate::ui_map::{self, apply_all};
use crate::view_model::*;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use stalemate_core::diagnostics::{About, Diagnostics, MatchDiag, PerfSample};
use stalemate_core::settings::DiagnosticsRules;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const STATES: usize = 5;

fn named(prefix: &str, n: u32) -> NamedOption {
    NamedOption {
        id: n,
        name: format!("{prefix} {n}"),
        category: None,
    }
}

fn item_name(i: u32) -> String {
    let letter = (b'A' + (i % 26) as u8) as char;
    format!("Item {letter}")
}

/// Demo items cycle through the three shop categories so every colour shows (DEC-068).
fn item_category(i: u32) -> Option<ItemCategory> {
    Some(match i % 3 {
        0 => ItemCategory::Weapon,
        1 => ItemCategory::Vitality,
        _ => ItemCategory::Spirit,
    })
}

fn item_vm(i: u32) -> ItemVm {
    ItemVm {
        name: item_name(i),
        icon: None,
        category: item_category(i),
    }
}

/// `count` demo items in sections of `per` items.
fn demo_sections(first: u32, count: u32, titles: &[&str]) -> Vec<SectionVm> {
    let per = count.div_ceil(titles.len().max(1) as u32).max(1);
    titles
        .iter()
        .enumerate()
        .filter_map(|(n, title)| {
            let start = n as u32 * per;
            let end = (start + per).min(count);
            (start < end).then(|| SectionVm {
                title: (*title).into(),
                items: (start..end).map(|i| item_vm(first + i)).collect(),
            })
        })
        .collect()
}

fn stats(overall: (u32, u32), last: (u32, u32), rank: &str, hero_matches: u32) -> ScoutStats {
    ScoutStats {
        overall: Record {
            wins: overall.0,
            losses: overall.1,
        },
        last: Record {
            wins: last.0,
            losses: last.1,
        },
        last_window: 20,
        rank: Some(rank.to_owned()),
        mmr_trend_per_10: None,
        mains: (1..=5)
            .map(|i| MainHero {
                hero_name: format!("Hero {i}"),
                matches: 120 / i,
                wins: 60 / i + i,
            })
            .collect(),
        matches_on_hero: hero_matches,
    }
}

fn card(hero: u32, player: u32, you: bool, body: CardBody) -> ScoutCardVm {
    ScoutCardVm {
        hero_name: format!("Hero {hero}"),
        player_name: if you {
            "You".into()
        } else {
            format!("Player {player}")
        },
        is_you: you,
        body,
    }
}

pub fn demo_vm() -> AppVm {
    let your_team = vec![
        card(
            1,
            1,
            true,
            CardBody::Stats(stats((210, 190), (11, 9), "Rank A", 54)),
        ),
        card(
            2,
            2,
            false,
            CardBody::Stats(stats((90, 70), (13, 7), "Rank B", 12)),
        ),
        card(3, 3, false, CardBody::Loading),
        card(
            4,
            4,
            false,
            CardBody::Stats(stats((400, 380), (9, 11), "Rank A", 230)),
        ),
        card(5, 5, false, CardBody::HeroOnly),
    ];
    let enemy_team = vec![
        card(
            6,
            6,
            false,
            CardBody::Stats(stats((55, 61), (4, 3), "Rank C", 6)),
        ),
        card(7, 7, false, CardBody::NoPublicData),
        card(
            8,
            8,
            false,
            CardBody::Stats(stats((800, 700), (12, 8), "Rank B", 310)),
        ),
        card(
            9,
            9,
            false,
            CardBody::Stats(stats((150, 160), (10, 10), "Rank B", 40)),
        ),
        card(
            10,
            10,
            false,
            CardBody::Stats(stats((33, 40), (8, 12), "Rank D", 3)),
        ),
        card(
            11,
            11,
            false,
            CardBody::Stats(stats((260, 240), (14, 6), "Rank A", 98)),
        ),
    ];

    let enemies = (6..=11)
        .map(|h| EnemyRowVm {
            hero_name: format!("Hero {h}"),
            portrait: None,
            player_name: format!("Player {h}"),
            team: Side::Sapphire,
            // LS-08 is allowed for every player (DEC-059).
            souls: Some(9_000 + h * 1_350),
            items: (0..(h % 4 + 2)).map(|i| item_vm(h + i)).collect(),
            // Blocked until verified (T-007): the demo leaves KDA and level off.
            kda: None,
            level: None,
        })
        .collect();
    let log = [
        (612, 7, 3, PurchaseAction::Buy),
        (598, 9, 5, PurchaseAction::Upgrade),
        (540, 6, 2, PurchaseAction::Sell),
        (421, 8, 1, PurchaseAction::Buy),
        (305, 10, 0, PurchaseAction::Buy),
    ]
    .into_iter()
    .map(|(t, p, i, action)| PurchaseEntryVm {
        t_game_secs: t,
        player_name: format!("Player {p}"),
        item_name: item_name(i),
        category: item_category(i),
        action,
    })
    .collect();

    let list = vec![
        ReviewListItemVm {
            match_id: 1,
            pending: false,
            outcome: Outcome::Win,
            hero_name: "Hero 1".into(),
            duration_secs: 34 * 60 + 12,
            ended_ago_secs: Some(3 * 3600),
        },
        ReviewListItemVm {
            match_id: 2,
            pending: false,
            outcome: Outcome::Loss,
            hero_name: "Hero 4".into(),
            duration_secs: 41 * 60 + 5,
            ended_ago_secs: Some(26 * 3600),
        },
        ReviewListItemVm {
            match_id: 3,
            pending: false,
            outcome: Outcome::NoResult,
            hero_name: "Hero 2".into(),
            duration_secs: 7 * 60 + 40,
            ended_ago_secs: Some(3 * 86_400),
        },
        ReviewListItemVm {
            match_id: 4,
            pending: false,
            outcome: Outcome::Abandoned,
            hero_name: "Hero 3".into(),
            duration_secs: 12 * 60 + 5,
            ended_ago_secs: Some(5 * 86_400),
        },
    ];

    AppVm {
        offline: false,
        home: HomeVm {
            checks: vec![
                SetupCheck {
                    id: CheckId::Steam,
                    state: CheckState::Pass,
                    value: Some("~/.steam/steam".into()),
                },
                SetupCheck {
                    id: CheckId::Condebug,
                    state: CheckState::Fail,
                    value: None,
                },
                SetupCheck {
                    id: CheckId::AccountId,
                    state: CheckState::Pass,
                    value: Some("123456789".into()),
                },
                SetupCheck {
                    id: CheckId::Broadcast,
                    state: CheckState::Pending,
                    value: None,
                },
                SetupCheck {
                    id: CheckId::DeadlockApi,
                    state: CheckState::Pass,
                    value: None,
                },
            ],
            last_match: Some(MatchSummary {
                outcome: Outcome::Win,
                hero_name: "Hero 1".into(),
                duration_secs: 34 * 60 + 12,
                ended_ago_secs: Some(3 * 3600),
            }),
            setup_result: CheckState::Pending,
            phase: Phase::Idle,
        },
        scouting: ScoutingVm {
            visible: true,
            state: DataState::Loaded,
            your_side: Side::Amber,
            your_team,
            enemy_team,
        },
        live: LiveVm {
            state: DataState::Loaded,
            broadcast_delay_secs: Some(95),
            delay_stale: false,
            enemies,
            purchase_log: log,
        },
        reviews: ReviewsVm {
            state: DataState::Loaded,
            list,
            selected: None,
            report: None,
        },
        builds: BuildsVm {
            state: DataState::Loaded,
            library: vec![
                BuildVm {
                    build_id: "b1".into(),
                    name: "Build 1".into(),
                    hero_name: "Hero 1".into(),
                    source: BuildSource::InGame,
                    item_count: 14,
                    sections: demo_sections(0, 14, &["Early game", "Mid game", "Late game"]),
                    active: true,
                    items_removed: 0,
                },
                BuildVm {
                    build_id: "b2".into(),
                    name: "Build 2".into(),
                    hero_name: "Hero 1".into(),
                    source: BuildSource::Manual,
                    item_count: 11,
                    sections: demo_sections(5, 11, &["Lane", "Fights"]),
                    active: false,
                    items_removed: 2,
                },
                BuildVm {
                    build_id: "b3".into(),
                    name: "Build 3".into(),
                    hero_name: "Hero 2".into(),
                    source: BuildSource::External,
                    item_count: 1,
                    sections: demo_sections(2, 1, &["Starter"]),
                    active: false,
                    items_removed: 0,
                },
            ],
            heroes: (1..=12).map(|n| named("Hero", n)).collect(),
            community_state: DataState::Empty,
            community: vec![],
        },
        settings: SettingsVm {
            account_id: Some(123_456_789),
            account_invalid: false,
            version: env!("CARGO_PKG_VERSION").into(),
            diag_enabled: true,
            ..Default::default()
        },
    }
}

fn demo_reports() -> HashMap<u64, ReviewReportVm> {
    let sources = |a, b, c, d, e| {
        vec![
            SoulSourceVm {
                kind: SoulSourceKind::LaneTroopers,
                souls: a,
            },
            SoulSourceVm {
                kind: SoulSourceKind::NeutralCamps,
                souls: b,
            },
            SoulSourceVm {
                kind: SoulSourceKind::Breakables,
                souls: c,
            },
            SoulSourceVm {
                kind: SoulSourceKind::Urn,
                souls: d,
            },
            SoulSourceVm {
                kind: SoulSourceKind::SinnersSacrifice,
                souls: e,
            },
        ]
    };
    let report = |id, outcome, hero: &str, secs, lead, s| ReviewReportVm {
        match_id: id,
        outcome,
        hero_name: hero.into(),
        duration_secs: secs,
        lane_check_secs: 540,
        lane_soul_lead: lead,
        soul_sources: s,
    };
    HashMap::from([
        (
            1,
            report(
                1,
                Outcome::Win,
                "Hero 1",
                34 * 60 + 12,
                Some(420),
                sources(9_400, 7_200, 900, 600, 250),
            ),
        ),
        (
            2,
            report(
                2,
                Outcome::Loss,
                "Hero 4",
                41 * 60 + 5,
                Some(-310),
                sources(8_100, 9_900, 1_200, 0, 0),
            ),
        ),
        (
            3,
            report(
                3,
                Outcome::NoResult,
                "Hero 2",
                7 * 60 + 40,
                None,
                sources(2_100, 800, 100, 0, 0),
            ),
        ),
        (
            4,
            report(
                4,
                Outcome::Abandoned,
                "Hero 3",
                12 * 60 + 5,
                Some(-90),
                sources(3_600, 1_400, 200, 0, 0),
            ),
        ),
    ])
}

fn demo_state(i: usize) -> DataState {
    match i % STATES {
        0 => DataState::Loaded,
        1 => DataState::Loading,
        2 => DataState::Empty,
        3 => DataState::Stale { age_secs: 600 },
        _ => DataState::Error {
            source: "deadlock-api".into(),
        },
    }
}

/// A diagnostics record with made-up content, in a temp folder, so the Settings screen and the
/// save step can be tried without a game. The other players' names are given to the scan as
/// other players, the same way the real pipeline does.
fn demo_diagnostics(dir: &std::path::Path) -> SaveEnv {
    let rules = DiagnosticsRules {
        enabled_default: true,
        keep_days: 14,
        perf_sample_s: 10,
        max_file_mb: 20,
    };
    let diag = Diagnostics::open(&dir.join("diagnostics"), &rules, true);
    for (id, ok) in [
        ("steam", true),
        ("condebug", false),
        ("account_id", true),
        ("deadlock_api", true),
    ] {
        diag.check(id, ok, if ok { "pass" } else { "fail" });
    }
    diag.match_record(&MatchDiag {
        match_id: 1,
        broadcast_located: Some(true),
        broadcast_connect_ms: Some(2_400),
        delay_s: Some(95),
        item_resolve_pct: Some(100.0),
        roster_complete_ms: Some(5_100),
        scouting_lookups: Some(11),
        scouting_failures: Some(1),
        first_card_ms: Some(3_800),
        review_attempts: Some(2),
        review_result: Some("ready".into()),
        purchase_events: Some(23),
        ..Default::default()
    });
    for (cpu, rss) in [(1.2, 180), (1.5, 184), (1.1, 183)] {
        diag.perf(PerfSample {
            cpu_pct: cpu,
            rss_mb: rss,
        });
    }
    for key in ["view-home", "view-live", "view-live", "builder-save"] {
        diag.usage(key);
    }
    diag.marker("the delay looked wrong");
    for h in 2..=11u32 {
        diag.redact_also(1_000 + h, &format!("Player {h}"));
    }
    let log_dir = dir.join("logs");
    let _ = std::fs::create_dir_all(&log_dir);
    let _ = std::fs::write(
        log_dir.join("stalemate-2026-11-06.log"),
        "2026-11-06T12:00:00Z  INFO stalemate_app: started\n\
         2026-11-06T12:00:03Z  INFO stalemate_app: catalog images: 120 present, 0 failed\n",
    );
    SaveEnv {
        diag,
        log_dir,
        out_dir: dir.join("exports"),
        about: About {
            version: env!("CARGO_PKG_VERSION").into(),
            commit: "demo".into(),
            package_format: "dev".into(),
            os: "linux".into(),
            os_version: "Demo OS".into(),
            desktop_session: "KDE Wayland".into(),
            display_scale: Some(1.0),
            steam_kind: "native".into(),
        },
        api_key: Some("demo-api-key-0123456789".into()),
        me: Some(123_456_789),
    }
}

fn demo_dir() -> std::path::PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    std::env::temp_dir().join(format!(
        "stalemate-demo-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ))
}

pub struct Demo {
    pub vm: AppVm,
    dir: std::path::PathBuf,
    pub save_env: SaveEnv,
    i18n: Arc<I18n>,
    draft: BuilderDraft,
    items: Vec<NamedOption>,
    found: Vec<NamedOption>,
    reports: HashMap<u64, ReviewReportVm>,
    state_idx: [usize; 4],
    /// Set once the cycle button is pressed on a view that has no data states.
    no_states_note: bool,
    check_attempts: u32,
    saved: u32,
}

impl Drop for Demo {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

impl Demo {
    pub fn new(i18n: Arc<I18n>) -> Self {
        let dir = demo_dir();
        let save_env = demo_diagnostics(&dir);
        Self {
            vm: demo_vm(),
            dir,
            save_env,
            i18n,
            draft: BuilderDraft::new(),
            items: (0..26)
                .map(|n| NamedOption {
                    id: n,
                    name: item_name(n),
                    category: item_category(n),
                })
                .collect(),
            found: vec![],
            reports: demo_reports(),
            state_idx: [0; 4],
            no_states_note: false,
            check_attempts: 0,
            saved: 0,
        }
    }

    fn state_slot(&mut self, view: i32) -> Option<&mut DataState> {
        match view {
            1 => Some(&mut self.vm.scouting.state),
            2 => Some(&mut self.vm.live.state),
            3 => Some(&mut self.vm.reviews.state),
            4 => Some(&mut self.vm.builds.state),
            _ => None,
        }
    }

    /// Cycles the data state of the view on screen. Returns the text for the demo panel.
    fn cycle_state(&mut self, view: i32) {
        if !(1..=4).contains(&view) {
            self.no_states_note = true;
            return;
        }
        let slot = (view - 1) as usize;
        self.state_idx[slot] = (self.state_idx[slot] + 1) % STATES;
        let state = demo_state(self.state_idx[slot]);
        *self.state_slot(view).expect("view in range") = state;
    }

    /// The demo label for every view, indexed like `current-view`. Views 1 to 4 describe their
    /// own state; the others stay empty until the cycle button is pressed on them.
    fn state_texts(&self) -> Vec<String> {
        (0..6)
            .map(|view| match view {
                1..=4 => self.state_text(view - 1),
                _ if self.no_states_note => self.i18n.t("demo-no-states"),
                _ => String::new(),
            })
            .collect()
    }

    fn push_state_texts(&self, ui: &MainWindow) {
        let texts: Vec<slint::SharedString> =
            self.state_texts().into_iter().map(Into::into).collect();
        ui.global::<AppState>()
            .set_demo_state_texts(slint::ModelRc::new(slint::VecModel::from(texts)));
    }

    fn state_text(&self, slot: usize) -> String {
        match self.state_idx[slot] % STATES {
            0 => self.i18n.t("demo-state-loaded"),
            1 => self.i18n.t("demo-state-loading"),
            2 => self.i18n.t("demo-state-empty"),
            3 => self.i18n.t("demo-state-stale"),
            _ => self.i18n.t("demo-state-error"),
        }
    }
}

/// Connects the demo controller to the window's callbacks.
pub fn wire(ui: &MainWindow, demo: Rc<RefCell<Demo>>) {
    let state = ui.global::<AppState>();
    state.set_demo(true);
    demo.borrow().push_state_texts(ui);

    macro_rules! with {
        ($ui:ident, $demo:ident, |$u:ident, $d:ident $(, $arg:ident)*| $body:block) => {{
            let weak = $ui.as_weak();
            let $demo = demo.clone();
            move |$($arg),*| {
                let Some($u) = weak.upgrade() else { return };
                let _ = &$u;
                let mut $d = $demo.borrow_mut();
                #[allow(unused_variables)]
                let $d = &mut *$d;
                $body
            }
        }};
    }

    state.on_check_again(with!(ui, demo, |u, d| {
        d.check_attempts += 1;
        // First press fails and the second passes, so both results can be seen.
        let pass = d.check_attempts >= 2;
        d.vm.home.setup_result = if pass {
            CheckState::Pass
        } else {
            CheckState::Fail
        };
        if let Some(c) =
            d.vm.home
                .checks
                .iter_mut()
                .find(|c| c.id == CheckId::Condebug)
        {
            c.state = d.vm.home.setup_result;
        }
        ui_map::apply_home(&u, &d.vm.home, &d.i18n);
    }));

    state.on_retry(with!(ui, demo, |u, d, view| {
        if let Some(slot) = d.state_slot(view) {
            *slot = DataState::Loaded;
        }
        if (1..=4).contains(&view) {
            d.state_idx[(view - 1) as usize] = 0;
        }
        let i18n = d.i18n.clone();
        ui_map::apply_scouting(&u, &d.vm.scouting, &i18n);
        ui_map::apply_live(&u, &d.vm.live, &i18n);
        ui_map::apply_reviews(&u, &d.vm.reviews, &i18n);
        ui_map::apply_builds(&u, &d.vm.builds, &i18n);
        d.push_state_texts(&u);
    }));

    state.on_demo_cycle_state(with!(ui, demo, |u, d| {
        let view = u.global::<AppState>().get_current_view();
        d.cycle_state(view);
        let i18n = d.i18n.clone();
        apply_all(&u, &d.vm, &i18n);
        d.push_state_texts(&u);
    }));

    state.on_demo_toggle_offline(with!(ui, demo, |u, d| {
        d.vm.offline = !d.vm.offline;
        u.global::<AppState>().set_offline(d.vm.offline);
    }));

    state.on_select_review(with!(ui, demo, |u, d, i| {
        let Some(item) = usize::try_from(i)
            .ok()
            .and_then(|i| d.vm.reviews.list.get(i))
        else {
            return;
        };
        let id = item.match_id;
        d.vm.reviews.selected = Some(i as usize);
        d.vm.reviews.report = d.reports.get(&id).cloned();
        let i18n = d.i18n.clone();
        ui_map::apply_reviews(&u, &d.vm.reviews, &i18n);
    }));

    state.on_set_active(with!(ui, demo, |u, d, id| {
        let hero =
            d.vm.builds
                .library
                .iter()
                .find(|b| b.build_id == id.as_str())
                .map(|b| b.hero_name.clone());
        if let Some(hero) = hero {
            for b in
                d.vm.builds
                    .library
                    .iter_mut()
                    .filter(|b| b.hero_name == hero)
            {
                b.active = b.build_id == id.as_str();
            }
        }
        let i18n = d.i18n.clone();
        ui_map::apply_builds(&u, &d.vm.builds, &i18n);
    }));

    state.on_search_community(with!(ui, demo, |u, d, hero| {
        let hero_name =
            d.vm.builds
                .heroes
                .get(hero as usize)
                .map(|h| h.name.clone())
                .unwrap_or_default();
        d.vm.builds.community = (1..=3)
            .map(|n| CommunityBuildVm {
                name: format!("Community Build {n}"),
                hero_name: hero_name.clone(),
                usage: 4000 / n,
                sections: demo_sections(n * 3, 9, &["Weapon core", "Survive", "Spirit"]),
            })
            .collect();
        d.vm.builds.community_state = DataState::Loaded;
        let i18n = d.i18n.clone();
        ui_map::apply_builds(&u, &d.vm.builds, &i18n);
    }));

    state.on_import_community(with!(ui, demo, |u, d, i| {
        let Some(c) = usize::try_from(i)
            .ok()
            .and_then(|i| d.vm.builds.community.get(i))
            .cloned()
        else {
            return;
        };
        d.saved += 1;
        d.vm.builds.library.push(BuildVm {
            build_id: format!("imported-{}", d.saved),
            name: c.name,
            hero_name: c.hero_name,
            source: BuildSource::InGame,
            item_count: c.sections.iter().map(|s| s.items.len() as u32).sum(),
            sections: c.sections,
            active: false,
            items_removed: 0,
        });
        u.global::<AppState>().set_builds_tab(BuildsTab::Library);
        let i18n = d.i18n.clone();
        ui_map::apply_builds(&u, &d.vm.builds, &i18n);
    }));

    state.on_builder_search(with!(ui, demo, |u, d, query| {
        let q = query.to_lowercase();
        d.found = if q.is_empty() {
            vec![]
        } else {
            d.items
                .iter()
                .filter(|i| i.name.to_lowercase().contains(&q))
                .take(8)
                .cloned()
                .collect()
        };
        let chips: Vec<_> = d
            .found
            .iter()
            .map(|i| ui_map::map_option_chip(&d.i18n, i))
            .collect();
        u.global::<AppState>()
            .set_builder_results(ModelRc::new(VecModel::from(chips)));
    }));

    state.on_builder_add_section(with!(ui, demo, |u, d| {
        d.draft.add_section();
        let i18n = d.i18n.clone();
        ui_map::apply_builder(&u, &d.draft, None, &i18n);
    }));

    state.on_builder_rename_section(with!(ui, demo, |u, d, section, title| {
        // No redraw: the text box already shows what was typed.
        let _ = d.draft.rename_section(section as usize, &title);
    }));

    state.on_builder_add_item(with!(ui, demo, |u, d, i| {
        let section = u.global::<AppState>().get_builder_target() as usize;
        let error = d
            .found
            .get(i as usize)
            .cloned()
            .and_then(|item| d.draft.add_item(section, item).err());
        let i18n = d.i18n.clone();
        ui_map::apply_builder(&u, &d.draft, error, &i18n);
    }));

    state.on_builder_move_item(with!(ui, demo, |u, d, section, index, delta| {
        d.draft.move_item(section as usize, index as usize, delta);
        let i18n = d.i18n.clone();
        ui_map::apply_builder(&u, &d.draft, None, &i18n);
    }));

    state.on_builder_remove_item(with!(ui, demo, |u, d, section, index| {
        d.draft.remove_item(section as usize, index as usize);
        let i18n = d.i18n.clone();
        ui_map::apply_builder(&u, &d.draft, None, &i18n);
    }));

    state.on_builder_save(with!(ui, demo, |u, d| {
        let s = u.global::<AppState>();
        d.draft.hero =
            d.vm.builds
                .heroes
                .get(s.get_builder_hero() as usize)
                .cloned();
        d.draft.name = s.get_builder_name().to_string();
        d.draft.set_ability_order(s.get_builder_ability().as_str());
        let i18n = d.i18n.clone();
        match d.draft.validate() {
            Err(e) => ui_map::apply_builder(&u, &d.draft, Some(e), &i18n),
            Ok(()) => {
                d.saved += 1;
                let hero = d.draft.hero.clone().expect("validated");
                d.vm.builds.library.push(BuildVm {
                    build_id: format!("manual-{}", d.saved),
                    name: d.draft.name.clone(),
                    hero_name: hero.name,
                    source: BuildSource::Manual,
                    item_count: d.draft.item_count(),
                    sections: d
                        .draft
                        .sections
                        .iter()
                        .filter(|s| !s.items.is_empty())
                        .map(|s| SectionVm {
                            title: s.title.clone(),
                            items: s
                                .items
                                .iter()
                                .map(|i| ItemVm {
                                    name: i.name.clone(),
                                    icon: None,
                                    category: i.category,
                                })
                                .collect(),
                        })
                        .collect(),
                    active: false,
                    items_removed: 0,
                });
                d.draft = BuilderDraft::new();
                s.set_builder_name(SharedString::new());
                s.set_builder_ability(SharedString::new());
                s.set_builder_results(ModelRc::default());
                s.set_builds_tab(BuildsTab::Library);
                ui_map::apply_builder(&u, &d.draft, None::<BuilderError>, &i18n);
                ui_map::apply_builds(&u, &d.vm.builds, &i18n);
            }
        }
    }));

    state.on_save_account_id(with!(ui, demo, |u, d, text| {
        let s = u.global::<AppState>();
        match text.trim().parse::<u32>() {
            Ok(id) if id > 0 => {
                d.vm.settings.account_id = Some(id);
                if let Some(c) =
                    d.vm.home
                        .checks
                        .iter_mut()
                        .find(|c| c.id == CheckId::AccountId)
                {
                    c.state = CheckState::Pass;
                    c.value = Some(id.to_string());
                }
                s.set_account_id_error(SharedString::new());
            }
            _ => s.set_account_id_error(d.i18n.t("settings-account-invalid").into()),
        }
        let i18n = d.i18n.clone();
        ui_map::apply_home(&u, &d.vm.home, &i18n);
    }));

    state.on_set_diag_enabled(with!(ui, demo, |u, d, on| {
        d.save_env.diag.set_enabled(on);
        d.vm.settings.diag_enabled = on;
        let i18n = d.i18n.clone();
        ui_map::apply_settings(&u, &d.vm.settings, &i18n);
    }));

    state.on_open_diag_save(with!(ui, demo, |u, d| {
        let i18n = d.i18n.clone();
        d.vm.settings.diag_open = true;
        d.vm.settings.diag_preview = diag::preview_lines(&i18n, &d.save_env.diag.summary());
        d.vm.settings.diag_result = SaveResult::None;
        ui_map::apply_settings(&u, &d.vm.settings, &i18n);
    }));

    state.on_close_diag_save(with!(ui, demo, |u, d| {
        d.vm.settings.diag_open = false;
        let i18n = d.i18n.clone();
        ui_map::apply_settings(&u, &d.vm.settings, &i18n);
    }));

    state.on_save_diag(with!(ui, demo, |u, d, note, include_id| {
        d.save_env.diag.usage("save-diagnostics");
        d.vm.settings.diag_result = d.save_env.save(note.as_str(), include_id).into();
        let i18n = d.i18n.clone();
        ui_map::apply_settings(&u, &d.vm.settings, &i18n);
    }));

    state.on_open_diag_folder(with!(ui, demo, |u, d| {
        // Not in tests: it would open a window on the developer's desktop.
        if !cfg!(test) {
            diag::open_folder(&d.save_env.out_dir);
        }
    }));

    state.on_mark_problem(with!(ui, demo, |u, d, note| {
        d.save_env.diag.marker(note.as_str());
        let text = d
            .i18n
            .t_args("mark-confirmed", &[("time", diag::clock_utc().into())]);
        u.global::<AppState>().set_mark_confirm(text.into());
    }));

    state.on_view_opened(with!(ui, demo, |u, d, view| {
        d.save_env.diag.usage(diag::view_key(view));
    }));

    state.on_set_text_scale(with!(ui, demo, |u, d, pct| {
        u.global::<AppState>()
            .set_text_scale_pct(pct.clamp(75, 200));
    }));
}

pub fn initial_apply(ui: &MainWindow, demo: &Demo) {
    apply_all(ui, &demo.vm, &demo.i18n);
    ui_map::apply_builder(ui, &demo.draft, None, &demo.i18n);
}

#[cfg(test)]
mod tests {
    use super::*;

    // TST-108 (the part the app owns): the record from a played match saves as one file, and the
    // leak scan refuses forbidden data. The scan and the zip are `stalemate_core::diagnostics`.
    #[test]
    fn demo_diagnostics_export_and_the_leak_scan_blocks_other_players() {
        let demo = Demo::new(Arc::new(I18n::en_us()));
        let env = &demo.save_env;
        let path = env.save("it went fine", false).expect("clean data saves");
        assert!(path.is_file());
        assert!(path.starts_with(&env.out_dir));
        assert!(std::fs::metadata(&path).unwrap().len() > 100);

        // The preview counts what was recorded.
        let s = env.diag.summary();
        assert_eq!(
            (s.checks, s.matches, s.perf_samples, s.markers),
            (4, 1, 3, 1)
        );
        assert!(s.usage_actions >= 4);

        // Another player's name, the API key, and a home path in the note all stop the save.
        let before = std::fs::read_dir(&env.out_dir).unwrap().count();
        let _ = std::fs::remove_file(&path);
        for note in ["Player 7 was rude", "key demo-api-key-0123456789"] {
            match SaveResult::from(env.save(note, false)) {
                SaveResult::Leak(kind) => assert!(!kind.is_empty()),
                other => panic!("{note}: {other:?}"),
            }
        }
        assert_eq!(std::fs::read_dir(&env.out_dir).unwrap().count(), before - 1);
    }

    #[test]
    fn demo_label_tracks_the_view_on_screen() {
        let mut demo = Demo::new(Arc::new(I18n::en_us()));
        let loaded = demo.i18n.t("demo-state-loaded");
        let error = demo.i18n.t("demo-state-error");
        for _ in 0..4 {
            demo.cycle_state(1);
        }
        assert_eq!(demo.state_texts()[1], error);
        // Live was never touched, so its label stays loaded.
        assert_eq!(demo.state_texts()[2], loaded);
        // Retry on Scouting resets both the state and the label.
        demo.state_idx[0] = 0;
        demo.vm.scouting.state = DataState::Loaded;
        let texts = demo.state_texts();
        assert_eq!(texts[1], loaded);
        assert_eq!(texts[2], loaded);
        assert!(texts[0].is_empty() && texts[5].is_empty());
        demo.cycle_state(0);
        assert_eq!(demo.state_texts()[0], demo.i18n.t("demo-no-states"));
    }
}
