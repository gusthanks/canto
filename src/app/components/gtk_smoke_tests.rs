use gtk::prelude::*;

use super::{CommandPaletteDialog, LyricsPageWidget, SettingsDialog};

#[test]
fn instantiate_wave2_gtk_widgets_and_icons_when_configured() {
    let Some(gresource_path) = std::env::var_os("CANTO_TEST_GRESOURCE") else {
        return;
    };

    gtk::init().expect("gtk::init should succeed under test display");
    libadwaita::init().expect("libadwaita::init should succeed");

    let res = gio::Resource::load(&gresource_path).unwrap_or_else(|e| {
        panic!(
            "failed to load gresource {}: {e}",
            gresource_path.to_string_lossy()
        )
    });
    gio::resources_register(&res);

    let display = gtk::gdk::Display::default().expect("default GDK display must exist");
    let css = gtk::CssProvider::new();
    css.load_from_resource("/dev/diegovsky/Riff/app.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_resource_path("/dev/diegovsky/Riff/icons");
    assert!(
        icon_theme.has_icon("canto-genius-symbolic"),
        "canto-genius-symbolic icon must resolve via resource icon path"
    );
    for icon in [
        "alarm-symbolic",
        "canto-equalizer-symbolic",
        "canto-lyrics-symbolic",
    ] {
        assert!(
            icon_theme.has_icon(icon),
            "{} icon must resolve via resource icon path",
            icon
        );
    }

    for res_path in [
        "/dev/diegovsky/Riff/window.ui",
        "/dev/diegovsky/Riff/components/shortcuts.ui",
        "/dev/diegovsky/Riff/components/playback_widget.ui",
        "/dev/diegovsky/Riff/components/lyrics_page.ui",
        "/dev/diegovsky/Riff/components/settings.ui",
        "/dev/diegovsky/Riff/components/command_palette.ui",
        "/dev/diegovsky/Riff/components/command_palette.css",
    ] {
        gio::resources_lookup_data(res_path, gio::ResourceLookupFlags::NONE)
            .unwrap_or_else(|e| panic!("missing resource {}: {}", res_path, e));
    }

    super::expose_custom_widgets();

    let playback_type =
        glib::Type::from_name("PlaybackWidget").expect("PlaybackWidget type should be registered");
    let playback = glib::Object::with_type(playback_type);
    drop(playback);

    let lyrics: LyricsPageWidget = glib::Object::new();
    lyrics.set_about_prompt(
        "Test Track",
        "Test Artist",
        Some("Test Album"),
        "https://genius.com/search?q=Test%20Artist%20Test%20Track",
    );
    drop(lyrics);

    let settings: SettingsDialog = glib::Object::new();
    libadwaita::prelude::PreferencesDialogExt::set_visible_page_name(&settings, "integrations");
    assert_eq!(
        libadwaita::prelude::PreferencesDialogExt::visible_page_name(&settings).as_deref(),
        Some("integrations")
    );
    drop(settings);

    let palette: CommandPaletteDialog = glib::Object::new();
    drop(palette);

    // The Genius guide offers a copy button for each value to type in.
    let guide = super::genius_guide_for_tests();
    let mut copy_buttons = 0;
    let mut stack = vec![guide.clone()];
    while let Some(widget) = stack.pop() {
        if widget
            .downcast_ref::<gtk::Button>()
            .is_some_and(|b| b.icon_name().as_deref() == Some("edit-copy-symbolic"))
        {
            copy_buttons += 1;
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    assert_eq!(
        copy_buttons, 3,
        "Genius guide should copy App Name, Website and Redirect URI"
    );
    assert!(
        icon_theme.has_icon("edit-copy-symbolic"),
        "copy icon must resolve"
    );

    assert_sidebar_now_playing_row_paints();
    assert_card_playing_badge_shows_bars();
}

/// The round "playing" badge on album cards must show the bars on top of
/// its background, not just a plain dark disc.
fn assert_card_playing_badge_shows_bars() {
    let css = gtk::CssProvider::new();
    css.load_from_resource("/dev/diegovsky/Riff/components/card.css");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    badge.add_css_class("playback-indicator");
    badge.add_css_class("card__playing");
    badge.set_halign(gtk::Align::Start);
    badge.set_valign(gtk::Align::Start);
    let host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    host.set_size_request(48, 48);
    host.append(&badge);
    let window = gtk::Window::new();
    window.set_child(Some(&host));
    window.present();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !badge.is_mapped() && std::time::Instant::now() < deadline {
        pump(10);
    }
    pump(500);
    let colors = distinct_colors(host.upcast_ref(), "card-playing-badge");
    eprintln!("card playing badge: {} distinct colors", colors);
    // A plain disc is one color; the bars add a second one.
    assert!(
        colors >= 2,
        "card playing badge shows no bars ({} colors)",
        colors
    );
    window.destroy();
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

/// Counts pixels in the first `max_x` columns of `widget` that differ from
/// the row's own background (sampled at its right edge).
fn icon_pixels(widget: &gtk::Widget, max_x: usize, tag: &str) -> usize {
    let (w, h) = (widget.width(), widget.height());
    let paintable = gtk::WidgetPaintable::new(Some(widget));
    // The paintable is empty until the window has drawn a frame; retry briefly.
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
    let Some(node) = node else {
        eprintln!("{tag}: {}x{} produced no render node", w, h);
        return 0;
    };
    let renderer = widget
        .native()
        .and_then(|n| n.renderer())
        .expect("widget should be in a realized window");
    let texture = renderer.render_texture(&node, None);
    if let Some(dir) = std::env::var_os("CANTO_TEST_DUMP_DIR") {
        let _ = texture.save_to_png(std::path::Path::new(&dir).join(format!("{tag}.png")));
    }
    let mut downloader = gtk::gdk::TextureDownloader::new(&texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    bytes
        .chunks_exact(stride)
        .map(|line| {
            let bg = &line[line.len() - 4..];
            line.chunks_exact(4)
                .take(max_x)
                .filter(|px| (0..4).any(|i| px[i].abs_diff(bg[i]) > 24))
                .count()
        })
        .sum()
}

/// Puts the real sidebar "Now Playing" row in a window and returns it.
fn now_playing_row_in_window(idle: bool) -> (gtk::Window, gtk::ListBox, gtk::Widget) {
    let row = super::shell::navigation_panel::now_playing_row_for_tests();
    let listbox = gtk::ListBox::new();
    listbox.add_css_class("navigation-sidebar");
    if idle {
        listbox.add_css_class("now-playing--idle");
    }
    listbox.append(&row);
    let window = gtk::Window::new();
    window.set_default_size(240, 48);
    window.set_child(Some(&listbox));
    window.present();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !row.is_mapped() && std::time::Instant::now() < deadline {
        pump(10);
    }
    assert!(row.is_mapped(), "test window should map");
    pump(1000);
    (window, listbox, row)
}

/// Renders the real sidebar "Now Playing" row while playing and while paused,
/// so a broken indicator (nothing drawn) fails here instead of in the app.
fn assert_sidebar_now_playing_row_paints() {
    for idle in [false, true] {
        let (window, listbox, _row) = now_playing_row_in_window(idle);
        let tag = format!("now-playing-row-idle-{}", idle);
        let painted = icon_pixels(listbox.upcast_ref(), 32, &tag);
        eprintln!("sidebar now playing icon (idle={}): {} px", idle, painted);
        assert!(
            painted > 0,
            "sidebar now playing icon painted nothing (idle={})",
            idle
        );
        window.destroy();
    }
}
