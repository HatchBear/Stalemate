//! Maps view models to Slint structs and formats every display string through Fluent.
//!
//! No hero, item or rank names live here: they arrive in the view models as data.

use crate::builder::{BuilderDraft, BuilderError};
use crate::i18n::I18n;
use crate::ui::{
    AppState, BuildRowUi, CardKind, Check, ChipRowUi, CommunityRowUi, DataKind, DiagResult,
    EnemyRowUi, ItemCategory as UiCategory, ItemChipUi, LastMatchUi, LogAction, LogRowUi,
    MainHeroUi, MainWindow, MatchOutcome, ReportUi, ReviewRowUi, ScoutCardUi, SectionUi, SoulKind,
    SoulRowUi, StateUi, TeamSide,
};
use crate::view_model::*;
use fluent_bundle::FluentValue;
use slint::{ComponentHandle, Image, ModelRc, SharedString, VecModel};

/// Keys chosen from a table or an enum rather than written at the call site, so the
/// "every key exists" test lists them here. `keys_for_all_variants` checks the tables agree.
#[cfg(test)]
pub const DYNAMIC_KEYS: &[&str] = &[
    "check-steam",
    "check-condebug",
    "check-account-id",
    "check-broadcast",
    "check-deadlock-api",
    "check-match-watcher",
    "check-steam-fail",
    "check-condebug-fail",
    "check-account-id-fail",
    "check-broadcast-fail",
    "check-deadlock-api-fail",
    "check-match-watcher-fail",
    "phase-idle",
    "phase-match-detected",
    "phase-live",
    "phase-live-no-broadcast",
    "phase-awaiting-end",
    "phase-reviewing",
    "phase-stopped",
    "category-weapon",
    "category-vitality",
    "category-spirit",
    "result-win",
    "result-loss",
    "result-none",
    "result-abandoned",
    "soul-other",
    "soul-lane-troopers",
    "soul-neutral-camps",
    "soul-breakables",
    "soul-urn",
    "soul-sinners-sacrifice",
    "log-buy",
    "log-sell",
    "log-upgrade",
    "build-source-ingame",
    "build-source-external",
    "build-source-manual",
    "builder-error-duplicate",
    "builder-error-no-section",
    "builder-error-no-hero",
    "builder-error-no-name",
    "builder-error-empty",
    "builder-error-save",
];

pub fn check_label_key(id: CheckId) -> &'static str {
    match id {
        CheckId::Steam => "check-steam",
        CheckId::Condebug => "check-condebug",
        CheckId::AccountId => "check-account-id",
        CheckId::Broadcast => "check-broadcast",
        CheckId::DeadlockApi => "check-deadlock-api",
        CheckId::MatchWatcher => "check-match-watcher",
    }
}

pub fn check_fail_key(id: CheckId) -> &'static str {
    match id {
        CheckId::Steam => "check-steam-fail",
        CheckId::Condebug => "check-condebug-fail",
        CheckId::AccountId => "check-account-id-fail",
        CheckId::Broadcast => "check-broadcast-fail",
        CheckId::DeadlockApi => "check-deadlock-api-fail",
        CheckId::MatchWatcher => "check-match-watcher-fail",
    }
}

pub fn soul_key(kind: SoulSourceKind) -> &'static str {
    match kind {
        SoulSourceKind::LaneTroopers => "soul-lane-troopers",
        SoulSourceKind::NeutralCamps => "soul-neutral-camps",
        SoulSourceKind::Breakables => "soul-breakables",
        SoulSourceKind::Urn => "soul-urn",
        SoulSourceKind::SinnersSacrifice => "soul-sinners-sacrifice",
        SoulSourceKind::Other => "soul-other",
    }
}

pub fn category_key(c: ItemCategory) -> &'static str {
    match c {
        ItemCategory::Weapon => "category-weapon",
        ItemCategory::Vitality => "category-vitality",
        ItemCategory::Spirit => "category-spirit",
    }
}

fn result_key(o: Outcome) -> &'static str {
    match o {
        Outcome::Win => "result-win",
        Outcome::Loss => "result-loss",
        Outcome::NoResult => "result-none",
        Outcome::Abandoned => "result-abandoned",
    }
}

pub fn action_key(action: PurchaseAction) -> &'static str {
    match action {
        PurchaseAction::Buy => "log-buy",
        PurchaseAction::Sell => "log-sell",
        PurchaseAction::Upgrade => "log-upgrade",
    }
}

pub fn build_source_key(source: BuildSource) -> &'static str {
    match source {
        BuildSource::InGame => "build-source-ingame",
        BuildSource::External => "build-source-external",
        BuildSource::Manual => "build-source-manual",
    }
}

pub fn builder_error_key(error: BuilderError) -> &'static str {
    match error {
        BuilderError::DuplicateItem => "builder-error-duplicate",
        BuilderError::NoSuchSection => "builder-error-no-section",
        BuilderError::NoHero => "builder-error-no-hero",
        BuilderError::NoName => "builder-error-no-name",
        BuilderError::Empty => "builder-error-empty",
        BuilderError::SaveFailed => "builder-error-save",
    }
}

// ---- small formatters -------------------------------------------------------------------

/// `m:ss`, or `h:mm:ss` from one hour.
pub fn clock(secs: u32) -> String {
    let (h, m, s) = (secs / 3600, secs / 60 % 60, secs % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// `12,345`, en-US grouping.
pub fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

fn signed(n: i64) -> String {
    if n > 0 {
        format!("+{}", thousands(n))
    } else {
        thousands(n)
    }
}

fn num(n: impl Into<i64>) -> FluentValue<'static> {
    FluentValue::from(n.into())
}

fn text(s: &str) -> FluentValue<'static> {
    FluentValue::from(s.to_owned())
}

pub fn ago(i18n: &I18n, secs: u64) -> String {
    match secs {
        0..=59 => i18n.t("age-now"),
        60..=3599 => i18n.t_n("age-minutes", "n", (secs / 60) as i64),
        3600..=86_399 => i18n.t_n("age-hours", "n", (secs / 3600) as i64),
        _ => i18n.t_n("age-days", "n", (secs / 86_400) as i64),
    }
}

fn matches_text(i18n: &I18n, n: u32) -> String {
    i18n.t_n("matches-count", "n", n.into())
}

fn record_text(i18n: &I18n, r: Record) -> String {
    match r.win_rate_pct() {
        None => i18n.t("record-none"),
        Some(pct) => i18n.t_args(
            "record-line",
            &[
                ("wins", num(r.wins)),
                ("losses", num(r.losses)),
                ("pct", num(pct)),
            ],
        ),
    }
}

// ---- state ------------------------------------------------------------------------------

pub fn map_state(i18n: &I18n, state: &DataState) -> StateUi {
    let (kind, message) = match state {
        DataState::Loaded => (DataKind::Loaded, String::new()),
        DataState::Loading => (DataKind::Loading, String::new()),
        DataState::Empty => (DataKind::Empty, String::new()),
        DataState::Stale { age_secs } => (
            DataKind::Stale,
            i18n.t_args("state-stale", &[("age", text(&ago(i18n, *age_secs)))]),
        ),
        DataState::Error { source } => (
            DataKind::Error,
            i18n.t_args("state-error", &[("source", text(source))]),
        ),
    };
    StateUi {
        kind,
        message: message.into(),
    }
}

fn check(state: CheckState) -> Check {
    match state {
        CheckState::Pending => Check::Pending,
        CheckState::Pass => Check::Pass,
        CheckState::Fail => Check::Fail,
    }
}

fn model<T: Clone + 'static>(items: Vec<T>) -> ModelRc<T> {
    ModelRc::new(VecModel::from(items))
}

// ---- home -------------------------------------------------------------------------------

pub fn map_check(i18n: &I18n, c: &SetupCheck) -> crate::ui::CheckUi {
    let detail = match c.state {
        CheckState::Fail => i18n.t(check_fail_key(c.id)),
        CheckState::Pass => c.value.clone().unwrap_or_default(),
        CheckState::Pending => String::new(),
    };
    crate::ui::CheckUi {
        label: i18n.t(check_label_key(c.id)).into(),
        detail: detail.into(),
        state: check(c.state),
    }
}

pub fn map_last_match(i18n: &I18n, m: &Option<MatchSummary>) -> LastMatchUi {
    let Some(m) = m else {
        return LastMatchUi {
            has_match: false,
            outcome: MatchOutcome::None,
            headline: SharedString::new(),
            detail: SharedString::new(),
        };
    };
    LastMatchUi {
        has_match: true,
        outcome: outcome_ui(m.outcome),
        headline: result_title(i18n, m.outcome, &m.hero_name).into(),
        detail: duration_and_ago(i18n, m.duration_secs, m.ended_ago_secs).into(),
    }
}

fn outcome_ui(o: Outcome) -> MatchOutcome {
    match o {
        Outcome::Win => MatchOutcome::Win,
        Outcome::Loss => MatchOutcome::Loss,
        Outcome::NoResult => MatchOutcome::None,
        Outcome::Abandoned => MatchOutcome::Abandoned,
    }
}

fn result_title(i18n: &I18n, outcome: Outcome, hero: &str) -> String {
    i18n.t_args(result_key(outcome), &[("hero", text(hero))])
}

fn duration_and_ago(i18n: &I18n, duration: u32, ended_ago: Option<u64>) -> String {
    match ended_ago {
        Some(secs) => i18n.t_args(
            "match-meta",
            &[
                ("duration", text(&clock(duration))),
                ("ago", text(&ago(i18n, secs))),
            ],
        ),
        None => i18n.t_args("match-meta-short", &[("duration", text(&clock(duration)))]),
    }
}

// ---- scouting ---------------------------------------------------------------------------

pub fn map_card(i18n: &I18n, c: &ScoutCardVm) -> ScoutCardUi {
    let mut ui = ScoutCardUi {
        kind: CardKind::HeroOnly,
        hero: c.hero_name.clone().into(),
        name: c.player_name.clone().into(),
        you: c.is_you,
        overall: SharedString::new(),
        last_label: SharedString::new(),
        last: SharedString::new(),
        rank: SharedString::new(),
        trend: SharedString::new(),
        on_hero: SharedString::new(),
        mains: ModelRc::default(),
    };
    match &c.body {
        CardBody::HeroOnly => {}
        CardBody::NoPublicData => ui.kind = CardKind::NoPublicData,
        CardBody::Loading => ui.kind = CardKind::Loading,
        CardBody::Unavailable => ui.kind = CardKind::Unavailable,
        CardBody::Stats(s) => {
            ui.kind = CardKind::Data;
            ui.overall = record_text(i18n, s.overall).into();
            let n = s.last.matches();
            ui.last_label = if n < s.last_window {
                i18n.t_args(
                    "stat-last-few",
                    &[("n", num(n)), ("window", num(s.last_window))],
                )
            } else {
                i18n.t_n("stat-last", "n", n.into())
            }
            .into();
            ui.last = record_text(i18n, s.last).into();
            ui.rank = s.rank.clone().unwrap_or_else(|| i18n.t("rank-none")).into();
            ui.trend = match s.mmr_trend_per_10 {
                None => i18n.t("trend-unavailable"),
                Some(0) => i18n.t("trend-flat"),
                Some(v) if v > 0 => i18n.t_n("trend-up", "n", v.into()),
                Some(v) => i18n.t_n("trend-down", "n", (-v).into()),
            }
            .into();
            ui.on_hero = matches_text(i18n, s.matches_on_hero).into();
            let mains: Vec<MainHeroUi> = s
                .mains
                .iter()
                .take(5)
                .map(|m| MainHeroUi {
                    hero: m.hero_name.clone().into(),
                    record: i18n
                        .t_args(
                            "main-record",
                            &[
                                ("matches", num(m.matches)),
                                (
                                    "pct",
                                    num(Record {
                                        wins: m.wins,
                                        losses: m.matches.saturating_sub(m.wins),
                                    }
                                    .win_rate_pct()
                                    .unwrap_or(0)),
                                ),
                            ],
                        )
                        .into(),
                })
                .collect();
            ui.mains = model(mains);
        }
    }
    ui
}

fn side(s: Side) -> TeamSide {
    match s {
        Side::Amber => TeamSide::Amber,
        Side::Sapphire => TeamSide::Sapphire,
    }
}

fn side_name(i18n: &I18n, s: Side) -> String {
    match s {
        Side::Amber => i18n.t("team-amber"),
        Side::Sapphire => i18n.t("team-sapphire"),
    }
}

// ---- live -------------------------------------------------------------------------------

fn ui_category(c: Option<ItemCategory>) -> UiCategory {
    match c {
        None => UiCategory::None,
        Some(ItemCategory::Weapon) => UiCategory::Weapon,
        Some(ItemCategory::Vitality) => UiCategory::Vitality,
        Some(ItemCategory::Spirit) => UiCategory::Spirit,
    }
}

/// An item as a chip: name, category colour and icon, and the category name for screen readers.
pub fn map_chip(
    i18n: &I18n,
    name: &str,
    category: Option<ItemCategory>,
    icon: Option<&std::path::PathBuf>,
) -> ItemChipUi {
    let art = icon.and_then(|p| Image::load_from_path(p).ok());
    ItemChipUi {
        name: name.into(),
        category: ui_category(category),
        category_label: category
            .map(|c| i18n.t(category_key(c)))
            .unwrap_or_default()
            .into(),
        has_icon: art.is_some(),
        icon: art.unwrap_or_default(),
    }
}

pub fn map_item(i18n: &I18n, i: &ItemVm) -> ItemChipUi {
    map_chip(i18n, &i.name, i.category, i.icon.as_ref())
}

pub fn map_option_chip(i18n: &I18n, o: &NamedOption) -> ItemChipUi {
    map_chip(i18n, &o.name, o.category, None)
}

/// How many items of each category a build has, as chips ("Weapon 5"). Items with no category
/// are left out. The order is fixed: Weapon, Vitality, Spirit.
pub fn category_mix(i18n: &I18n, sections: &[SectionVm]) -> Vec<ItemChipUi> {
    [
        ItemCategory::Weapon,
        ItemCategory::Vitality,
        ItemCategory::Spirit,
    ]
    .into_iter()
    .filter_map(|c| {
        let n = sections
            .iter()
            .flat_map(|s| &s.items)
            .filter(|i| i.category == Some(c))
            .count();
        (n > 0).then(|| {
            let label = i18n.t_args(
                "category-count",
                &[
                    ("category", text(&i18n.t(category_key(c)))),
                    ("n", num(n as i64)),
                ],
            );
            map_chip(i18n, &label, Some(c), None)
        })
    })
    .collect()
}

/// Chips per row in a build's sections and in a Live player row. Slint has no wrapping layout, so
/// the rows are cut here; the chips in a row share the width equally. A layout choice, not a rule.
pub const BUILD_CHIPS_PER_ROW: usize = 6;
pub const LIVE_CHIPS_PER_ROW: usize = 5;

pub fn chip_rows(chips: Vec<ItemChipUi>, per_row: usize) -> Vec<ChipRowUi> {
    chips
        .chunks(per_row.max(1))
        .map(|c| ChipRowUi {
            chips: model(c.to_vec()),
        })
        .collect()
}

pub fn map_section(i18n: &I18n, s: &SectionVm) -> SectionUi {
    let chips: Vec<ItemChipUi> = s.items.iter().map(|i| map_item(i18n, i)).collect();
    SectionUi {
        title: s.title.clone().into(),
        rows: model(chip_rows(chips.clone(), BUILD_CHIPS_PER_ROW)),
        items: model(chips),
    }
}

pub fn map_enemy(i18n: &I18n, e: &EnemyRowVm) -> EnemyRowUi {
    let art = e
        .portrait
        .as_ref()
        .and_then(|p| Image::load_from_path(p).ok());
    EnemyRowUi {
        hero: e.hero_name.clone().into(),
        initial: e
            .hero_name
            .chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            .into(),
        has_portrait: art.is_some(),
        portrait: art.unwrap_or_default(),
        name: e.player_name.clone().into(),
        side: side(e.team),
        side_label: side_name(i18n, e.team).into(),
        item_rows: model(chip_rows(
            e.items.iter().map(|i| map_item(i18n, i)).collect(),
            LIVE_CHIPS_PER_ROW,
        )),
        has_items: !e.items.is_empty(),
        items_none: i18n.t("items-none").into(),
        kda: e
            .kda
            .map(|k| format!("{}/{}/{}", k.kills, k.deaths, k.assists))
            .unwrap_or_else(|| i18n.t("value-missing"))
            .into(),
        level: e
            .level
            .map(|l| l.to_string())
            .unwrap_or_else(|| i18n.t("value-missing"))
            .into(),
        souls: e
            .souls
            .map(|s| thousands(s.into()))
            .unwrap_or_else(|| i18n.t("value-missing"))
            .into(),
    }
}

pub fn map_log(i18n: &I18n, p: &PurchaseEntryVm) -> LogRowUi {
    LogRowUi {
        time: clock(p.t_game_secs).into(),
        player: p.player_name.clone().into(),
        item: map_chip(i18n, &p.item_name, p.category, None),
        action: match p.action {
            PurchaseAction::Buy => LogAction::Buy,
            PurchaseAction::Sell => LogAction::Sell,
            PurchaseAction::Upgrade => LogAction::Upgrade,
        },
        action_label: i18n.t(action_key(p.action)).into(),
    }
}

// ---- reviews ----------------------------------------------------------------------------

pub fn map_review_row(i18n: &I18n, r: &ReviewListItemVm) -> ReviewRowUi {
    ReviewRowUi {
        title: result_title(i18n, r.outcome, &r.hero_name).into(),
        outcome: outcome_ui(r.outcome),
        pending: r.pending,
        meta: if r.pending {
            i18n.t("review-pending-meta")
        } else {
            duration_and_ago(i18n, r.duration_secs, r.ended_ago_secs)
        }
        .into(),
    }
}

/// Stats-only report with the CD-08 coach voice. Every number comes from a placeable.
pub fn map_report(i18n: &I18n, r: &Option<ReviewReportVm>) -> ReportUi {
    let Some(r) = r else {
        return ReportUi {
            has_report: false,
            outcome: MatchOutcome::None,
            headline: SharedString::new(),
            duration: SharedString::new(),
            lane_title: SharedString::new(),
            lane_lead: SharedString::new(),
            lane_line: SharedString::new(),
            summary_line: SharedString::new(),
            sources: ModelRc::default(),
        };
    };
    let minutes = (r.duration_secs / 60) as i64;
    let result_line = i18n.t_n(
        match r.outcome {
            Outcome::Win => "report-summary-win",
            Outcome::Loss => "report-summary-loss",
            Outcome::NoResult => "report-summary-none",
            Outcome::Abandoned => "report-summary-abandoned",
        },
        "minutes",
        minutes,
    );
    let total: u32 = r.soul_sources.iter().map(|s| s.souls).sum();
    let top = r.soul_sources.iter().max_by_key(|s| s.souls);
    let summary = match top {
        Some(top) if total > 0 => format!(
            "{} {}",
            result_line,
            i18n.t_args(
                "report-top-source",
                &[("source", text(&i18n.t(soul_key(top.kind)).to_lowercase()))]
            )
        ),
        _ => result_line,
    };
    let time = clock(r.lane_check_secs);
    let (lane_lead, lane_line) = match r.lane_soul_lead {
        None => (
            i18n.t("report-lane-unavailable"),
            i18n.t_args("report-lane-unavailable-line", &[("time", text(&time))]),
        ),
        Some(lead) => {
            let souls = thousands(lead.unsigned_abs().into());
            let key = match lead {
                1.. => "report-lane-ahead",
                0 => "report-lane-even",
                _ => "report-lane-behind",
            };
            (
                i18n.t_args(
                    "report-lane-value",
                    &[("souls", text(&signed(lead.into())))],
                ),
                i18n.t_args(key, &[("souls", text(&souls)), ("time", text(&time))]),
            )
        }
    };
    let sources: Vec<SoulRowUi> = r
        .soul_sources
        .iter()
        .map(|s| {
            let pct = if total > 0 {
                (u64::from(s.souls) * 100 + u64::from(total) / 2) / u64::from(total)
            } else {
                0
            };
            SoulRowUi {
                kind: match s.kind {
                    SoulSourceKind::LaneTroopers => SoulKind::LaneTroopers,
                    SoulSourceKind::NeutralCamps => SoulKind::NeutralCamps,
                    SoulSourceKind::Breakables => SoulKind::Breakables,
                    SoulSourceKind::Urn => SoulKind::Urn,
                    SoulSourceKind::SinnersSacrifice => SoulKind::SinnersSacrifice,
                    SoulSourceKind::Other => SoulKind::Other,
                },
                label: i18n.t(soul_key(s.kind)).into(),
                souls: i18n
                    .t_args(
                        "soul-amount",
                        &[
                            ("souls", text(&thousands(s.souls.into()))),
                            ("pct", num(pct as i64)),
                        ],
                    )
                    .into(),
            }
        })
        .collect();
    ReportUi {
        has_report: true,
        outcome: outcome_ui(r.outcome),
        headline: result_title(i18n, r.outcome, &r.hero_name).into(),
        duration: i18n
            .t_args(
                "report-duration",
                &[("duration", text(&clock(r.duration_secs)))],
            )
            .into(),
        lane_title: i18n
            .t_args("report-lane-title", &[("time", text(&time))])
            .into(),
        lane_lead: lane_lead.into(),
        lane_line: lane_line.into(),
        summary_line: summary.into(),
        sources: model(sources),
    }
}

// ---- builds -----------------------------------------------------------------------------

pub fn map_build(i18n: &I18n, b: &BuildVm) -> BuildRowUi {
    BuildRowUi {
        id: b.build_id.clone().into(),
        name: b.name.clone().into(),
        hero: b.hero_name.clone().into(),
        meta: i18n
            .t_args(
                "build-meta",
                &[
                    ("source", text(&i18n.t(build_source_key(b.source)))),
                    ("count", num(b.item_count)),
                ],
            )
            .into(),
        active: b.active,
        flag: if b.items_removed > 0 {
            i18n.t_n("build-needs-update", "n", b.items_removed.into())
        } else {
            String::new()
        }
        .into(),
        mix: model(category_mix(i18n, &b.sections)),
        sections: model(b.sections.iter().map(|s| map_section(i18n, s)).collect()),
    }
}

pub fn map_community(i18n: &I18n, c: &CommunityBuildVm) -> CommunityRowUi {
    CommunityRowUi {
        name: c.name.clone().into(),
        hero: c.hero_name.clone().into(),
        usage: i18n.t_n("community-usage", "n", c.usage.into()).into(),
        mix: model(category_mix(i18n, &c.sections)),
        sections: model(c.sections.iter().map(|s| map_section(i18n, s)).collect()),
    }
}

pub fn map_sections(i18n: &I18n, draft: &BuilderDraft) -> Vec<SectionUi> {
    draft
        .sections
        .iter()
        .map(|s| SectionUi {
            title: s.title.clone().into(),
            items: model(s.items.iter().map(|i| map_option_chip(i18n, i)).collect()),
            rows: ModelRc::default(),
        })
        .collect()
}

// ---- apply to the window ----------------------------------------------------------------

/// Installs the Fluent lookup as `Tr.t`.
pub fn install_strings(ui: &MainWindow, i18n: std::sync::Arc<I18n>) {
    let tr = ui.global::<crate::ui::Tr>();
    tr.on_t(move |key| i18n.t(key.as_str()).into());
    // Display headings are uppercase. Slint has no text-transform, so Rust does it.
    tr.on_upper(|text| text.to_uppercase().into());
    tr.on_initial(|text| {
        text.chars()
            .next()
            .map(|c| c.to_uppercase().collect::<String>())
            .unwrap_or_default()
            .into()
    });
}

pub fn phase_key(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "phase-idle",
        Phase::MatchDetected => "phase-match-detected",
        Phase::Live => "phase-live",
        Phase::LiveNoBroadcast => "phase-live-no-broadcast",
        Phase::AwaitingBroadcastEnd => "phase-awaiting-end",
        Phase::Reviewing => "phase-reviewing",
        Phase::Stopped => "phase-stopped",
    }
}

pub fn apply_home(ui: &MainWindow, vm: &HomeVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_phase_text(i18n.t(phase_key(vm.phase)).into());
    s.set_checks(model(
        vm.checks.iter().map(|c| map_check(i18n, c)).collect(),
    ));
    s.set_last_match(map_last_match(i18n, &vm.last_match));
    s.set_setup_result(check(vm.setup_result));
    s.set_setup_result_detail(
        match vm.setup_result {
            CheckState::Fail => i18n.t("setup-fail-hint"),
            CheckState::Pass => i18n.t("setup-pass-hint"),
            CheckState::Pending => String::new(),
        }
        .into(),
    );
}

pub fn apply_scouting(ui: &MainWindow, vm: &ScoutingVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_scouting_visible(vm.visible);
    s.set_scouting_state(map_state(i18n, &vm.state));
    s.set_your_side(side(vm.your_side));
    s.set_your_team_heading(
        i18n.t_args(
            "team-heading-yours",
            &[("team", text(&side_name(i18n, vm.your_side)))],
        )
        .into(),
    );
    s.set_enemy_team_heading(
        i18n.t_args(
            "team-heading-enemy",
            &[("team", text(&side_name(i18n, vm.your_side.other())))],
        )
        .into(),
    );
    s.set_your_cards(model(
        vm.your_team.iter().map(|c| map_card(i18n, c)).collect(),
    ));
    s.set_enemy_cards(model(
        vm.enemy_team.iter().map(|c| map_card(i18n, c)).collect(),
    ));
}

pub fn apply_live(ui: &MainWindow, vm: &LiveVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_live_state(map_state(i18n, &vm.state));
    s.set_delay_known(vm.broadcast_delay_secs.is_some());
    s.set_delay_text(
        vm.broadcast_delay_secs
            .map(|d| i18n.t_n("live-delay-value", "secs", d.into()))
            .unwrap_or_default()
            .into(),
    );
    s.set_delay_stale(vm.delay_stale);
    s.set_show_kda(vm.show_kda());
    s.set_show_level(vm.show_level());
    s.set_show_souls(vm.show_souls());
    s.set_enemy_board_heading(
        vm.enemies
            .first()
            .map(|e| {
                i18n.t_args(
                    "live-board-heading",
                    &[("team", text(&side_name(i18n, e.team)))],
                )
            })
            .unwrap_or_default()
            .into(),
    );
    s.set_enemy_rows(model(
        vm.enemies.iter().map(|e| map_enemy(i18n, e)).collect(),
    ));
    s.set_log_rows(model(
        vm.purchase_log.iter().map(|p| map_log(i18n, p)).collect(),
    ));
}

pub fn apply_reviews(ui: &MainWindow, vm: &ReviewsVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_reviews_state(map_state(i18n, &vm.state));
    s.set_review_rows(model(
        vm.list.iter().map(|r| map_review_row(i18n, r)).collect(),
    ));
    s.set_review_count_text(i18n.t_n("reviews-count", "n", vm.list.len() as i64).into());
    s.set_selected_review(vm.selected.map_or(-1, |i| i as i32));
    s.set_report(map_report(i18n, &vm.report));
    s.set_selected_pending(
        vm.selected
            .and_then(|i| vm.list.get(i))
            .is_some_and(|r| r.pending),
    );
}

pub fn apply_builds(ui: &MainWindow, vm: &BuildsVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_builds_state(map_state(i18n, &vm.state));
    s.set_library(model(
        vm.library.iter().map(|b| map_build(i18n, b)).collect(),
    ));
    s.set_library_count_text(
        i18n.t_n("builds-count", "n", vm.library.len() as i64)
            .into(),
    );
    s.set_heroes(model(
        vm.heroes
            .iter()
            .map(|h| SharedString::from(h.name.as_str()))
            .collect(),
    ));
    s.set_community_state(map_state(i18n, &vm.community_state));
    s.set_community_rows(model(
        vm.community
            .iter()
            .map(|c| map_community(i18n, c))
            .collect(),
    ));
}

pub fn apply_builder(
    ui: &MainWindow,
    draft: &BuilderDraft,
    error: Option<BuilderError>,
    i18n: &I18n,
) {
    let s = ui.global::<AppState>();
    s.set_builder_sections(model(map_sections(i18n, draft)));
    s.set_builder_error(
        error
            .map(|e| i18n.t(builder_error_key(e)))
            .unwrap_or_default()
            .into(),
    );
}

pub fn apply_settings(ui: &MainWindow, vm: &SettingsVm, i18n: &I18n) {
    let s = ui.global::<AppState>();
    s.set_account_id(
        vm.account_id
            .map(|a| a.to_string())
            .unwrap_or_default()
            .into(),
    );
    s.set_account_id_error(if vm.account_invalid {
        i18n.t("settings-account-invalid").into()
    } else {
        SharedString::new()
    });
    s.set_version_text(
        i18n.t_args("about-version", &[("version", text(&vm.version))])
            .into(),
    );
    s.set_diag_enabled(vm.diag_enabled);
    s.set_diag_save_open(vm.diag_open);
    s.set_diag_preview(model(
        vm.diag_preview
            .iter()
            .map(|l| SharedString::from(l.as_str()))
            .collect(),
    ));
    let (kind, result_text, folder) = match &vm.diag_result {
        crate::diag::SaveResult::None => (DiagResult::None, String::new(), String::new()),
        crate::diag::SaveResult::Saved(path) => (
            DiagResult::Saved,
            i18n.t_args("diag-saved", &[("path", text(&path.display().to_string()))]),
            path.parent()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        ),
        crate::diag::SaveResult::Leak(kind) => (
            DiagResult::Leak,
            i18n.t_args("diag-leak", &[("kind", text(kind))]),
            String::new(),
        ),
        crate::diag::SaveResult::Failed => {
            (DiagResult::Failed, i18n.t("diag-failed"), String::new())
        }
    };
    s.set_diag_result(kind);
    s.set_diag_result_text(result_text.into());
    s.set_diag_folder(folder.into());
}

pub fn apply_all(ui: &MainWindow, vm: &AppVm, i18n: &I18n) {
    ui.global::<AppState>().set_offline(vm.offline);
    apply_home(ui, &vm.home, i18n);
    apply_scouting(ui, &vm.scouting, i18n);
    apply_live(ui, &vm.live, i18n);
    apply_reviews(ui, &vm.reviews, i18n);
    apply_builds(ui, &vm.builds, i18n);
    apply_settings(ui, &vm.settings, i18n);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn i18n() -> I18n {
        I18n::en_us()
    }

    fn stats(last_matches: (u32, u32)) -> ScoutStats {
        ScoutStats {
            overall: Record {
                wins: 55,
                losses: 45,
            },
            last: Record {
                wins: last_matches.0,
                losses: last_matches.1,
            },
            last_window: 20,
            rank: Some("Rank X".into()),
            mmr_trend_per_10: None,
            mains: (1..=7)
                .map(|i| MainHero {
                    hero_name: format!("Hero {i}"),
                    matches: 10 * i,
                    wins: 5 * i,
                })
                .collect(),
            matches_on_hero: 1,
        }
    }

    fn card(body: CardBody) -> ScoutCardVm {
        ScoutCardVm {
            hero_name: "Hero 1".into(),
            player_name: "Player 1".into(),
            is_you: false,
            body,
        }
    }

    #[test]
    fn clock_and_thousands() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(9 * 60), "9:00");
        assert_eq!(clock(3725), "1:02:05");
        assert_eq!(thousands(1234567), "1,234,567");
        assert_eq!(thousands(-1500), "-1,500");
        assert_eq!(signed(300), "+300");
    }

    #[test]
    fn card_with_full_window_is_labeled_20_and_trend_is_unavailable() {
        let ui = map_card(&i18n(), &card(CardBody::Stats(stats((12, 8)))));
        assert_eq!(ui.kind, CardKind::Data);
        assert_eq!(ui.overall, "55 W / 45 L (55%)");
        assert_eq!(ui.last_label, "Last 20 matches");
        assert_eq!(ui.last, "12 W / 8 L (60%)");
        assert_eq!(ui.trend, "Not available yet");
        assert_eq!(ui.rank, "Rank X");
        assert_eq!(ui.on_hero, "1 match");
    }

    #[test]
    fn card_with_fewer_than_20_matches_shows_n() {
        let ui = map_card(&i18n(), &card(CardBody::Stats(stats((3, 4)))));
        assert!(ui.last_label.contains('7'), "{}", ui.last_label);
        assert!(ui.last_label.contains("20"), "{}", ui.last_label);
    }

    #[test]
    fn mains_are_capped_at_five() {
        use slint::Model;
        let ui = map_card(&i18n(), &card(CardBody::Stats(stats((10, 10)))));
        assert_eq!(ui.mains.row_count(), 5);
        assert_eq!(ui.mains.row_data(0).unwrap().record, "10 matches, 50%");
    }

    #[test]
    fn opt_out_and_bot_cards_carry_no_stats() {
        let ui = map_card(&i18n(), &card(CardBody::NoPublicData));
        assert_eq!(ui.kind, CardKind::NoPublicData);
        assert!(ui.overall.is_empty());
        let ui = map_card(&i18n(), &card(CardBody::HeroOnly));
        assert_eq!(ui.kind, CardKind::HeroOnly);
        assert_eq!(ui.hero, "Hero 1");
    }

    #[test]
    fn data_states_map_with_age_and_source() {
        let i = i18n();
        let ui = map_state(&i, &DataState::Stale { age_secs: 600 });
        assert_eq!(ui.kind, DataKind::Stale);
        assert!(ui.message.contains("10 min"), "{}", ui.message);
        let ui = map_state(
            &i,
            &DataState::Error {
                source: "deadlock-api".into(),
            },
        );
        assert_eq!(ui.kind, DataKind::Error);
        assert!(ui.message.contains("deadlock-api"));
        assert_eq!(map_state(&i, &DataState::Loading).kind, DataKind::Loading);
        assert_eq!(map_state(&i, &DataState::Empty).kind, DataKind::Empty);
    }

    #[test]
    fn failed_check_gets_hint_text_and_state() {
        let ui = map_check(
            &i18n(),
            &SetupCheck {
                id: CheckId::Steam,
                state: CheckState::Fail,
                value: None,
            },
        );
        assert_eq!(ui.state, Check::Fail);
        assert!(!ui.detail.is_empty());
        assert_ne!(ui.detail, check_fail_key(CheckId::Steam));
    }

    #[test]
    fn enemy_row_shows_dash_for_missing_values_and_keeps_item_categories() {
        use slint::Model;
        let row = EnemyRowVm {
            hero_name: "Hero 1".into(),
            portrait: None,
            player_name: "Player 1".into(),
            team: Side::Sapphire,
            souls: Some(12_345),
            items: vec![
                ItemVm {
                    name: "Item A".into(),
                    icon: None,
                    category: Some(ItemCategory::Weapon),
                },
                ItemVm {
                    name: "Item B".into(),
                    icon: None,
                    category: None,
                },
            ],
            kda: None,
            level: Some(7),
        };
        let ui = map_enemy(&i18n(), &row);
        assert!(ui.has_items);
        // Three chips to a row in the Live list: two items make one row.
        assert_eq!(ui.item_rows.row_count(), 1);
        let first = ui.item_rows.row_data(0).unwrap();
        assert_eq!(first.chips.row_count(), 2);
        let a = first.chips.row_data(0).unwrap();
        assert_eq!(a.category, UiCategory::Weapon);
        assert_eq!(a.category_label, "Weapon", "the category name is text too");
        assert_eq!(first.chips.row_data(1).unwrap().category, UiCategory::None);
        assert_eq!(ui.kda, "-");
        assert_eq!(ui.level, "7");
        assert_eq!(ui.souls, "12,345");
        assert_eq!(ui.side, TeamSide::Sapphire);
        assert_eq!(ui.side_label, "Sapphire");
        assert_eq!(ui.initial, "H");
        assert!(!ui.has_portrait);
    }

    #[test]
    fn build_mix_counts_each_category_and_names_it() {
        use slint::Model;
        let item = |c| ItemVm {
            name: "Item".into(),
            icon: None,
            category: c,
        };
        let sections = vec![
            SectionVm {
                title: "Early".into(),
                items: vec![
                    item(Some(ItemCategory::Weapon)),
                    item(Some(ItemCategory::Weapon)),
                    item(Some(ItemCategory::Spirit)),
                ],
            },
            SectionVm {
                title: "Late".into(),
                items: vec![item(Some(ItemCategory::Vitality)), item(None)],
            },
        ];
        let mix = category_mix(&i18n(), &sections);
        let names: Vec<String> = mix.iter().map(|c| c.name.to_string()).collect();
        assert_eq!(names, ["Weapon 2", "Vitality 1", "Spirit 1"]);
        let ui = map_build(
            &i18n(),
            &BuildVm {
                build_id: "b".into(),
                name: "B".into(),
                hero_name: "H".into(),
                source: BuildSource::Manual,
                item_count: 5,
                sections,
                active: false,
                items_removed: 0,
            },
        );
        assert_eq!(ui.mix.row_count(), 3);
        assert_eq!(ui.sections.row_count(), 2);
    }

    #[test]
    fn abandoned_has_its_own_words_in_the_list_and_the_report() {
        let i = i18n();
        assert_eq!(outcome_ui(Outcome::Abandoned), MatchOutcome::Abandoned);
        assert_eq!(
            result_title(&i, Outcome::Abandoned, "Hero 1"),
            "Abandoned as Hero 1"
        );
        assert_eq!(
            result_title(&i, Outcome::NoResult, "Hero 1"),
            "No result as Hero 1"
        );
        let mut r = report(None);
        r.outcome = Outcome::Abandoned;
        let ui = map_report(&i, &Some(r));
        assert_eq!(ui.outcome, MatchOutcome::Abandoned);
        assert!(ui.summary_line.contains("abandoned"), "{}", ui.summary_line);
    }

    fn report(lead: Option<i32>) -> ReviewReportVm {
        ReviewReportVm {
            match_id: 1,
            outcome: Outcome::Win,
            hero_name: "Hero 1".into(),
            duration_secs: 34 * 60 + 12,
            lane_soul_lead: lead,
            lane_check_secs: 540,
            soul_sources: vec![
                SoulSourceVm {
                    kind: SoulSourceKind::LaneTroopers,
                    souls: 6000,
                },
                SoulSourceVm {
                    kind: SoulSourceKind::NeutralCamps,
                    souls: 3000,
                },
                SoulSourceVm {
                    kind: SoulSourceKind::Breakables,
                    souls: 500,
                },
                SoulSourceVm {
                    kind: SoulSourceKind::Urn,
                    souls: 400,
                },
                SoulSourceVm {
                    kind: SoulSourceKind::SinnersSacrifice,
                    souls: 100,
                },
            ],
        }
    }

    #[test]
    fn report_numbers_come_from_the_data() {
        use slint::Model;
        let ui = map_report(&i18n(), &Some(report(Some(450))));
        assert!(ui.has_report);
        assert_eq!(ui.lane_lead, "+450 souls");
        assert!(ui.lane_line.contains("450"), "{}", ui.lane_line);
        assert!(ui.summary_line.contains("34"), "{}", ui.summary_line);
        assert!(
            ui.summary_line.contains("lane troopers"),
            "{}",
            ui.summary_line
        );
        assert_eq!(ui.sources.row_count(), 5);
        assert_eq!(ui.sources.row_data(0).unwrap().souls, "6,000 souls (60%)");
    }

    #[test]
    fn report_lane_lead_covers_ahead_even_behind_and_missing() {
        let i = i18n();
        let ahead = map_report(&i, &Some(report(Some(10)))).lane_line;
        let even = map_report(&i, &Some(report(Some(0)))).lane_line;
        let behind = map_report(&i, &Some(report(Some(-10)))).lane_line;
        assert!(ahead != even && even != behind && ahead != behind);
        let missing = map_report(&i, &Some(report(None)));
        assert_eq!(missing.lane_lead, i.t("report-lane-unavailable"));
        assert!(!map_report(&i, &None).has_report);
    }

    #[test]
    fn build_flag_appears_only_when_items_were_removed() {
        let mut b = BuildVm {
            build_id: "b1".into(),
            name: "Build 1".into(),
            hero_name: "Hero 1".into(),
            source: BuildSource::Manual,
            item_count: 12,
            sections: vec![],
            active: false,
            items_removed: 0,
        };
        assert!(map_build(&i18n(), &b).flag.is_empty());
        b.items_removed = 2;
        assert!(map_build(&i18n(), &b).flag.contains('2'));
    }

    #[test]
    fn keys_for_all_variants_exist() {
        let i = i18n();
        let mut keys = vec![];
        for id in [
            CheckId::Steam,
            CheckId::Condebug,
            CheckId::AccountId,
            CheckId::Broadcast,
            CheckId::DeadlockApi,
            CheckId::MatchWatcher,
        ] {
            keys.push(check_label_key(id));
            keys.push(check_fail_key(id));
        }
        for c in [
            ItemCategory::Weapon,
            ItemCategory::Vitality,
            ItemCategory::Spirit,
        ] {
            keys.push(category_key(c));
        }
        for o in [
            Outcome::Win,
            Outcome::Loss,
            Outcome::NoResult,
            Outcome::Abandoned,
        ] {
            keys.push(result_key(o));
        }
        for k in [
            SoulSourceKind::LaneTroopers,
            SoulSourceKind::NeutralCamps,
            SoulSourceKind::Breakables,
            SoulSourceKind::Urn,
            SoulSourceKind::SinnersSacrifice,
            SoulSourceKind::Other,
        ] {
            keys.push(soul_key(k));
        }
        for p in [
            Phase::Idle,
            Phase::MatchDetected,
            Phase::Live,
            Phase::LiveNoBroadcast,
            Phase::AwaitingBroadcastEnd,
            Phase::Reviewing,
            Phase::Stopped,
        ] {
            keys.push(phase_key(p));
        }
        for a in [
            PurchaseAction::Buy,
            PurchaseAction::Sell,
            PurchaseAction::Upgrade,
        ] {
            keys.push(action_key(a));
        }
        for s in [
            BuildSource::InGame,
            BuildSource::External,
            BuildSource::Manual,
        ] {
            keys.push(build_source_key(s));
        }
        for e in [
            BuilderError::DuplicateItem,
            BuilderError::NoSuchSection,
            BuilderError::NoHero,
            BuilderError::NoName,
            BuilderError::Empty,
            BuilderError::SaveFailed,
        ] {
            keys.push(builder_error_key(e));
        }
        for k in &keys {
            assert!(i.has(k), "{k} missing from main.ftl");
            assert!(DYNAMIC_KEYS.contains(k), "{k} missing from DYNAMIC_KEYS");
        }
        assert_eq!(keys.len(), DYNAMIC_KEYS.len());
    }
}

#[cfg(test)]
mod window_tests {
    //! Tests that need a window object. They use Slint's testing backend, so no display is needed.
    use super::*;
    use crate::demo::Demo;
    use slint::Model;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    fn window() -> MainWindow {
        i_slint_backend_testing::init_no_event_loop();
        MainWindow::new().unwrap()
    }

    // TC-20-1: no Steam found, so Home shows a failed setup check.
    #[test]
    fn tc_20_1_no_steam_shows_a_failed_check() {
        let ui = window();
        let i18n = I18n::en_us();
        let home = HomeVm {
            checks: vec![SetupCheck {
                id: CheckId::Steam,
                state: CheckState::Fail,
                value: None,
            }],
            ..Default::default()
        };
        apply_home(&ui, &home, &i18n);
        let checks = ui.global::<AppState>().get_checks();
        assert_eq!(checks.row_count(), 1);
        let c = checks.row_data(0).unwrap();
        assert_eq!(c.state, Check::Fail);
        assert!(
            !c.detail.is_empty(),
            "a failed check carries text, not colour alone"
        );
    }

    // FP-10: Scouting Cards stay hidden until the game has started.
    #[test]
    fn fp_10_scouting_hidden_until_the_game_starts() {
        let ui = window();
        let i18n = I18n::en_us();
        let mut vm = crate::demo::demo_vm().scouting;
        vm.visible = false;
        apply_scouting(&ui, &vm, &i18n);
        assert!(!ui.global::<AppState>().get_scouting_visible());
        vm.visible = true;
        apply_scouting(&ui, &vm, &i18n);
        assert!(ui.global::<AppState>().get_scouting_visible());
    }

    #[test]
    fn scouting_shows_five_and_six_cards_grouped_by_team() {
        let ui = window();
        apply_scouting(&ui, &crate::demo::demo_vm().scouting, &I18n::en_us());
        let s = ui.global::<AppState>();
        assert_eq!(
            s.get_your_cards().row_count() + s.get_enemy_cards().row_count(),
            11
        );
        assert_eq!(s.get_your_side(), TeamSide::Amber);
        assert!(s.get_enemy_team_heading().contains("Sapphire"));
    }

    #[test]
    fn live_hides_kda_and_level_until_provided() {
        let ui = window();
        let mut live = crate::demo::demo_vm().live;
        apply_live(&ui, &live, &I18n::en_us());
        let s = ui.global::<AppState>();
        assert!(!s.get_show_kda() && !s.get_show_level());
        live.enemies[0].kda = Some(Kda {
            kills: 1,
            deaths: 2,
            assists: 3,
        });
        apply_live(&ui, &live, &I18n::en_us());
        assert!(s.get_show_kda() && !s.get_show_level());
    }

    #[test]
    fn demo_reaches_every_data_state_and_check_again_passes_on_retry() {
        let ui = window();
        let i18n = Arc::new(I18n::en_us());
        install_strings(&ui, i18n.clone());
        let demo = Rc::new(RefCell::new(Demo::new(i18n)));
        crate::demo::wire(&ui, demo.clone());
        crate::demo::initial_apply(&ui, &demo.borrow());
        let s = ui.global::<AppState>();

        s.set_current_view(1);
        let mut seen = vec![];
        for _ in 0..4 {
            s.invoke_demo_cycle_state();
            seen.push(s.get_scouting_state().kind);
        }
        assert_eq!(
            seen,
            vec![
                DataKind::Loading,
                DataKind::Empty,
                DataKind::Stale,
                DataKind::Error
            ]
        );
        s.invoke_retry(1);
        assert_eq!(s.get_scouting_state().kind, DataKind::Loaded);

        s.invoke_check_again();
        assert_eq!(s.get_setup_result(), Check::Fail);
        s.invoke_check_again();
        assert_eq!(s.get_setup_result(), Check::Pass);

        s.invoke_save_account_id("not a number".into());
        assert!(!s.get_account_id_error().is_empty());
        s.invoke_save_account_id("42".into());
        assert!(s.get_account_id_error().is_empty());
    }

    #[test]
    fn strings_global_looks_up_fluent_messages() {
        let ui = window();
        install_strings(&ui, Arc::new(I18n::en_us()));
        let tr = ui.global::<crate::ui::Tr>();
        assert_eq!(tr.invoke_t("nav-home".into()), "Home");
    }
}
