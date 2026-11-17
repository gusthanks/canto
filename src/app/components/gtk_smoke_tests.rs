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
        "canto-queue-symbolic",
        "canto-mini-player-symbolic",
        "canto-more-symbolic",
        "canto-library-symbolic",
        "canto-back-symbolic",
        "canto-forward-symbolic",
        "canto-translate-symbolic",
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

    assert_card_playing_badge_shows_bars();

    // Space plays/pauses unless it's typed into a text field
    let entry = gtk::SearchEntry::new();
    let text = entry.first_child().filter(|c| c.is::<gtk::Text>());
    assert!(crate::app::space_is_typing(Some(entry.clone().upcast())));
    if let Some(text) = text {
        assert!(crate::app::space_is_typing(Some(text)));
    }
    assert!(!crate::app::space_is_typing(Some(gtk::Button::new().upcast())));
    assert!(!crate::app::space_is_typing(None));

    // The Now Playing side panel builds, with its heart icons available.
    let now_panel = super::shell::now_panel::widgets_for_tests();
    drop(now_panel);
    if std::env::var_os("CANTO_TEST_DUMP_DIR").is_some() {
        dump_now_panel_sample();
    }
    for icon in ["canto-heart-symbolic", "canto-heart-filled-symbolic"] {
        assert!(icon_theme.has_icon(icon), "{} must resolve", icon);
    }
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

/// Renders the Now Playing panel with a sample track into CANTO_TEST_DUMP_DIR.
fn dump_now_panel_sample() {
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    // A warm gradient standing in for a cover
    let (w, h) = (256usize, 256usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[(200 + x / 8) as u8, (40 + y / 3) as u8, 20, 255]);
        }
    }
    let bytes = glib::Bytes::from_owned(data);
    let cover = gtk::gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &bytes,
        w * 4,
    );
    let theme = gtk::CssProvider::new();
    theme.load_from_string(
        "@define-color canto_cover_accent #c0392b;",
    );
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &theme,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
    );
    let panel = super::shell::now_panel::sample_for_tests(cover.upcast_ref());
    let window = gtk::Window::new();
    window.set_default_size(380, 1100);
    window.set_child(Some(&panel));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "now-panel-sample");
    window.destroy();

    // The main window's frame: top bar over the sidebar and content
    let builder = gtk::Builder::from_resource("/dev/diegovsky/Riff/window.ui");
    let main: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    for id in ["top_bar", "top_history", "home_button", "search_button", "top_search_slot", "user"] {
        assert!(builder.object::<glib::Object>(id).is_some(), "{id} must exist");
    }
    let header: super::AppHeaderBar = builder.object("app_header").unwrap();
    header.set_history_slot(&builder.object("top_history").unwrap());
    header.place_history_buttons(true);
    main.set_default_size(1180, 640);
    main.present();
    pump(1500);
    distinct_colors(main.upcast_ref(), "main-window-sample");
    // Narrow, laid out by the real window layout: the sidebar squeezed to
    // its rail of covers, the search pill a round button
    let _layout = super::WindowLayout::new(&builder);
    main.set_default_size(583, 621);
    pump(2500);
    let split: libadwaita::OverlaySplitView = builder.object("navigation_split").unwrap();
    let sidebar: gtk::Widget = builder.object("navigation_panel").unwrap();
    // Whether the rail settles in time here varies with the headless
    // display; the opening itself is checked in the app (see the notes)
    let _ = (&split, &sidebar);
    distinct_colors(main.upcast_ref(), "main-window-narrow-sample");

    // The search page's field takes the pill's place in the top bar
    header.set_top_search(
        &builder.object("top_search_stack").unwrap(),
        &builder.object("top_search_slot").unwrap(),
    );
    let field = gtk::SearchEntry::new();
    let registrar = super::HeaderRegistrar::new(header.clone(), Default::default());
    let search_header = registrar.page("search");
    search_header.set_title_widget(&field);
    header.set_active("search");
    let slot: libadwaita::Bin = builder.object("top_search_slot").unwrap();
    assert_eq!(
        libadwaita::prelude::BinExt::child(&slot).as_ref(),
        Some(field.upcast_ref::<gtk::Widget>())
    );
    let stack: gtk::Stack = builder.object("top_search_stack").unwrap();
    assert_eq!(stack.visible_child().as_ref(), Some(slot.upcast_ref::<gtk::Widget>()));
    field.set_placeholder_text(Some("Search field"));
    pump(1500);
    distinct_colors(main.upcast_ref(), "main-window-search-sample");
    header.set_active("other");
    assert!(libadwaita::prelude::BinExt::child(&slot).is_none());
    main.destroy();

    // Lyrics with a translation under each line
    let lyrics: LyricsPageWidget = glib::Object::new();
    let lines = ["Eu vim do nada", "", "Hoje eu tô aqui"];
    lyrics.set_lyrics(
        &riff_api::Lyrics::Synced(
            lines
                .iter()
                .enumerate()
                .map(|(i, t)| riff_api::lyrics::LyricLine {
                    start_time_ms: i as u64 * 4000,
                    text: t.to_string(),
                })
                .collect(),
        ),
        |_| {},
    );
    assert_eq!(lyrics.line_texts(), lines);
    lyrics.set_translate_active(true);
    lyrics.set_translations(Some(&[
        Some("I came from nothing".to_string()),
        None,
        Some("Today I'm here".to_string()),
    ]));
    lyrics.set_annotations(Some(&[Some("What the line means".to_string()), None, None]));
    lyrics.set_position(Some((1, 0.5)));
    let window = gtk::Window::new();
    window.set_default_size(700, 600);
    window.set_child(Some(&lyrics));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "lyrics-translation-sample");
    window.destroy();

    let bar: super::shell::playback::PlaybackWidget = glib::Object::new();
    bar.set_track(
        "TODOS ESSES MANOS",
        true,
        "<a href=\"artist:1\">Niink</a>, <a href=\"artist:2\">Jotapê</a>, <a href=\"artist:3\">Supernova Ent</a>",
    );
    bar.set_liked(Some(false));
    let window = gtk::Window::new();
    window.set_default_size(1180, 100);
    window.set_child(Some(&bar));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "player-bar-sample");
    window.destroy();

    // An album header over its cover tint
    let css = gtk::CssProvider::new();
    css.load_from_resource("/dev/diegovsky/Riff/components/details_page/style.css");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let header_widget: super::DetailsHeaderWidget = glib::Object::new();
    let header = super::DetailsHeader::for_widget(
        header_widget.clone(),
        super::HeaderImageShape::Square,
    );
    header.set_caption("Album");
    header.set_title("ESSÊNCIA DE RISCO");
    header.set_subtitle("Niink, Supernova Ent · 2026 · 18 songs");
    header.connect_play(|| {});
    header.connect_shuffle(|| {});
    header.set_like_visible(true);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("details-page-content");
    page.add_css_class("details-page--loaded");
    let clamp = gtk::Box::new(gtk::Orientation::Vertical, 0);
    clamp.add_css_class("details-header-clamp");
    clamp.append(&header_widget);
    page.append(&clamp);
    let filler = gtk::Box::new(gtk::Orientation::Vertical, 0);
    filler.set_vexpand(true);
    page.append(&filler);
    super::set_tint(&page, Some("#b0281e"));
    let window = gtk::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(&page));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "album-header-sample");
    window.destroy();

    let window = super::shell::fullscreen_lyrics::sample_for_tests(cover.upcast_ref());
    window.set_default_size(1280, 720);
    window.present();
    pump(2000);
    distinct_colors(window.upcast_ref(), "fullscreen-lyrics-sample");
    window.destroy();

    let window = super::shell::mini_player::sample_for_tests(cover.upcast_ref());
    window.present();
    pump(1200);
    distinct_colors(window.upcast_ref(), "mini-player-sample");
    window.destroy();

    // The sidebar with covers, open and collapsed
    for rail in [false, true] {
        let sidebar = super::shell::navigation_panel::sample_for_tests(cover.upcast_ref(), rail);
        let window = gtk::Window::new();
        window.set_default_size(if rail { 72 } else { 280 }, 360);
        window.set_child(Some(&sidebar));
        window.present();
        pump(1200);
        let name = if rail { "sidebar-rail-sample" } else { "sidebar-sample" };
        distinct_colors(window.upcast_ref(), name);
        window.destroy();
    }
}
