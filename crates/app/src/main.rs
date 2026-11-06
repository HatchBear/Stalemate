//! The `stalemate` binary (PRG-20). This shell renders view models. Until the core is wired in
//! (a later task), `--demo` fills them with made-up data.

mod builder;
mod demo;
mod i18n;
mod ui;
mod ui_map;
mod view_model;

use anyhow::{bail, Result};
use slint::ComponentHandle;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;
use ui::{AppState, MainWindow};

#[derive(Default, Debug)]
struct Args {
    demo: bool,
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
    let args = parse_args(std::env::args().skip(1))?;
    let i18n = Arc::new(i18n::I18n::en_us());
    let window = MainWindow::new()?;
    ui_map::install_strings(&window, i18n.clone());

    let state = window.global::<AppState>();
    state.set_reduce_motion(args.reduce_motion);
    state.set_setup_open(args.first_run);
    // Slint durations are whole milliseconds. CD-06: 150 ms or less, none with reduce-motion.
    window
        .global::<ui::Theme>()
        .set_motion(if args.reduce_motion { 0 } else { 120 });

    if args.demo {
        let controller = Rc::new(RefCell::new(demo::Demo::new(i18n)));
        demo::wire(&window, controller.clone());
        demo::initial_apply(&window, &controller.borrow());
    } else {
        // Core wiring comes later. Until then, show the empty state of every view.
        let vm = view_model::AppVm {
            settings: view_model::SettingsVm {
                account_id: None,
                version: env!("CARGO_PKG_VERSION").into(),
            },
            ..Default::default()
        };
        ui_map::apply_all(&window, &vm, &i18n);
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
    Ok(())
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
        assert!(a.demo && a.first_run);
        assert!(parse_args(["--nope"].into_iter().map(String::from)).is_err());
    }
}
