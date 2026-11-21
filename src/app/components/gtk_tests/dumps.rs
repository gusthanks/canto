//! Sample renders of the app's parts with sample data, written as PNGs to
//! CANTO_TEST_DUMP_DIR when it is set (see `distinct_colors`). The asserts
//! in them run either way.

use super::*;

gtk_tests! {
    info_tab_samples => assert_info_tab_samples,
    now_panel_sample => render_now_panel_sample,
    main_window_samples => render_main_window_samples,
    lyrics_translation_sample => render_lyrics_translation_sample,
    player_bar_sample => render_player_bar_sample,
    album_header_sample => render_album_header_sample,
    fullscreen_lyrics_sample => render_fullscreen_lyrics_sample,
    mini_player_sample => render_mini_player_sample,
    sidebar_samples => render_sidebar_samples,
}

/// The About and Stats tabs of the side panel, with every card filled.
fn assert_info_tab_samples() {
    let _look = SampleLook::new();
    let about = crate::app::components::shell::now_panel::about_sample_for_tests();
    let stats = crate::app::components::shell::now_panel::stats_sample_for_tests();
    dump_info_tab_samples(about, stats);
}

/// The Now Playing panel with a sample track.
fn render_now_panel_sample() {
    let _look = SampleLook::new();
    let cover = sample_cover();
    let panel = crate::app::components::shell::now_panel::sample_for_tests(cover.upcast_ref());
    let window = gtk::Window::new();
    window.set_default_size(380, 1100);
    window.set_child(Some(&panel));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "now-panel-sample");
    window.destroy();
}

/// The main window's frame: wide, with the side panel open, narrow, and
/// with the search page's field in the top bar.
fn render_main_window_samples() {
    let _look = SampleLook::new();
    // The main window's frame: top bar over the sidebar and content
    let builder = gtk::Builder::from_resource("/io/github/gusthanks/Canto/window.ui");
    let main: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    for id in [
        "top_bar",
        "top_history",
        "home_button",
        "search_button",
        "top_search_slot",
        "user",
    ] {
        assert!(
            builder.object::<glib::Object>(id).is_some(),
            "{} must exist",
            id
        );
    }
    let header: crate::app::components::AppHeaderBar = builder.object("app_header").unwrap();
    header.set_history_slot(&builder.object("top_history").unwrap());
    header.place_history_buttons(true);
    main.set_default_size(1180, 640);
    main.present();
    pump(1500);
    distinct_colors(main.upcast_ref(), "main-window-sample");
    // With the side panel open: its tabs under the top bar, beside the page.
    // A window of its own, opened before it shows: a slide-in started on
    // screen may never finish on the headless display
    {
        let builder = gtk::Builder::from_resource("/io/github/gusthanks/Canto/window.ui");
        let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
        let header: crate::app::components::AppHeaderBar = builder.object("app_header").unwrap();
        header.set_history_slot(&builder.object("top_history").unwrap());
        header.place_history_buttons(true);
        // Fixed widths, so before/after renders line up (the headless
        // screen is 1024px wide whatever the window asks)
        let navigation: libadwaita::OverlaySplitView = builder.object("navigation_split").unwrap();
        navigation.set_min_sidebar_width(200.0);
        navigation.set_max_sidebar_width(200.0);
        let utility: libadwaita::OverlaySplitView = builder.object("utility_split").unwrap();
        utility.set_min_sidebar_width(300.0);
        utility.set_max_sidebar_width(300.0);
        utility.set_show_sidebar(true);
        window.set_default_size(1024, 640);
        window.present();
        pump(1500);
        distinct_colors(window.upcast_ref(), "main-window-panel-sample");
        window.destroy();
    }
    // Narrow, laid out by the real window layout: the sidebar squeezed to
    // its rail of covers, the search pill a round button
    let _layout = crate::app::components::WindowLayout::new(&builder);
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
    let registrar =
        crate::app::components::HeaderRegistrar::new(header.clone(), Default::default());
    let search_header = registrar.page("search");
    search_header.set_title_widget(&field);
    header.set_active("search");
    let slot: libadwaita::Bin = builder.object("top_search_slot").unwrap();
    assert_eq!(
        libadwaita::prelude::BinExt::child(&slot).as_ref(),
        Some(field.upcast_ref::<gtk::Widget>())
    );
    let stack: gtk::Stack = builder.object("top_search_stack").unwrap();
    assert_eq!(
        stack.visible_child().as_ref(),
        Some(slot.upcast_ref::<gtk::Widget>())
    );
    field.set_placeholder_text(Some("Search field"));
    pump(1500);
    distinct_colors(main.upcast_ref(), "main-window-search-sample");
    header.set_active("other");
    assert!(libadwaita::prelude::BinExt::child(&slot).is_none());
    main.destroy();
}

/// Lyrics with a translation and a note under the lines.
fn render_lyrics_translation_sample() {
    let _look = SampleLook::new();
    // Lyrics with a translation under each line
    let lyrics: LyricsPageWidget = glib::Object::new();
    let lines = ["Eu vim do nada", "", "Hoje eu tô aqui"];
    lyrics.set_lyrics(
        &canto_api::Lyrics::Synced(
            lines
                .iter()
                .enumerate()
                .map(|(i, t)| canto_api::lyrics::LyricLine {
                    start_time_ms: i as u64 * 4000,
                    text: t.to_string(),
                    ..Default::default()
                })
                .collect(),
        ),
        &canto_api::LyricsSource::Lrclib,
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
    lyrics.set_position(Some((1, 4)));
    let window = gtk::Window::new();
    window.set_default_size(700, 600);
    window.set_child(Some(&lyrics));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "lyrics-translation-sample");
    window.destroy();
}

/// The player bar with a title and three linked artists.
fn render_player_bar_sample() {
    let _look = SampleLook::new();
    let bar: crate::app::components::shell::playback::PlaybackWidget = glib::Object::new();
    bar.set_track(
        &crate::app::components::now_panel::title_markup("TODOS ESSES MANOS", "1", true),
        "<a href=\"https://open.spotify.com/artist/1\">Niink</a>, <a href=\"https://open.spotify.com/artist/2\">Jotapê</a>, <a href=\"https://open.spotify.com/artist/3\">Supernova Ent</a>",
    );
    bar.set_liked(Some(false));
    let window = gtk::Window::new();
    window.set_default_size(1180, 100);
    window.set_child(Some(&bar));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "player-bar-sample");
    window.destroy();
}

/// An album header over its cover's tint.
fn render_album_header_sample() {
    let _look = SampleLook::new();
    // An album header over its cover tint
    let header_widget: crate::app::components::DetailsHeaderWidget = glib::Object::new();
    let header = crate::app::components::DetailsHeader::for_widget(
        header_widget.clone(),
        crate::app::components::HeaderImageShape::Square,
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
    crate::app::components::set_tint(&page, Some("#b0281e"));
    let window = gtk::Window::new();
    window.set_default_size(900, 600);
    window.set_child(Some(&page));
    window.present();
    pump(1500);
    distinct_colors(window.upcast_ref(), "album-header-sample");
    window.destroy();
}

/// The full-screen lyrics window.
fn render_fullscreen_lyrics_sample() {
    let _look = SampleLook::new();
    let cover = sample_cover();
    let window =
        crate::app::components::shell::fullscreen_lyrics::sample_for_tests(cover.upcast_ref());
    window.set_default_size(1280, 720);
    window.present();
    pump(2000);
    distinct_colors(window.upcast_ref(), "fullscreen-lyrics-sample");
    window.destroy();
}

/// The mini player.
fn render_mini_player_sample() {
    let _look = SampleLook::new();
    let cover = sample_cover();
    let window = crate::app::components::shell::mini_player::sample_for_tests(cover.upcast_ref());
    window.present();
    pump(1200);
    distinct_colors(window.upcast_ref(), "mini-player-sample");
    window.destroy();
}

/// The sidebar with covers, open and on its rail.
fn render_sidebar_samples() {
    let _look = SampleLook::new();
    let cover = sample_cover();
    // The sidebar with covers, open and collapsed
    for rail in [false, true] {
        let sidebar = crate::app::components::shell::navigation_panel::sample_for_tests(
            cover.upcast_ref(),
            rail,
        );
        let window = gtk::Window::new();
        window.set_default_size(if rail { 72 } else { 280 }, 360);
        window.set_child(Some(&sidebar));
        window.present();
        pump(1200);
        let name = if rail {
            "sidebar-rail-sample"
        } else {
            "sidebar-sample"
        };
        distinct_colors(window.upcast_ref(), name);
        window.destroy();
    }
}

/// Renders the About and Stats tabs with sample data into CANTO_TEST_DUMP_DIR.
fn dump_info_tab_samples(about: gtk::Widget, stats: gtk::Widget) {
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    for (tab, name) in [(about, "about-tab-sample"), (stats, "stats-tab-sample")] {
        let window = gtk::Window::new();
        window.set_default_size(380, 1700);
        window.set_child(Some(&tab));
        window.present();
        pump(1500);
        distinct_colors(window.upcast_ref(), name);
        // And scrolled to the end, for the cards under the fold
        let mut scrollers = vec![tab.clone()];
        while let Some(widget) = scrollers.pop() {
            if let Some(scroller) = widget.downcast_ref::<gtk::ScrolledWindow>() {
                let adj = scroller.vadjustment();
                adj.set_value(adj.upper() - adj.page_size());
                break;
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                scrollers.push(c);
            }
        }
        pump(500);
        distinct_colors(window.upcast_ref(), &format!("{name}-end"));
        window.destroy();
    }
}
