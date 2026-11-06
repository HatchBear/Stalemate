//! Demo data and a demo controller behind `--demo`. All names are made up ("Hero 1",
//! "Item A"). Nothing here is Valve data. The real wiring replaces this with core state.

use crate::builder::{BuilderDraft, BuilderError};
use crate::i18n::I18n;
use crate::ui::{AppState, BuildsTab, MainWindow};
use crate::ui_map::{self, apply_all};
use crate::view_model::*;
use slint::{ComponentHandle, ModelRc, SharedString, VecModel};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

const STATES: usize = 5;

fn named(prefix: &str, n: u32) -> NamedOption {
    NamedOption {
        id: n,
        name: format!("{prefix} {n}"),
    }
}

fn item_name(i: u32) -> String {
    let letter = (b'A' + (i % 26) as u8) as char;
    format!("Item {letter}")
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
            player_name: format!("Player {h}"),
            items: (0..(h % 4 + 2))
                .map(|i| ItemVm {
                    name: item_name(h + i),
                    icon: None,
                })
                .collect(),
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
        action,
    })
    .collect();

    let list = vec![
        ReviewListItemVm {
            match_id: 1,
            won: true,
            hero_name: "Hero 1".into(),
            duration_secs: 34 * 60 + 12,
            ended_ago_secs: Some(3 * 3600),
        },
        ReviewListItemVm {
            match_id: 2,
            won: false,
            hero_name: "Hero 4".into(),
            duration_secs: 41 * 60 + 5,
            ended_ago_secs: Some(26 * 3600),
        },
        ReviewListItemVm {
            match_id: 3,
            won: false,
            hero_name: "Hero 2".into(),
            duration_secs: 7 * 60 + 40,
            ended_ago_secs: Some(3 * 86_400),
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
                won: true,
                hero_name: "Hero 1".into(),
                duration_secs: 34 * 60 + 12,
                ended_ago_secs: Some(3 * 3600),
            }),
            setup_result: CheckState::Pending,
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
                    active: true,
                    items_removed: 0,
                },
                BuildVm {
                    build_id: "b2".into(),
                    name: "Build 2".into(),
                    hero_name: "Hero 1".into(),
                    source: BuildSource::Manual,
                    item_count: 11,
                    active: false,
                    items_removed: 2,
                },
                BuildVm {
                    build_id: "b3".into(),
                    name: "Build 3".into(),
                    hero_name: "Hero 2".into(),
                    source: BuildSource::External,
                    item_count: 1,
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
            version: env!("CARGO_PKG_VERSION").into(),
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
    let report = |id, won, hero: &str, secs, lead, s| ReviewReportVm {
        match_id: id,
        won,
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
                true,
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
                false,
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
                false,
                "Hero 2",
                7 * 60 + 40,
                None,
                sources(2_100, 800, 100, 0, 0),
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

pub struct Demo {
    pub vm: AppVm,
    i18n: Arc<I18n>,
    draft: BuilderDraft,
    items: Vec<NamedOption>,
    found: Vec<NamedOption>,
    reports: HashMap<u64, ReviewReportVm>,
    state_idx: [usize; 4],
    check_attempts: u32,
    saved: u32,
}

impl Demo {
    pub fn new(i18n: Arc<I18n>) -> Self {
        Self {
            vm: demo_vm(),
            i18n,
            draft: BuilderDraft::new(),
            items: (0..26)
                .map(|n| NamedOption {
                    id: n,
                    name: item_name(n),
                })
                .collect(),
            found: vec![],
            reports: demo_reports(),
            state_idx: [0; 4],
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
    fn cycle_state(&mut self, view: i32) -> String {
        if !(1..=4).contains(&view) {
            return self.i18n.t("demo-no-states");
        }
        let slot = (view - 1) as usize;
        self.state_idx[slot] = (self.state_idx[slot] + 1) % STATES;
        let state = demo_state(self.state_idx[slot]);
        *self.state_slot(view).expect("view in range") = state;
        self.state_text(slot)
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
    }));

    state.on_demo_cycle_state(with!(ui, demo, |u, d| {
        let view = u.global::<AppState>().get_current_view();
        let text = d.cycle_state(view);
        let i18n = d.i18n.clone();
        apply_all(&u, &d.vm, &i18n);
        u.global::<AppState>().set_demo_state_text(text.into());
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
            item_count: 12,
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
        let names: Vec<SharedString> = d.found.iter().map(|i| i.name.as_str().into()).collect();
        u.global::<AppState>()
            .set_builder_results(ModelRc::new(VecModel::from(names)));
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

    state.on_set_text_scale(with!(ui, demo, |u, d, pct| {
        u.global::<AppState>()
            .set_text_scale_pct(pct.clamp(75, 200));
    }));
}

pub fn initial_apply(ui: &MainWindow, demo: &Demo) {
    apply_all(ui, &demo.vm, &demo.i18n);
    ui_map::apply_builder(ui, &demo.draft, None, &demo.i18n);
}
