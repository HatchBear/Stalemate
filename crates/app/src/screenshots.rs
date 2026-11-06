//! Developer aid: renders each view in demo mode to a PNG, with no window and no display.
//!
//! It uses Slint's software renderer, so nothing opens on the desktop. Run it with
//! `cargo test -p stalemate-app screenshots -- --ignored`. The images go to
//! `target/screenshots/`, 1280x800 (plus one at 1280x720). The software renderer draws no blur
//! and no element rotation, so shadows come from layers and stamps from polygons (doc 09).
//! Run it on its own: it installs its own Slint platform, which only one test per process can do.

use crate::demo::{initial_apply, wire, Demo};
use crate::i18n::I18n;
use crate::ui::{AppState, BuildsTab, ItemCategory, ItemChipUi, MainWindow, Theme};
use crate::ui_map::{self, install_strings};
use crate::view_model::{MatchSummary, Outcome};
use slint::platform::software_renderer::{MinimalSoftwareWindow, RepaintBufferType};
use slint::platform::WindowEvent;
use slint::platform::{Platform, WindowAdapter};
use slint::{
    ComponentHandle, Image, ModelRc, PhysicalSize, PlatformError, Rgb8Pixel, Rgba8Pixel,
    SharedPixelBuffer, VecModel,
};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

struct HeadlessPlatform {
    window: Rc<MinimalSoftwareWindow>,
}

impl Platform for HeadlessPlatform {
    fn create_window_adapter(&self) -> Result<Rc<dyn WindowAdapter>, PlatformError> {
        Ok(self.window.clone())
    }
}

fn out_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/screenshots")
}

fn render(window: &Rc<MinimalSoftwareWindow>, w: u32, h: u32, name: &str) {
    render_crop(window, w, h, name, None);
}

/// Renders at `w` x `h` physical pixels and, with `crop` (x, y, width, height), keeps only that part.
fn render_crop(
    window: &Rc<MinimalSoftwareWindow>,
    w: u32,
    h: u32,
    name: &str,
    crop: Option<(u32, u32, u32, u32)>,
) {
    // `STALEMATE_SHOTS=01,04` renders only the screens whose names start with one of these.
    if let Ok(only) = std::env::var("STALEMATE_SHOTS") {
        if !only.split(',').any(|p| name.starts_with(p.trim())) {
            return;
        }
    }
    window.set_size(PhysicalSize::new(w, h));
    slint::platform::update_timers_and_animations();
    let mut buf = vec![Rgb8Pixel::default(); (w * h) as usize];
    window.request_redraw();
    let drawn = window.draw_if_needed(|renderer| {
        renderer.render(&mut buf, w as usize);
    });
    assert!(drawn, "{name}: nothing was drawn");
    let (cx, cy, cw, ch) = crop.unwrap_or((0, 0, w, h));
    let mut bytes = Vec::with_capacity((cw * ch * 3) as usize);
    for y in cy..cy + ch {
        for x in cx..cx + cw {
            let p = buf[(y * w + x) as usize];
            bytes.extend_from_slice(&[p.r, p.g, p.b]);
        }
    }
    let (w, h) = (cw, ch);
    let dir = out_dir();
    std::fs::create_dir_all(&dir).unwrap();
    let file = std::fs::File::create(dir.join(format!("{name}.png"))).unwrap();
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .unwrap()
        .write_image_data(&bytes)
        .unwrap();
}

#[test]
#[ignore = "developer aid: writes PNGs to target/screenshots/"]
fn screenshots() {
    // Start clean: no old screenshots stay behind.
    if std::env::var_os("STALEMATE_SHOTS").is_none() {
        let _ = std::fs::remove_dir_all(out_dir());
    }
    let window = MinimalSoftwareWindow::new(RepaintBufferType::NewBuffer);
    slint::platform::set_platform(Box::new(HeadlessPlatform {
        window: window.clone(),
    }))
    .expect("the platform can be set once per process");

    let i18n = Arc::new(I18n::en_us());
    let ui = MainWindow::new().unwrap();
    install_strings(&ui, i18n.clone());
    let demo = Rc::new(RefCell::new(Demo::new(i18n.clone())));
    wire(&ui, demo.clone());
    initial_apply(&ui, &demo.borrow());
    // No fades, so the first frame is the finished one.
    ui.global::<Theme>().set_motion(0);
    ui.show().unwrap();

    let s = ui.global::<AppState>();
    let shot = |name: &str| render(&window, 1280, 800, name);
    let tall = |name: &str| render(&window, 1280, 1700, name);

    s.set_current_view(0);
    shot("01-home");
    // The whole page, to see what scrolls out of the first screen.
    tall("01b-home-full");
    // Home with an abandoned match as the last one.
    {
        let d = demo.borrow();
        let mut home = d.vm.home.clone();
        home.last_match = home.last_match.map(|m| MatchSummary {
            outcome: Outcome::Abandoned,
            ..m
        });
        ui_map::apply_home(&ui, &home, &i18n);
    }
    tall("01c-home-abandoned");
    {
        let d = demo.borrow();
        ui_map::apply_home(&ui, &d.vm.home, &i18n);
    }
    s.set_setup_open(true);
    shot("02-setup");
    s.set_setup_open(false);
    s.set_current_view(1);
    shot("03-scouting");
    s.set_current_view(2);
    shot("04-live");
    tall("04b-live-full");
    s.set_current_view(3);
    s.set_selected_review(-1);
    shot("05-reviews-list");
    s.invoke_select_review(0);
    s.set_selected_review(0);
    shot("06-reviews-report");
    s.invoke_select_review(2);
    s.set_selected_review(2);
    shot("07-reviews-no-result");
    s.invoke_select_review(3);
    s.set_selected_review(3);
    shot("07b-reviews-abandoned");
    s.set_current_view(4);
    s.set_builds_tab(BuildsTab::Library);
    tall("08-builds-library");
    s.set_builds_tab(BuildsTab::Community);
    s.invoke_search_community(0);
    tall("08b-builds-community");
    s.set_builds_tab(BuildsTab::Builder);
    s.invoke_builder_search("item".into());
    s.invoke_builder_add_item(0);
    s.invoke_builder_add_item(1);
    s.invoke_builder_add_item(2);
    s.invoke_builder_search("item".into());
    tall("09-builds-builder");
    s.set_current_view(5);
    shot("10-settings");
    // The nav's sub-items scroll to a section.
    s.set_settings_section(5);
    s.set_settings_jump(s.get_settings_jump() + 1);
    shot("10a-settings-jump-about");
    s.set_settings_section(0);
    s.set_settings_jump(s.get_settings_jump() + 1);
    // The diagnostics preview and a saved file.
    s.invoke_open_diag_save();
    s.set_diag_note("The delay looked wrong at 12:00.".into());
    tall("10b-settings-diagnostics-preview");
    s.invoke_save_diag(s.get_diag_note(), false);
    tall("10c-settings-diagnostics-saved");
    s.set_text_scale_pct(150);
    shot("11-settings-150pct");
    s.set_text_scale_pct(100);

    // States: loading, empty, stale, error on Scouting; then offline.
    s.set_current_view(1);
    for name in [
        "12-scouting-loading",
        "13-scouting-empty",
        "14-scouting-stale",
        "15-scouting-error",
    ] {
        s.invoke_demo_cycle_state();
        shot(name);
    }
    s.invoke_demo_toggle_offline();
    shot("16-scouting-error-offline");

    // The smallest window the layout is meant for.
    s.invoke_retry(1);
    s.invoke_demo_toggle_offline();
    s.set_current_view(0);
    render(&window, 1280, 720, "17-home-1280x720");
    s.set_current_view(1);
    render(&window, 1280, 720, "18-scouting-1280x720");
    s.set_current_view(2);
    render(&window, 1280, 720, "19-live-1280x720");

    // The builder with item tiles of all three categories, full page.
    s.set_current_view(4);
    s.set_builds_tab(BuildsTab::Builder);
    s.invoke_builder_search("item".into());
    render(&window, 1280, 1400, "20-builds-builder-tiles");
    // The Live board at the smallest size, with its small tiles.
    s.set_current_view(2);
    render(&window, 1280, 720, "21-live-1280x720-tiles");
    // A close-up at 2x of one row of tiles: each category twice, some with art, some with the glyph.
    let art = |r: u8, g: u8, b: u8| {
        let mut px = SharedPixelBuffer::<Rgba8Pixel>::new(32, 32);
        for (i, p) in px.make_mut_slice().iter_mut().enumerate() {
            let (x, y) = ((i % 32) as i32 - 16, (i / 32) as i32 - 16);
            // A round gem shape, so the art is easy to tell from the glyph.
            *p = if x * x + y * y < 200 {
                Rgba8Pixel::new(r, g, b, 255)
            } else {
                Rgba8Pixel::new(0, 0, 0, 0)
            };
        }
        Image::from_rgba8(px)
    };
    let chip = |name: &str, c: ItemCategory, label: &str, art: Option<Image>| ItemChipUi {
        name: name.into(),
        category: c,
        category_label: label.into(),
        has_icon: art.is_some(),
        icon: art.unwrap_or_default(),
    };
    let chips = vec![
        chip(
            "Item A",
            ItemCategory::Weapon,
            "Weapon",
            Some(art(250, 245, 230)),
        ),
        chip(
            "Item with a long name",
            ItemCategory::Weapon,
            "Weapon",
            None,
        ),
        chip(
            "Item B",
            ItemCategory::Vitality,
            "Vitality",
            Some(art(250, 245, 230)),
        ),
        chip("Item D", ItemCategory::Vitality, "Vitality", None),
        chip(
            "Item C",
            ItemCategory::Spirit,
            "Spirit",
            Some(art(250, 245, 230)),
        ),
        chip("Item F", ItemCategory::Spirit, "Spirit", None),
    ];
    s.set_builder_results(ModelRc::new(VecModel::from(chips)));
    s.set_current_view(4);
    s.set_builds_tab(BuildsTab::Builder);
    window.dispatch_event(WindowEvent::ScaleFactorChanged { scale_factor: 2.0 });
    render_crop(&window, 2560, 3000, "22-item-tiles-closeup", Some(CLOSEUP));
}

/// Where the tile row sits in the builder page at 2x (found by looking at the render).
const CLOSEUP: (u32, u32, u32, u32) = (500, 1250, 1100, 330);
