//! The `stalemate` binary (PRG-20). It renders view models from the real pipeline (`runtime`).
//! `--demo` fills them with made-up data instead.

mod builder;
mod demo;
mod diag;
mod i18n;
mod runtime;
#[cfg(test)]
mod screenshots;
mod ui;
mod ui_map;
mod view_model;
mod vm_build;

use anyhow::{bail, Result};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use ui::{AppState, MainWindow};

#[derive(Default, Debug)]
struct Args {
    demo: bool,
    /// Print `stalemate <version> (<package format>)` and exit (TST-094).
    version: bool,
    first_run: bool,
    reduce_motion: bool,
    /// Developer aid: render each scenario to a PPM file in this folder, then exit.
    snapshot_dir: Option<std::path::PathBuf>,
    /// Developer aid: close the window after this many milliseconds.
    quit_after_ms: Option<u64>,
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Args> {
    let mut out = Args {
        reduce_motion: std::env::var_os("STALEMATE_REDUCE_MOTION").is_some_and(|v| v != "0"),
        ..Default::default()
    };
    let mut args = args;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--demo" => out.demo = true,
            "--version" | "-V" => out.version = true,
            "--first-run" => out.first_run = true,
            "--reduce-motion" => out.reduce_motion = true,
            "--snapshot-dir" => out.snapshot_dir = args.next().map(Into::into),
            "--quit-after-ms" => out.quit_after_ms = args.next().and_then(|v| v.parse().ok()),
            other => bail!("unknown argument: {other}"),
        }
    }
    Ok(out)
}

fn main() -> Result<()> {
    // The local UTC offset must be read before any thread starts (the tokio runtime, the logger,
    // the window). On Linux the call fails once the process is multithreaded.
    let utc_offset = time::UtcOffset::current_local_offset();
    let args = parse_args(std::env::args().skip(1))?;
    // Detected paths, for the package format and the log folder. A failure only costs those.
    let paths = stalemate_core::settings::Paths::detect().ok();
    if args.version {
        println!(
            "{}",
            diag::version_line(paths.as_ref().map(|p| p.shipped_config_dir.as_path()))
        );
        return Ok(());
    }
    if !args.demo && args.snapshot_dir.is_none() {
        if let Some(p) = &paths {
            diag::install_logging(&p.data_dir);
        }
    }
    let utc_offset = utc_offset.unwrap_or_else(|e| {
        tracing::warn!("could not read the local time zone, using UTC for log-line times: {e}");
        time::UtcOffset::UTC
    });
    let i18n = Arc::new(i18n::I18n::en_us());
    let window = create_window(paths.as_ref().map(|p| p.data_dir.as_path()))?;
    ui_map::install_strings(&window, i18n.clone());

    let state = window.global::<AppState>();
    state.set_reduce_motion(args.reduce_motion);
    state.set_setup_open(args.first_run);
    // Slint durations are whole milliseconds. CD-06: 150 ms or less, none with reduce-motion.
    window
        .global::<ui::Theme>()
        .set_motion(if args.reduce_motion { 0 } else { 120 });

    let runtime;
    if args.demo {
        runtime = None::<runtime::Runtime>;
        let controller = Rc::new(RefCell::new(demo::Demo::new(i18n)));
        demo::wire(&window, controller.clone());
        demo::initial_apply(&window, &controller.borrow());
    } else {
        // The empty state shows until the core has loaded; then it pushes the real view models.
        let vm = view_model::AppVm {
            settings: view_model::SettingsVm {
                version: env!("CARGO_PKG_VERSION").into(),
                ..Default::default()
            },
            ..Default::default()
        };
        ui_map::apply_all(&window, &vm, &i18n);
        runtime = Some(runtime::start(&window, utc_offset)?);
    }

    if let Some(ms) = args.quit_after_ms {
        let timer = slint::Timer::default();
        timer.start(
            slint::TimerMode::SingleShot,
            std::time::Duration::from_millis(ms),
            || {
                let _ = slint::quit_event_loop();
            },
        );
        std::mem::forget(timer);
    }
    if let Some(dir) = args.snapshot_dir.clone() {
        snapshot::run(&window, dir)?;
    }

    window.run()?;
    drop(runtime);
    Ok(())
}

/// Set while the app runs again after a start-up panic in the font system.
#[cfg(target_os = "linux")]
const FONT_RETRY_VAR: &str = "STALEMATE_FONT_RETRY";

/// Creates the main window, and survives a system with no fonts at all (a bare container).
///
/// Slint asks fontconfig for the system's fonts while it starts. With none installed, `fontique`
/// 0.11 (inside Slint 1.18) unwraps a `NoMatch` error in `SystemFonts::new`
/// (`fontique/src/backend/fontconfig.rs`, line 685) and the app panics before any window exists.
/// Slint has no switch to skip the system fonts, and after that panic it cannot start again in the
/// same process ("platform initialized in another thread"). So the panic is caught here, and on
/// Linux the app starts itself again with `FONTCONFIG_FILE` pointing at a small config of its own
/// that lists one bundled font (Jost, SIL OFL). The UI's own fonts (Jost, Noto Sans, Barlow
/// Condensed) are embedded in the binary and load either way.
fn create_window(data_dir: Option<&std::path::Path>) -> Result<MainWindow> {
    use std::sync::atomic::{AtomicBool, Ordering};
    static FROM_FONTS: AtomicBool = AtomicBool::new(false);
    // The first attempt's panic is expected; keep it out of the terminal and note it in the log.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|info| {
        let in_fonts = info
            .location()
            .is_some_and(|l| l.file().contains("fontique"));
        FROM_FONTS.store(in_fonts, Ordering::SeqCst);
        tracing::warn!("panic while starting the window: {info}");
    }));
    let first = std::panic::catch_unwind(MainWindow::new);
    std::panic::set_hook(hook);
    match first {
        Ok(window) => Ok(window?),
        // Any other panic is a real bug: let it through.
        Err(payload) if !FROM_FONTS.load(Ordering::SeqCst) => std::panic::resume_unwind(payload),
        Err(_) => {
            tracing::warn!(
                "no system fonts (fontique NoMatch, fontconfig.rs:685): starting again with a bundled fallback font"
            );
            restart_with_fallback_font(data_dir)
        }
    }
}

#[cfg(target_os = "linux")]
fn restart_with_fallback_font(data_dir: Option<&std::path::Path>) -> Result<MainWindow> {
    use std::os::unix::process::CommandExt;
    if std::env::var_os(FONT_RETRY_VAR).is_some() {
        bail!("the window could not start: the font system failed even with the fallback font");
    }
    let base = data_dir
        .map(|d| d.join("fallback-fonts"))
        .unwrap_or_else(|| std::env::temp_dir().join("stalemate-fallback-fonts"));
    let conf = fallback_fontconfig(&base)?;
    // `exec` only returns when it failed.
    let err = std::process::Command::new(std::env::current_exe()?)
        .args(std::env::args_os().skip(1))
        .env("FONTCONFIG_FILE", conf)
        .env(FONT_RETRY_VAR, "1")
        .exec();
    bail!("the window could not start, and starting again failed: {err}")
}

#[cfg(not(target_os = "linux"))]
fn restart_with_fallback_font(_: Option<&std::path::Path>) -> Result<MainWindow> {
    bail!("the window could not start: the font system failed")
}

/// Jost, the one font the fallback config lists.
#[cfg(any(target_os = "linux", test))]
const FALLBACK_FONT: &[u8] = include_bytes!("../../../assets/fonts/jost/Jost-Variable.ttf");

/// Writes the fallback font and a minimal fontconfig file under `base`; returns the file's path.
#[cfg(any(target_os = "linux", test))]
fn fallback_fontconfig(base: &std::path::Path) -> Result<std::path::PathBuf> {
    let fonts = base.join("fonts");
    let cache = base.join("cache");
    std::fs::create_dir_all(&fonts)?;
    std::fs::create_dir_all(&cache)?;
    std::fs::write(fonts.join("Jost-Variable.ttf"), FALLBACK_FONT)?;
    let conf = base.join("fonts.conf");
    std::fs::write(
        &conf,
        format!(
            "<?xml version=\"1.0\"?>\n<!DOCTYPE fontconfig SYSTEM \"fonts.dtd\">\n<fontconfig>\n  <dir>{}</dir>\n  <cachedir>{}</cachedir>\n</fontconfig>\n",
            fonts.display(),
            cache.display()
        ),
    )?;
    Ok(conf)
}

/// Developer aid for checking the look without a person at the screen.
mod snapshot {
    use super::*;
    use slint::{Timer, TimerMode};
    use std::time::Duration;

    pub fn run(window: &MainWindow, dir: std::path::PathBuf) -> Result<()> {
        std::fs::create_dir_all(&dir)?;
        let weak = window.as_weak();
        let step = Rc::new(RefCell::new(0usize));
        let timer = Rc::new(Timer::default());
        let keep = timer.clone();
        timer.start(TimerMode::Repeated, Duration::from_millis(500), move || {
            let Some(ui) = weak.upgrade() else { return };
            let s = ui.global::<AppState>();
            let mut i = step.borrow_mut();
            // Save the current scenario, then move to the next.
            if *i > 0 {
                if let Ok(buf) = ui.window().take_snapshot() {
                    let name = format!("{:02}.ppm", *i - 1);
                    let mut data =
                        format!("P6\n{} {}\n255\n", buf.width(), buf.height()).into_bytes();
                    for p in buf.as_slice() {
                        data.extend_from_slice(&[p.r, p.g, p.b]);
                    }
                    let _ = std::fs::write(dir.join(name), data);
                }
            }
            match *i {
                0..=5 => {
                    s.set_current_view(*i as i32);
                    s.set_setup_open(false);
                }
                6 => {
                    s.set_current_view(0);
                    s.set_setup_open(true);
                }
                7 => {
                    s.set_setup_open(false);
                    s.set_current_view(5);
                    s.set_text_scale_pct(150);
                }
                8 => {
                    s.set_text_scale_pct(100);
                    s.set_current_view(3);
                    s.invoke_select_review(0);
                }
                9 => {
                    s.set_current_view(4);
                    s.set_builds_tab(ui::BuildsTab::Builder);
                    s.invoke_builder_search("item".into());
                    s.invoke_builder_add_item(0);
                    s.invoke_builder_add_item(1);
                }
                10 => {
                    s.set_current_view(1);
                    for _ in 0..4 {
                        s.invoke_demo_cycle_state();
                    }
                    s.invoke_demo_toggle_offline();
                }
                11 => {
                    s.set_current_view(2);
                    s.invoke_demo_cycle_state();
                }
                12 => {
                    s.set_current_view(2);
                    s.invoke_demo_cycle_state();
                    s.invoke_demo_cycle_state();
                }
                _ => {
                    let _ = slint::quit_event_loop();
                }
            }
            *i += 1;
            let _ = &keep;
        });
        std::mem::forget(timer);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn args_parse() {
        let a = parse_args(["--demo", "--first-run"].into_iter().map(String::from)).unwrap();
        assert!(a.demo && a.first_run && !a.version);
        let v = parse_args(["--version"].into_iter().map(String::from)).unwrap();
        assert!(v.version);
        assert!(parse_args(["--nope"].into_iter().map(String::from)).is_err());
    }

    #[test]
    fn fallback_fontconfig_lists_one_bundled_font() {
        let dir = std::env::temp_dir().join(format!("stalemate-fonts-test-{}", std::process::id()));
        let conf = fallback_fontconfig(&dir).unwrap();
        let text = std::fs::read_to_string(&conf).unwrap();
        assert!(text.contains("<dir>") && text.contains("<cachedir>"));
        assert!(
            dir.join("fonts/Jost-Variable.ttf")
                .metadata()
                .unwrap()
                .len()
                > 10_000
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
