//! The GTK tests: real widgets on a headless Broadway display, with the
//! built gresource and the repo's settings schema (scripts/dev/gtk-tests.sh
//! sets all of that up). Each check is its own test, so one run reports
//! every failure and a filter runs one check alone. They all run on one GTK
//! thread, one after another (`gtk::test_synced`): the process is set up
//! once (`setup`), and the state the checks change is put back after each
//! one (`clean_up`), so no check depends on another having run first.

use gtk::prelude::*;

use crate::app::components::{CommandPaletteDialog, LyricsPageWidget, SettingsDialog};

/// Declares GTK tests: each `name => check` is a test that runs `check`
/// through `run`. Ignored in a plain `cargo test`, which has no Broadway.
macro_rules! gtk_tests {
    ($($(#[$doc:meta])* $name:ident => $check:expr),* $(,)?) => {
        $(
            $(#[$doc])*
            #[test]
            #[ignore = "needs Broadway: run scripts/dev/gtk-tests.sh"]
            fn $name() {
                super::run(concat!(module_path!(), "::", stringify!($name)), $check)
            }
        )*
    };
}

mod dumps;
mod library;
mod lyrics;
mod now_panel;
mod playback_bar;
mod queue;
mod resources_icons;
mod settings;
mod shell_layout;

/// How to run the GTK tests: they need a headless display, the built gresource and the
/// repo's settings schema, which scripts/dev/gtk-tests.sh sets up.
const GTK_TESTS_HINT: &str = "needs Broadway: run scripts/dev/gtk-tests.sh";

/// Runs one GTK check on the shared GTK thread: the setup first (once per
/// process), then the check, then the clean-up, even when the check
/// panicked. The test fails when the check panicked, when a `check` call
/// failed, or when libadwaita or GTK reported an overflow or a layout error
/// while it ran. Ends with a line of its own: "GTK test <name>: ok in <s>"
/// (or FAILED), since the test harness's result line can end up after the
/// check's output.
fn run(name: &'static str, test: fn()) {
    // Never a silent pass: an explicit run without the setup fails and says
    // why, before GTK starts (GTK failing to start on the test thread aborts
    // the whole run)
    let Some(gresource) = std::env::var_os("CANTO_TEST_GRESOURCE") else {
        panic!("CANTO_TEST_GRESOURCE is not set: {}", GTK_TESTS_HINT);
    };
    // The windows must open on a headless display, never on the desktop session
    if std::env::var("GDK_BACKEND").as_deref() != Ok("broadway") {
        panic!("GDK_BACKEND is not broadway: {}", GTK_TESTS_HINT);
    }
    let name = name.trim_start_matches("canto::app::components::gtk_tests::");
    gtk::test_synced(move || {
        let started = std::time::Instant::now();
        let baseline = setup(&gresource);
        FAILED_CHECKS.with(|failed| failed.borrow_mut().clear());
        for list in [&OVERFLOWS, &LAYOUT_ERRORS] {
            if let Ok(mut list) = list.lock() {
                list.clear();
            }
        }
        let outcome = std::panic::catch_unwind(test);
        clean_up(&baseline);
        let drain = |list: &std::sync::Mutex<Vec<String>>| {
            list.lock()
                .map(|mut list| std::mem::take(&mut *list))
                .unwrap_or_default()
        };
        let overflows = drain(&OVERFLOWS);
        check(
            "nothing overflows its breakpoint bin (libadwaita's \"exceeds\")",
            overflows.is_empty(),
            format!("{:?}", overflows),
        );
        let layout_errors = drain(&LAYOUT_ERRORS);
        check(
            "GTK allocated no widget a negative size and logged no critical",
            layout_errors.is_empty(),
            format!("{:?}", layout_errors),
        );
        let failed = FAILED_CHECKS.with(|failed| std::mem::take(&mut *failed.borrow_mut()));
        eprintln!(
            "\nGTK test {}: {} in {:.1} s",
            name,
            if outcome.is_ok() && failed.is_empty() {
                "ok"
            } else {
                "FAILED"
            },
            started.elapsed().as_secs_f64()
        );
        if let Err(panic) = outcome {
            if !failed.is_empty() {
                eprintln!("{} checks also failed: {:?}", failed.len(), failed);
            }
            std::panic::resume_unwind(panic);
        }
        assert!(
            failed.is_empty(),
            "{} checks failed: {:?}",
            failed.len(),
            failed
        );
    })
}

/// What the clean-up puts back after each test.
#[derive(Clone)]
struct Baseline {
    animations: bool,
    xft_dpi: i32,
}

thread_local! {
    /// The setup's outcome, kept on the GTK thread: done once, and a setup
    /// that failed fails every test with the same reason.
    static SETUP: std::cell::RefCell<Option<Result<Baseline, String>>> = Default::default();

    /// The checks that failed (see `check`).
    static FAILED_CHECKS: std::cell::RefCell<Vec<String>> = Default::default();
}

/// Sets the process up for every test, once: libadwaita, the log watch, the
/// gresource, all the app's style sheets and its icons, and the custom
/// widget types the UI files name. Returns what the clean-up restores.
fn setup(gresource: &std::ffi::OsStr) -> Baseline {
    let done = SETUP.with(|setup| setup.borrow().clone());
    let outcome = done.unwrap_or_else(|| {
        let outcome = std::panic::catch_unwind(|| set_up_once(gresource)).map_err(|panic| {
            panic
                .downcast_ref::<String>()
                .cloned()
                .or_else(|| panic.downcast_ref::<&str>().map(|s| s.to_string()))
                .unwrap_or_else(|| "the GTK test setup panicked".to_string())
        });
        SETUP.with(|setup| *setup.borrow_mut() = Some(outcome.clone()));
        outcome
    });
    outcome.unwrap_or_else(|e| panic!("GTK test setup failed: {}", e))
}

fn set_up_once(gresource: &std::ffi::OsStr) -> Baseline {
    // gtk::test_synced started GTK on this thread
    libadwaita::init().expect("libadwaita::init should succeed");
    watch_overflows();

    let res = gio::Resource::load(gresource).unwrap_or_else(|e| {
        panic!(
            "failed to load gresource {}: {e}",
            gresource.to_string_lossy()
        )
    });
    gio::resources_register(&res);

    let display = gtk::gdk::Display::default().expect("default GDK display must exist");
    let css = gtk::CssProvider::new();
    css.load_from_resource("/io/github/gusthanks/Canto/app.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    // The widgets' own style sheets, which the app adds as each widget is
    // first built: all of them up front, so no check depends on another
    // having built a widget first
    for sheet in [
        "/io/github/gusthanks/Canto/components/card.css",
        "/io/github/gusthanks/Canto/components/details_page/style.css",
        "/io/github/gusthanks/Canto/components/playback.css",
        "/io/github/gusthanks/Canto/components/track_row.css",
        "/io/github/gusthanks/Canto/components/drawer.css",
        "/io/github/gusthanks/Canto/components/segmented_button.css",
        "/io/github/gusthanks/Canto/components/selection_toolbar.css",
        "/io/github/gusthanks/Canto/components/playlist_picker.css",
        "/io/github/gusthanks/Canto/components/playlist_dialog.css",
        "/io/github/gusthanks/Canto/components/search.css",
        "/io/github/gusthanks/Canto/components/command_palette.css",
    ] {
        crate::app::components::display_add_css_provider(sheet);
    }

    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_resource_path("/io/github/gusthanks/Canto/icons");

    crate::app::components::expose_custom_widgets();

    let gtk_settings = gtk::Settings::default().expect("GTK settings");
    Baseline {
        animations: gtk_settings.is_gtk_enable_animations(),
        xft_dpi: gtk_settings.gtk_xft_dpi(),
    }
}

/// Puts back what a test may have changed: closes its windows, restores
/// animations and the text size, turns the forced dark look off and resets
/// every app setting.
fn clean_up(baseline: &Baseline) {
    for window in gtk::Window::list_toplevels() {
        if let Ok(window) = window.downcast::<gtk::Window>() {
            window.destroy();
        }
    }
    let gtk_settings = gtk::Settings::default().expect("GTK settings");
    if gtk_settings.is_gtk_enable_animations() != baseline.animations {
        gtk_settings.set_gtk_enable_animations(baseline.animations);
    }
    if gtk_settings.gtk_xft_dpi() != baseline.xft_dpi {
        gtk_settings.set_gtk_xft_dpi(baseline.xft_dpi);
    }
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::Default);
    let settings = gio::Settings::new(canto_config::settings::SETTINGS);
    if let Some(schema) = settings.settings_schema() {
        for key in schema.list_keys() {
            settings.reset(&key);
        }
    }
    pump(100);
}

/// The dark look the app's sample renders and most checks use.
fn force_dark() {
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
}

/// Animations off, so a check reads each change's end state at once.
fn no_animations() {
    gtk::Settings::default()
        .expect("GTK settings")
        .set_gtk_enable_animations(false);
}

/// The dark look with a cover's accent color, as the sample renders show
/// the app while a song plays. The accent goes away with the guard.
struct SampleLook(gtk::CssProvider);

impl SampleLook {
    fn new() -> Self {
        force_dark();
        let theme = gtk::CssProvider::new();
        theme.load_from_string("@define-color canto_cover_accent #c0392b;");
        gtk::style_context_add_provider_for_display(
            &gtk::gdk::Display::default().unwrap(),
            &theme,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
        Self(theme)
    }
}

impl Drop for SampleLook {
    fn drop(&mut self) {
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_remove_provider_for_display(&display, &self.0);
        }
    }
}

/// What libadwaita reported overflowing during the test: a breakpoint bin
/// whose child needs more than the bin's width ("exceeds"), which cuts the
/// child's end. The checks measure a few widths each; this catches one
/// between them, or in a step that measures nothing.
static OVERFLOWS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// What GTK reported going wrong in a layout during the test: a widget
/// allocated a negative size ("attempt to allocate"), or any critical. Both
/// come from state changed in the middle of an allocation, after the sizes
/// were measured (a CSS class adding margins, for one).
static LAYOUT_ERRORS: std::sync::Mutex<Vec<String>> = std::sync::Mutex::new(Vec::new());

/// Keeps libadwaita's "exceeds" warnings in `OVERFLOWS` and GTK's layout
/// errors in `LAYOUT_ERRORS`, every message still written out as before.
/// Once per process (GLib allows one writer).
fn watch_overflows() {
    glib::log_set_writer_func(|level, fields| {
        let field = |key: &str| {
            fields
                .iter()
                .find(|field| field.key() == key)
                .and_then(|field| field.value_str())
        };
        let domain = field("GLIB_DOMAIN");
        let message = field("MESSAGE");
        if domain == Some("Adwaita") {
            if let Some(message) = message.filter(|m| m.contains(" exceeds ")) {
                if let Ok(mut overflows) = OVERFLOWS.lock() {
                    overflows.push(message.to_string());
                }
            }
        }
        if domain == Some("Gtk") {
            let negative = message.is_some_and(|m| m.contains("attempt to allocate"));
            if negative || level == glib::LogLevel::Critical {
                if let Ok(mut errors) = LAYOUT_ERRORS.lock() {
                    errors.push(message.unwrap_or_default().to_string());
                }
            }
        }
        glib::log_writer_default(level, fields)
    });
}

/// A check that reports instead of stopping the test, so one run shows every
/// one that fails: printed either way, and the test fails at its end if any
/// did. `seen` is what was measured.
fn check(what: &str, ok: bool, seen: impl std::fmt::Display) {
    eprintln!(
        "check {}: {} ({})",
        what,
        if ok { "ok" } else { "FAILED" },
        seen
    );
    if !ok {
        FAILED_CHECKS.with(|failed| failed.borrow_mut().push(what.to_string()));
    }
}

/// Waits (up to 5 s) for `widget` to be on screen, then lets it settle.
fn wait_mapped(widget: &impl IsA<gtk::Widget>, settle_ms: u64) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !widget.is_mapped() && std::time::Instant::now() < deadline {
        pump(10);
    }
    pump(settle_ms);
}

/// The widgets under `widget` (itself included) that have `class`.
fn with_class(widget: &gtk::Widget, class: &str) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut stack = vec![widget.clone()];
    while let Some(w) = stack.pop() {
        if w.has_css_class(class) {
            found.push(w.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    found
}

/// Waits (up to 5 s) until `widget` has a size and stays where it is for
/// three reads in a row.
fn wait_laid_out(widget: &impl IsA<gtk::Widget>, relative_to: &impl IsA<gtk::Widget>) {
    let read = || {
        widget
            .compute_bounds(relative_to)
            .map(|b| (b.x(), b.y(), b.width(), b.height()))
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut last = read();
    let mut same = 0;
    while same < 3 && std::time::Instant::now() < deadline {
        pump(50);
        let now = read();
        let sized = widget.is_mapped() && now.is_some_and(|(_, _, w, h)| w > 0.0 && h > 0.0);
        same = if sized && now == last { same + 1 } else { 0 };
        last = now;
    }
}

/// The window's pixels (RGBA) inside `widget`'s bounds, from a fresh render.
fn pixels_in(window: &gtk::Window, widget: &gtk::Widget) -> Vec<u8> {
    let (w, h) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let mut node = None;
    for _ in 0..60 {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, w as f64, h as f64);
        node = snapshot.to_node();
        if node.is_some() {
            break;
        }
        window.queue_draw();
        pump(50);
    }
    let node = node.expect("a render of the window");
    let renderer = window.renderer().expect("the window's renderer");
    let viewport = gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
    let texture = renderer.render_texture(&node, Some(&viewport));
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let area = widget.compute_bounds(window).expect("bounds in the window");
    let (x0, y0) = (area.x().max(0.0) as usize, area.y().max(0.0) as usize);
    let x1 = ((area.x() + area.width()) as usize).min(texture.width() as usize);
    let y1 = ((area.y() + area.height()) as usize).min(texture.height() as usize);
    let mut pixels = Vec::new();
    for y in y0..y1 {
        pixels.extend_from_slice(&bytes[y * stride + x0 * 4..y * stride + x1 * 4]);
    }
    pixels
}

/// The brightest channel value `window` paints inside `widget`'s bounds,
/// from a render of the window (text: how strongly it reads).
fn brightest_in(window: &gtk::Window, widget: &impl IsA<gtk::Widget>) -> u8 {
    let (w, h) = (window.width(), window.height());
    let paintable = gtk::WidgetPaintable::new(Some(window));
    let mut node = None;
    for _ in 0..60 {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, w as f64, h as f64);
        node = snapshot.to_node();
        if node.is_some() {
            break;
        }
        window.queue_draw();
        pump(50);
    }
    let node = node.expect("a render of the window");
    let renderer = window.renderer().expect("the window's renderer");
    let viewport = gtk::graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
    let texture = renderer.render_texture(&node, Some(&viewport));
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let area = widget.compute_bounds(window).expect("bounds in the window");
    let (x0, y0) = (area.x().max(0.0) as usize, area.y().max(0.0) as usize);
    let x1 = ((area.x() + area.width()) as usize).min(texture.width() as usize);
    let y1 = ((area.y() + area.height()) as usize).min(texture.height() as usize);
    let mut brightest = 0;
    for y in y0..y1 {
        for x in x0..x1 {
            let px = &bytes[y * stride + x * 4..y * stride + x * 4 + 3];
            brightest = px.iter().copied().fold(brightest, u8::max);
        }
    }
    brightest
}

/// Pumps until `n` frames of `widget`'s window have started (a tick
/// callback keeps them coming; the headless display is slow), up to 10 s.
fn wait_frames(widget: &impl IsA<gtk::Widget>, n: u32) {
    let ticks = std::rc::Rc::new(std::cell::Cell::new(0u32));
    let ticker = {
        let ticks = ticks.clone();
        widget.as_ref().add_tick_callback(move |_, _| {
            ticks.set(ticks.get() + 1);
            glib::ControlFlow::Continue
        })
    };
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while ticks.get() < n && std::time::Instant::now() < deadline {
        pump(50);
    }
    ticker.remove();
}

/// The color `widget` renders at (`x`, `y`), as RGBA bytes.
fn pixel_at(widget: &gtk::Widget, x: i32, y: i32) -> Option<[u8; 4]> {
    let (w, h) = (widget.width(), widget.height());
    if x < 0 || y < 0 || x >= w || y >= h {
        return None;
    }
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let snapshot = gtk::Snapshot::new();
    paintable.snapshot(&snapshot, w as f64, h as f64);
    let node = snapshot.to_node()?;
    let renderer = widget.native().and_then(|n| n.renderer())?;
    let texture = renderer.render_texture(&node, None);
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    // The texture may be scaled (fractional or doubled display scale)
    let sx = texture.width() as f64 / w as f64;
    let sy = texture.height() as f64 / h as f64;
    let (px, py) = ((x as f64 * sx) as usize, (y as f64 * sy) as usize);
    let i = py * stride + px * 4;
    bytes.get(i..i + 4).map(|p| [p[0], p[1], p[2], p[3]])
}

/// Number of distinct opaque-ish colors `widget` renders (rough content check).
fn distinct_colors(widget: &gtk::Widget, tag: &str) -> usize {
    let (w, h) = (widget.width(), widget.height());
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    let mut node = None;
    for _ in 0..60 {
        let snapshot = gtk::Snapshot::new();
        paintable.snapshot(&snapshot, w as f64, h as f64);
        node = snapshot.to_node();
        if node.is_some() {
            break;
        }
        widget.queue_draw();
        pump(50);
    }
    let Some(node) = node else { return 0 };
    let renderer = widget.native().and_then(|n| n.renderer()).unwrap();
    let texture = renderer.render_texture(&node, None);
    if let Some(dir) = std::env::var_os("CANTO_TEST_DUMP_DIR") {
        let _ = texture.save_to_png(std::path::Path::new(&dir).join(format!("{tag}.png")));
    }
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, _stride) = downloader.download_bytes();
    let mut seen = std::collections::HashSet::new();
    for px in bytes.chunks_exact(4) {
        if px[3] > 200 {
            seen.insert((px[0] / 16, px[1] / 16, px[2] / 16));
        }
    }
    seen.len()
}

fn pump(ms: u64) {
    let ctx = glib::MainContext::default();
    let end = std::time::Instant::now() + std::time::Duration::from_millis(ms);
    while std::time::Instant::now() < end {
        ctx.iteration(false);
        std::thread::sleep(std::time::Duration::from_millis(5));
    }
}

/// A warm gradient standing in for a cover.
fn sample_cover() -> gtk::gdk::MemoryTexture {
    let (w, h) = (256usize, 256usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[(200 + x / 8) as u8, (40 + y / 3) as u8, 20, 255]);
        }
    }
    let bytes = glib::Bytes::from_owned(data);
    gtk::gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        w * 4,
    )
}

/// Pumps until `read` gives the same value four times in a row, 100 ms
/// apart (up to 4 s), and returns it.
fn settled<T: PartialEq>(read: impl Fn() -> T) -> T {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(4);
    let mut last = read();
    let mut same = 0;
    while same < 4 && std::time::Instant::now() < deadline {
        pump(100);
        let now = read();
        same = if now == last { same + 1 } else { 0 };
        last = now;
    }
    last
}

/// The widget under `root` (itself included) that its template or builder
/// file names `id`.
fn by_id(root: &gtk::Widget, id: &str) -> Option<gtk::Widget> {
    let mut stack = vec![root.clone()];
    while let Some(widget) = stack.pop() {
        if widget.buildable_id().as_deref() == Some(id) {
            return Some(widget);
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    None
}

/// A text scale of 0.91 (`text-scaling-factor` 0.9091) as GTK's font DPI
/// (96 × 1024 × scale). The header's breakpoint is in sp: at that scale 510
/// sp is 464 px, under what the cover and the buttons beside it take.
const SMALL_TEXT_XFT_DPI: i32 = 89_367;
