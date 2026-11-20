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
    css.load_from_resource("/io/github/gusthanks/Canto/app.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_resource_path("/io/github/gusthanks/Canto/icons");
    assert!(
        icon_theme.has_icon("canto-genius-symbolic"),
        "the Genius icon must resolve via the resource icon path"
    );
    for icon in [
        "alarm-symbolic",
        "canto-equalizer-symbolic",
        "canto-lyrics-symbolic",
        "canto-queue-symbolic",
        "canto-stats-symbolic",
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
        "/io/github/gusthanks/Canto/window.ui",
        "/io/github/gusthanks/Canto/components/shortcuts.ui",
        "/io/github/gusthanks/Canto/components/playback_widget.ui",
        "/io/github/gusthanks/Canto/components/lyrics_page.ui",
        "/io/github/gusthanks/Canto/components/settings.ui",
        "/io/github/gusthanks/Canto/components/command_palette.ui",
        "/io/github/gusthanks/Canto/components/command_palette.css",
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
    assert_scrollbar_stays_put();
    assert_lyrics_options();
    assert_lyrics_source_line();
    assert_genius_lyrics_switch();

    // Space plays/pauses unless it's typed into a text field
    let entry = gtk::SearchEntry::new();
    let text = entry.first_child().filter(|c| c.is::<gtk::Text>());
    assert!(crate::app::space_is_typing(Some(entry.clone().upcast())));
    if let Some(text) = text {
        assert!(crate::app::space_is_typing(Some(text)));
    }
    assert!(!crate::app::space_is_typing(Some(
        gtk::Button::new().upcast()
    )));
    assert!(!crate::app::space_is_typing(None));

    // The Now Playing side panel builds, with its heart icons available.
    let now_panel = super::shell::now_panel::widgets_for_tests();
    drop(now_panel);
    // The About and Stats tabs build with every card filled
    let about = super::shell::now_panel::about_sample_for_tests();
    let stats = super::shell::now_panel::stats_sample_for_tests();
    for (tab, cards) in [(&about, 7), (&stats, 7)] {
        let stack = tab.downcast_ref::<gtk::Stack>().unwrap();
        assert_eq!(stack.visible_child_name().as_deref(), Some("content"));
        fn find(widget: &gtk::Widget, class: &str) -> Option<gtk::Widget> {
            if widget.has_css_class(class) {
                return Some(widget.clone());
            }
            let mut child = widget.first_child();
            while let Some(c) = child {
                if let Some(found) = find(&c, class) {
                    return Some(found);
                }
                child = c.next_sibling();
            }
            None
        }
        let content = find(tab, "info-page__content").expect("info page content");
        assert_eq!(content.observe_children().n_items(), cards);
    }
    if std::env::var_os("CANTO_TEST_DUMP_DIR").is_some() {
        dump_now_panel_sample();
        dump_info_tab_samples(about, stats);
    }
    assert_lastfm_love_switch();
    assert_recently_played();
    assert_recently_played_opens_at_newest();
    assert_library_keeps_rows_on_play();
    for icon in ["canto-heart-symbolic", "canto-heart-filled-symbolic"] {
        assert!(icon_theme.has_icon(icon), "{} must resolve", icon);
    }
    assert_drawer_layout();
}

/// The round "playing" badge on album cards must show the bars on top of
/// its background, not just a plain dark disc.
fn assert_card_playing_badge_shows_bars() {
    let css = gtk::CssProvider::new();
    css.load_from_resource("/io/github/gusthanks/Canto/components/card.css");
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
    // As CardWidget::set_playing fills it
    crate::app::components::level_bars::fill(&badge, false).set_active(true);
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

/// The page scrollbar keeps its width and place with the pointer on it:
/// libadwaita widens it and pulls it off the edge, on top of the content.
fn assert_scrollbar_stays_put() {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content.set_size_request(300, 3000);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_child(Some(&content));
    let window = gtk::Window::new();
    window.set_default_size(400, 400);
    window.set_child(Some(&scrolled));
    window.present();
    let bar = scrolled.vscrollbar();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !bar.is_mapped() && std::time::Instant::now() < deadline {
        pump(10);
    }
    pump(300);
    let slider = |bar: &gtk::Widget| {
        let mut w = bar.clone();
        while let Some(child) = w.first_child() {
            w = child;
        }
        w
    };
    let measure = |tag: &str| {
        let b = bar.compute_bounds(&window).unwrap();
        let s = slider(&bar).compute_bounds(&window).unwrap();
        eprintln!(
            "scrollbar {tag}: classes {:?} bar x={} w={} slider x={} w={}",
            bar.css_classes(),
            b.x(),
            b.width(),
            s.x(),
            s.width()
        );
        (b.x(), b.width(), s.x(), s.width())
    };
    // The same reading three times in a row, 100 ms apart (up to 5 s): on a
    // busy machine one fixed wait sometimes read the fresh window's layout
    // (a 24 px bar off the overlay) and failed though nothing grew (18/11)
    let settled = |tag: &str| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut last = measure(tag);
        let mut same = 0;
        while same < 3 && std::time::Instant::now() < deadline {
            pump(100);
            let now = measure(tag);
            same = if now == last { same + 1 } else { 0 };
            last = now;
        }
        last
    };
    // One pass of the pointer first: before any, a fresh window still
    // reports the size libadwaita gives a bar without overlay
    bar.add_css_class("hovering");
    pump(500);
    bar.remove_css_class("hovering");
    pump(800);
    // libadwaita adds overlay-indicator a moment after mapping
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !bar.has_css_class("overlay-indicator") && std::time::Instant::now() < deadline {
        pump(10);
    }
    let idle = settled("idle");
    bar.add_css_class("hovering");
    pump(800);
    let hover = settled("hover");
    assert_eq!(idle, hover, "the scrollbar moves or grows on hover");
    window.destroy();
}

/// Word by word or whole line, and the lyrics coming back after a scroll
/// by hand (or staying put).
fn assert_lyrics_options() {
    use super::pages::lyrics::LyricsPageWidget;
    let lyrics: LyricsPageWidget = glib::Object::new();
    let lines: Vec<canto_api::lyrics::LyricLine> = (0..40)
        .map(|i| canto_api::lyrics::LyricLine::new(i * 4000, format!("linha {i} da letra")))
        .collect();
    lyrics.set_lyrics(
        &canto_api::Lyrics::Synced(lines),
        &canto_api::LyricsSource::Lrclib,
        |_| {},
    );
    let window = gtk::Window::new();
    window.set_default_size(500, 400);
    window.set_child(Some(&lyrics));
    window.present();
    pump(600);

    // "linha" lit, the rest dim
    lyrics.set_position(Some((3, 5)));
    let markup = lyrics.line_markup(3).unwrap();
    assert!(
        markup.starts_with("linha<span alpha"),
        "word by word: {}",
        markup
    );
    lyrics.set_word_by_word(false);
    lyrics.set_position(Some((3, 8)));
    let markup = lyrics.line_markup(3).unwrap();
    assert!(!markup.contains("<span"), "whole line: {}", markup);
    lyrics.set_word_by_word(true);
    lyrics.set_position(Some((3, 8)));
    assert!(lyrics.line_markup(3).unwrap().contains("<span"));

    // Follow the song: back by itself a moment after scrolling by hand
    lyrics.set_stay_put(false);
    lyrics.detach();
    assert!(lyrics.is_detached());
    pump(3400);
    assert!(!lyrics.is_detached(), "the lyrics did not come back");
    // Stay put: only the button brings them back
    lyrics.set_stay_put(true);
    lyrics.detach();
    pump(3400);
    assert!(lyrics.is_detached(), "the lyrics came back by themselves");
    lyrics.reattach();
    assert!(!lyrics.is_detached());
    eprintln!("lyrics options ok");
    window.destroy();
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

/// The line saying where the lyrics came from starts on the lines' text
/// edge, sits above the first line with a little room, reads smaller than
/// the lyrics and fits whole.
fn assert_source_line_placed(lyrics: &LyricsPageWidget, tag: &str) {
    let source = lyrics.source_label();
    assert!(source.is_mapped(), "{}: the source line is on screen", tag);
    let first = lyrics.line_label_at(0).expect("a first lyric line");
    // New lines are laid out a frame or more after they are set: on a busy
    // machine a fixed wait read the first line before it had a size (20/11)
    wait_laid_out(&source, lyrics);
    wait_laid_out(&first, lyrics);
    let s = source.compute_bounds(lyrics).expect("source line bounds");
    let f = first.compute_bounds(lyrics).expect("first line bounds");
    let gap = f.y() - (s.y() + s.height());
    eprintln!(
        "{} source line: x={} y={} h={} | first line x={} y={} h={} | gap {}",
        tag,
        s.x(),
        s.y(),
        s.height(),
        f.x(),
        f.y(),
        f.height(),
        gap
    );
    assert!(
        (s.x() - f.x()).abs() <= 1.0,
        "{}: the source line starts at x={}, the lyrics' text at x={}",
        tag,
        s.x(),
        f.x()
    );
    assert!(
        (8.0..=32.0).contains(&gap),
        "{}: {} px between the source line and the first line",
        tag,
        gap
    );
    assert!(
        s.height() < f.height(),
        "{}: the source line ({} px) is not smaller than a lyric line ({} px)",
        tag,
        s.height(),
        f.height()
    );
    assert!(
        !source.layout().is_ellipsized(),
        "{}: the source line is cut",
        tag
    );
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

/// Lyrics without timing read strongly; the source line above them reads
/// too, but quieter.
fn assert_source_line_reads(window: &gtk::Window, lyrics: &LyricsPageWidget, tag: &str) {
    let source = brightest_in(window, &lyrics.source_label());
    let line = brightest_in(window, &lyrics.line_label_at(0).expect("a first line"));
    eprintln!(
        "{} brightness: source line {} | first line {}",
        tag, source, line
    );
    assert!(
        line >= 180,
        "{}: lyrics without timing read dimmed ({})",
        tag,
        line
    );
    assert!(
        source >= 110,
        "{}: the source line is too faint to read ({})",
        tag,
        source
    );
    assert!(
        source <= line.saturating_sub(20),
        "{}: the source line ({}) is not quieter than the lyrics ({})",
        tag,
        source,
        line
    );
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

/// How much lyric line `index` changes under the pointer (the biggest
/// change of any color channel), on screen.
fn hover_change(window: &gtk::Window, lyrics: &LyricsPageWidget, index: usize) -> u8 {
    let label = lyrics.line_label_at(index).expect("a lyric line");
    let row = std::iter::successors(label.parent(), |w| w.parent())
        .find(|w| w.has_css_class("lyric-line"))
        .expect("the line's row");
    // The CSS fades don't run on the test's display: the end state at once
    let gtk_settings = gtk::Settings::default().expect("GTK settings");
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(false);
    let still = pixels_in(window, &row);
    assert!(!still.is_empty(), "the line is off screen");
    row.set_state_flags(gtk::StateFlags::PRELIGHT, false);
    window.queue_draw();
    pump(500);
    let hovered = pixels_in(window, &row);
    row.unset_state_flags(gtk::StateFlags::PRELIGHT);
    window.queue_draw();
    pump(300);
    // Back to how it was before animations come back, or the fade back
    // would never run here and the line would stay lit in later renders
    let back = pixels_in(window, &row);
    gtk_settings.set_gtk_enable_animations(animations);
    let change = |a: &[u8], b: &[u8]| {
        assert_eq!(a.len(), b.len(), "the row kept its size");
        a.iter()
            .zip(b)
            .map(|(x, y)| x.abs_diff(*y))
            .max()
            .unwrap_or(0)
    };
    assert!(
        change(&still, &back) <= 2,
        "line {} stayed lit after the pointer left",
        index
    );
    change(&still, &hovered)
}

/// Lines without timing can't be clicked, so the pointer doesn't light them
/// up.
fn assert_plain_lines_stay_still(window: &gtk::Window, lyrics: &LyricsPageWidget, tag: &str) {
    let change = hover_change(window, lyrics, 0);
    eprintln!(
        "{} line without timing under the pointer: change {}",
        tag, change
    );
    assert!(
        change <= 2,
        "{}: a line without timing lights up under the pointer (change {})",
        tag,
        change
    );
}

/// One quiet line above the lyrics says where they came from, the same for
/// both sources: in the side panel and full screen, never as a lyric line
/// (no index moves), and gone while loading or without lyrics.
fn assert_lyrics_source_line() {
    use super::{PanelPages, UtilityPanelWidget};
    use canto_api::{LyricLine, Lyrics, LyricsSource};

    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    // Invented lines (real lyrics never go in this repository), in stanzas
    // with Genius's blank line between them
    let lines: Vec<String> = [
        "Acordei com o céu ainda cinza",
        "O café esfriando na janela",
        "Contei os carros que passavam",
        "Nenhum deles parou por ela",
        "A cidade fala baixo cedo",
        "Quem escuta aprende a esperar",
        "",
        "Guardei no bolso um mapa velho",
        "De um bairro que mudou de lugar",
        "As ruas trocaram de nome",
        "Mas o vento sabe o caminho",
        "Eu sigo a linha do ônibus",
        "Cantando baixo, sozinho",
        "",
        "E quando a tarde cai devagar",
        "O rádio toca a mesma canção",
        "Ninguém sabe a letra inteira",
        "Mas todo mundo sabe o refrão",
    ]
    .iter()
    .map(|line| line.to_string())
    .collect();
    let genius = LyricsSource::Genius {
        url: "https://genius.com/Sample-artist-sample-song-lyrics".to_string(),
    };

    // As the lyrics page builds it, in the side panel's lyrics tab
    let lyrics = LyricsPageWidget::default();
    lyrics.set_lyrics(&Lyrics::Plain(lines.clone()), &genius, |_| {});
    let source = lyrics.source_label();
    assert_eq!(source.text().as_str(), "Lyrics: Genius · not synced");
    assert!(source.is_visible());
    assert_eq!(lyrics.source(), Some(genius.clone()));
    assert_eq!(
        lyrics.line_texts(),
        lines,
        "the source line is not a lyric line"
    );
    // Translations and Genius's marks land on the right lines of lyrics
    // without timing too
    let mut translations = vec![None; lines.len()];
    translations[0] = Some("I woke up with the sky still gray".to_string());
    lyrics.set_translations(Some(&translations));
    let shown: Vec<String> = with_class(lyrics.upcast_ref(), "lyric-translation")
        .iter()
        .filter(|w| w.is_visible())
        .filter_map(|w| w.downcast_ref::<gtk::Label>().map(|l| l.text().to_string()))
        .collect();
    assert_eq!(shown, vec!["I woke up with the sky still gray".to_string()]);
    let line0 = lyrics.line_label_at(0).unwrap();
    let translation_parent = with_class(lyrics.upcast_ref(), "lyric-translation")
        .into_iter()
        .find(|w| w.is_visible())
        .and_then(|w| w.parent());
    assert_eq!(
        translation_parent,
        line0.parent().and_then(|head| head.parent()),
        "the translation sits under line 0"
    );
    let mut notes = vec![None; lines.len()];
    notes[1] = Some("What the line means".to_string());
    lyrics.set_annotations(Some(&notes));
    let marks = with_class(lyrics.upcast_ref(), "lyric-annotation");
    assert_eq!(marks.len(), 1, "one Genius mark");
    assert_eq!(
        marks[0].parent(),
        lyrics.line_label_at(1).unwrap().parent(),
        "the Genius mark sits on line 1"
    );
    lyrics.set_translations(None);
    lyrics.set_annotations(None);

    let panel: UtilityPanelWidget = glib::Object::new();
    let tab = || Some(gtk::Box::new(gtk::Orientation::Vertical, 0).upcast::<gtk::Widget>());
    panel.set_pages(&PanelPages {
        now: tab(),
        queue: tab(),
        lyrics: Some(lyrics.clone().upcast()),
        about: tab(),
        stats: tab(),
    });
    panel.show_tab("lyrics");
    panel.set_vexpand(true);
    // The side panel's card color
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("view");
    card.append(&panel);
    let window = gtk::Window::new();
    window.set_default_size(420, 900);
    window.set_child(Some(&card));
    window.present();
    wait_mapped(&source, 1200);
    assert_source_line_placed(&lyrics, "side panel, Genius");
    assert_source_line_reads(&window, &lyrics, "side panel, Genius");
    assert_plain_lines_stay_still(&window, &lyrics, "side panel");
    distinct_colors(window.upcast_ref(), "lyrics-genius-sample");

    // LRCLIB, synced: the same line, with the source's name
    let synced: Vec<LyricLine> = lines
        .iter()
        .enumerate()
        .map(|(i, text)| LyricLine::new(i as u64 * 4000, text.clone()))
        .collect();
    lyrics.set_lyrics(&Lyrics::Synced(synced), &LyricsSource::Lrclib, |_| {});
    assert_eq!(source.text().as_str(), "Lyrics: LRCLIB");
    assert!(source.is_visible());
    assert_eq!(lyrics.line_texts(), lines);
    lyrics.set_position(Some((1, 6)));
    pump(800);
    assert_source_line_placed(&lyrics, "side panel, LRCLIB");
    // Lines that jump to their time do light up: the check above can see it
    let change = hover_change(&window, &lyrics, 0);
    eprintln!(
        "side panel, LRCLIB line under the pointer: change {}",
        change
    );
    assert!(change > 2, "a clickable line doesn't light up ({})", change);
    distinct_colors(window.upcast_ref(), "lyrics-lrclib-sample");
    // LRCLIB lyrics without timing say so too
    lyrics.set_lyrics(&Lyrics::Plain(lines.clone()), &LyricsSource::Lrclib, |_| {});
    assert_eq!(source.text().as_str(), "Lyrics: LRCLIB · not synced");

    // No source line while loading or without lyrics
    lyrics.set_loading();
    assert!(!source.is_visible(), "a source line while loading");
    assert_eq!(lyrics.source(), None);
    lyrics.set_lyrics(&Lyrics::Plain(lines.clone()), &genius, |_| {});
    assert!(source.is_visible());
    lyrics.set_empty(
        "No Lyrics Available",
        Some("Lyrics couldn't be found for this song."),
        true,
    );
    assert!(!source.is_visible(), "a source line without lyrics");
    assert_eq!(lyrics.source(), None);
    window.destroy();

    // Full screen: the lyrics are huge, the source line stays small
    let (w, h) = (256usize, 256usize);
    let mut data = Vec::with_capacity(w * h * 4);
    for y in 0..h {
        for x in 0..w {
            data.extend_from_slice(&[(30 + x / 6) as u8, (60 + y / 4) as u8, 120, 255]);
        }
    }
    let cover = gtk::gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        gtk::gdk::MemoryFormat::R8g8b8a8,
        &glib::Bytes::from_owned(data),
        w * 4,
    );
    let (window, lyrics) = super::shell::fullscreen_lyrics::sample_with_lyrics_for_tests(
        cover.upcast_ref(),
        &Lyrics::Plain(lines.clone()),
        &genius,
        "Linha do Ônibus",
        "Artista de Exemplo",
    );
    window.set_default_size(1280, 720);
    window.present();
    let source = lyrics.source_label();
    wait_mapped(&source, 2000);
    assert_eq!(source.text().as_str(), "Lyrics: Genius · not synced");
    assert!(source.is_visible());
    assert_source_line_placed(&lyrics, "full screen, Genius");
    // No line is being sung: they all read, none dimmed waiting its turn
    assert_source_line_reads(&window, &lyrics, "full screen, Genius");
    assert_plain_lines_stay_still(&window, &lyrics, "full screen");
    let first = lyrics.line_label_at(0).unwrap();
    assert!(
        source.height() * 2 < first.height(),
        "full screen: the source line ({} px) is not small next to the lyrics ({} px)",
        source.height(),
        first.height()
    );
    distinct_colors(window.upcast_ref(), "lyrics-fullscreen-genius-sample");
    window.destroy();
    eprintln!("lyrics source line ok");
}

/// "Lyrics When LRCLIB Has None" sits in Integrations, on by default, and
/// turns the Genius fallback key on and off.
fn assert_genius_lyrics_switch() {
    use canto_config::settings::{LYRICS_GENIUS_FALLBACK, SETTINGS};
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let gsettings = gio::Settings::new(SETTINGS);
    gsettings.reset(LYRICS_GENIUS_FALLBACK);
    assert!(gsettings.boolean(LYRICS_GENIUS_FALLBACK), "on by default");
    // What the look-ups get: the token while on (none works too), nothing
    // while off
    {
        use super::genius_fallback;
        use canto_config::settings::GENIUS_ACCESS_TOKEN;
        gsettings.reset(GENIUS_ACCESS_TOKEN);
        assert_eq!(genius_fallback(&gsettings), Some(String::new()));
        gsettings
            .set_string(GENIUS_ACCESS_TOKEN, "  test-token ")
            .expect("a token");
        assert_eq!(genius_fallback(&gsettings), Some("test-token".to_string()));
        gsettings
            .set_boolean(LYRICS_GENIUS_FALLBACK, false)
            .expect("the switch off");
        assert_eq!(genius_fallback(&gsettings), None);
        gsettings.reset(GENIUS_ACCESS_TOKEN);
        gsettings.reset(LYRICS_GENIUS_FALLBACK);
    }
    let dialog = SettingsDialog::new();
    let row = dialog.genius_lyrics_row();
    assert!(row.is_visible() && row.is_active(), "shown and on");
    row.set_active(false);
    pump(50);
    assert!(
        !gsettings.boolean(LYRICS_GENIUS_FALLBACK),
        "the switch turns the Genius fallback off"
    );
    row.set_active(true);
    pump(50);
    assert!(gsettings.boolean(LYRICS_GENIUS_FALLBACK));
    if std::env::var_os("CANTO_TEST_DUMP_DIR").is_some() {
        libadwaita::prelude::PreferencesDialogExt::set_visible_page_name(&dialog, "integrations");
        // A dialog shows only over an Adwaita window
        let window = libadwaita::Window::new();
        window.set_default_size(720, 1000);
        window.present();
        libadwaita::prelude::AdwDialogExt::present(&dialog, Some(&window));
        // The run's first dialog takes a while to show
        wait_mapped(&row, 1500);
        distinct_colors(window.upcast_ref(), "settings-genius-lyrics-sample");
        libadwaita::prelude::AdwDialogExt::force_close(&dialog);
        window.destroy();
    }
    gsettings.reset(LYRICS_GENIUS_FALLBACK);
    eprintln!("genius lyrics switch ok");
}

/// The "Love on Last.fm" switch shows in Integrations only once Last.fm is
/// connected, and starts on.
fn assert_lastfm_love_switch() {
    use canto_config::settings::{LASTFM_SESSION_KEY, LASTFM_USERNAME, SETTINGS};
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let gsettings = gio::Settings::new(SETTINGS);
    let love_row = |dialog: &SettingsDialog| dialog.lastfm_love_row();
    let _ = gsettings.set_string(LASTFM_SESSION_KEY, "");
    let dialog = SettingsDialog::new();
    assert!(
        !love_row(&dialog).is_visible(),
        "hidden until Last.fm is connected"
    );
    drop(dialog);

    let _ = gsettings.set_string(LASTFM_SESSION_KEY, "sample-session");
    let _ = gsettings.set_string(LASTFM_USERNAME, "gusthanks");
    let dialog = SettingsDialog::new();
    let row = love_row(&dialog);
    assert!(row.is_visible(), "shown once Last.fm is connected");
    assert!(row.is_active(), "on by default");
    if std::env::var_os("CANTO_TEST_DUMP_DIR").is_some() {
        libadwaita::prelude::PreferencesDialogExt::set_visible_page_name(&dialog, "integrations");
        // A dialog shows only over an Adwaita window
        let window = libadwaita::Window::new();
        window.set_default_size(720, 1000);
        window.present();
        libadwaita::prelude::AdwDialogExt::present(&dialog, Some(&window));
        pump(1500);
        // Down to the Last.fm rows
        let mut scrolled = row.parent();
        while let Some(w) = scrolled.clone() {
            if let Some(s) = w.downcast_ref::<gtk::ScrolledWindow>() {
                if let Some(point) = row.compute_point(s, &gtk::graphene::Point::new(0.0, 0.0)) {
                    let adj = s.vadjustment();
                    adj.set_value(adj.value() + point.y() as f64 - 420.0);
                }
                break;
            }
            scrolled = w.parent();
        }
        pump(800);
        distinct_colors(window.upcast_ref(), "settings-lastfm-love-sample");
        libadwaita::prelude::AdwDialogExt::force_close(&dialog);
        window.destroy();
    }
    let _ = gsettings.set_string(LASTFM_SESSION_KEY, "");
    let _ = gsettings.set_string(LASTFM_USERNAME, "");
}

/// Recently Played sits folded above the queue; unfolded, it lists songs
/// with their covers.
fn assert_recently_played() {
    use super::{QueuePageWidget, RecentRow};
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let page = QueuePageWidget::default();
    assert!(!page.is_recent_expanded(), "folded at first");
    let queue = gtk::Label::new(Some("(the queue)"));
    queue.set_vexpand(true);
    page.set_queue_list(&queue);
    page.set_queue_empty(false);
    page.set_recent_expanded(true);
    let rows: Vec<RecentRow> = [
        ("TODOS ESSES MANOS", "Niink, Jotapê · Just now"),
        ("Tô Aqui", "Niink · 4 min ago"),
        ("Conexões de Máfia", "Matuê, Rich the Kid · 12 min ago"),
        ("Quer Voar", "Matuê · 1 h ago"),
        ("Kenny G", "Matuê · 3 h ago"),
        ("Vampiro", "Matuê, Teto, WIU · 1 day ago"),
        ("M4", "Teto, Matuê · 2 days ago"),
        ("Groupies", "Teto · 3 days ago"),
    ]
    .iter()
    .enumerate()
    .map(|(i, (title, subtitle))| RecentRow {
        title: title.to_string(),
        subtitle: subtitle.to_string(),
        // Two liked songs: their hearts stay on (the sample shows them)
        liked: i == 2 || i == 5,
    })
    .collect();
    let covers = page.show_recent_rows(&rows);
    assert_eq!(covers.len(), rows.len());
    assert_recent_hearts(&page, &rows);
    page.show_recent_message("Songs you play will show up here.");
    page.show_recent_rows(&rows[..1]);
    let covers = page.show_recent_rows(&rows);
    assert_eq!(covers.len(), rows.len(), "a new list replaces the old one");
    if std::env::var_os("CANTO_TEST_DUMP_DIR").is_some() {
        for (i, cover) in covers.iter().enumerate() {
            let (w, h) = (64usize, 64usize);
            let mut data = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let shade = (i * 28) as u8;
                    data.extend_from_slice(&[
                        200u8.saturating_sub(shade) / 2 + x as u8,
                        40 + shade / 2,
                        60 + (y as u8),
                        255,
                    ]);
                }
            }
            let texture = gtk::gdk::MemoryTexture::new(
                w as i32,
                h as i32,
                gtk::gdk::MemoryFormat::R8g8b8a8,
                &glib::Bytes::from_owned(data),
                w * 4,
            );
            cover.set_paintable(Some(&texture));
        }
        let window = gtk::Window::new();
        window.set_default_size(380, 720);
        window.set_child(Some(&page));
        window.present();
        pump(1500);
        distinct_colors(window.upcast_ref(), "queue-recently-played-sample");
        page.set_recent_expanded(false);
        pump(800);
        distinct_colors(window.upcast_ref(), "queue-recently-played-folded-sample");
        window.destroy();
    }
}

/// Each Recently Played row ends with a heart: full for a liked song, and a
/// click names that row (20/11: liking songs that played before).
fn assert_recent_hearts(page: &super::QueuePageWidget, rows: &[super::RecentRow]) {
    let (_, list_rows) = page.recent_rows_for_tests();
    let hearts: Vec<gtk::Button> = list_rows
        .iter()
        .map(|row| {
            with_class(row.upcast_ref(), "queue-page__recent-like")
                .into_iter()
                .next()
                .and_downcast::<gtk::Button>()
                .expect("every Recently Played row has a heart")
        })
        .collect();
    for (heart, row) in hearts.iter().zip(rows) {
        let icon = heart.icon_name().map(|i| i.to_string()).unwrap_or_default();
        let want = if row.liked {
            "canto-heart-filled-symbolic"
        } else {
            "canto-heart-symbolic"
        };
        assert_eq!(icon, want, "heart of {}", row.title);
        assert_eq!(heart.has_css_class("heart--liked"), row.liked);
    }
    let clicked = std::rc::Rc::new(std::cell::Cell::new(None));
    let seen = clicked.clone();
    page.connect_recent_like(move |index| seen.set(Some(index)));
    hearts[3].emit_clicked();
    assert_eq!(clicked.get(), Some(3), "the heart names its own row");
    // A like made anywhere shows on every row of that song
    let mut liked: Vec<bool> = rows.iter().map(|r| r.liked).collect();
    liked[3] = true;
    page.set_recent_liked(&liked);
    assert_eq!(
        hearts[3].icon_name().as_deref(),
        Some("canto-heart-filled-symbolic")
    );
    assert!(hearts[3].has_css_class("heart--liked"));
    liked[3] = false;
    page.set_recent_liked(&liked);
    assert!(!hearts[3].has_css_class("heart--liked"));
}

/// Unfolded, Recently Played shows its newest song (the last row, next to
/// Now Playing) on screen, not just in the scroll value. 20/11: it opened
/// at the oldest songs and only jumped down on the first scroll.
fn assert_recently_played_opens_at_newest() {
    use super::{QueuePageWidget, RecentRow};
    // Unfolding is animated, and animations don't run on Broadway: without
    // them it unfolds at once
    let gtk_settings = gtk::Settings::default().expect("GTK settings");
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(false);
    let rows: Vec<RecentRow> = (1..=30)
        .map(|i| RecentRow {
            title: format!("Song {i}"),
            subtitle: format!("Artist · {i} h ago"),
            ..Default::default()
        })
        .collect();
    // Unfolded once the rows are there (a recent load is reused), and
    // unfolded first with the rows arriving after (a fresh load)
    for rows_first in [true, false] {
        let page = QueuePageWidget::default();
        let queue = gtk::Label::new(Some("(the queue)"));
        queue.set_vexpand(true);
        page.set_queue_list(&queue);
        page.set_queue_empty(false);
        let window = gtk::Window::new();
        window.set_default_size(380, 720);
        window.set_child(Some(&page));
        window.present();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while !page.is_mapped() && std::time::Instant::now() < deadline {
            pump(10);
        }
        pump(300);
        // What the scroll went through, so a failure explains itself
        let started = std::time::Instant::now();
        let timeline = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let adjustment = page.recent_rows_for_tests().0.vadjustment();
        let note = |what: &'static str| {
            let timeline = timeline.clone();
            move |adj: &gtk::Adjustment| {
                timeline.borrow_mut().push(format!(
                    "{} ms {}: value {} upper {} page {}",
                    started.elapsed().as_millis(),
                    what,
                    adj.value(),
                    adj.upper(),
                    adj.page_size()
                ));
            }
        };
        adjustment.connect_changed(note("changed"));
        adjustment.connect_value_changed(note("value-changed"));
        if rows_first {
            page.show_recent_rows(&rows);
            pump(300);
            page.set_recent_expanded(true);
        } else {
            page.set_recent_expanded(true);
            page.show_recent_loading();
            pump(400);
            page.show_recent_rows(&rows);
        }
        // Frames come slowly on Broadway: wait (up to 5 s) for the newest
        // row to be drawn in view. Without the fix it never is
        let (scroller, list_rows) = page.recent_rows_for_tests();
        let in_view = || {
            let last = list_rows.last()?.compute_bounds(&scroller)?;
            let height = scroller.height() as f32;
            Some(height > 0.0 && last.y() >= -1.0 && last.y() + last.height() <= height + 1.0)
        };
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        pump(300);
        while in_view() != Some(true) && std::time::Instant::now() < deadline {
            pump(50);
        }
        eprintln!(
            "recently played (rows first: {}): settled after {} ms",
            rows_first,
            started.elapsed().as_millis()
        );
        assert_eq!(list_rows.len(), rows.len());
        let height = scroller.height() as f32;
        assert!(
            height > 0.0,
            "the list must be on screen (rows first: {rows_first}; mapped {}, revealed {})",
            scroller.is_mapped(),
            page.is_recent_expanded()
        );
        let last = list_rows.last().unwrap().compute_bounds(&scroller).unwrap();
        let first = list_rows[0].compute_bounds(&scroller).unwrap();
        eprintln!(
            "recently played (rows first: {rows_first}): scroller {height} px, \
             first row y={} last row y={} h={} value={} upper={}",
            first.y(),
            last.y(),
            last.height(),
            scroller.vadjustment().value(),
            scroller.vadjustment().upper(),
        );
        assert!(
            last.y() >= -1.0 && last.y() + last.height() <= height + 1.0,
            "the newest song must be in view when Recently Played opens \
             (rows first: {rows_first}; it is drawn at y={} in a {height} px list); \
             the scroll went:\n{}",
            last.y(),
            timeline.borrow().join("\n")
        );
        assert!(
            first.y() < 0.0,
            "the oldest songs scroll out above (rows first: {})",
            rows_first
        );
        window.destroy();
    }
    gtk_settings.set_gtk_enable_animations(animations);
}

/// A song from another album plays: Recents moves that album up and every
/// other row keeps its widget, and so its cover (20/11: every cover of the
/// library blinked on each play, as all rows were made again).
fn assert_library_keeps_rows_on_play() {
    use super::shell::navigation_panel::{
        update_store_for_tests, NavigationPanelDestination, NavigationPanelItem,
    };
    // Built fresh each time, as the library refill builds them
    let items = |order: &[usize]| -> Vec<NavigationPanelItem> {
        order
            .iter()
            .map(|&i| {
                let item =
                    NavigationPanelItem::from_destination(NavigationPanelDestination::Album {
                        id: i.to_string(),
                        title: format!("Album {i}"),
                    });
                item.set_image(format!("https://example.com/{i}.jpg"));
                item.set_subtitle(format!("Album • Artist {i}"));
                item
            })
            .collect()
    };
    let store = gtk::gio::ListStore::new::<NavigationPanelItem>();
    let listbox = gtk::ListBox::new();
    listbox.bind_model(Some(&store), |obj| {
        let item = obj.downcast_ref::<NavigationPanelItem>().unwrap();
        gtk::Label::new(Some(&item.title())).upcast()
    });
    let rows = |n: i32| -> Vec<gtk::ListBoxRow> {
        (0..n).filter_map(|i| listbox.row_at_index(i)).collect()
    };
    update_store_for_tests(&store, &items(&[0, 1, 2, 3, 4, 5, 6, 7]));
    let before = rows(8);
    assert_eq!(before.len(), 8);
    // Album 5 plays
    update_store_for_tests(&store, &items(&[5, 0, 1, 2, 3, 4, 6, 7]));
    let after = rows(8);
    let titles: Vec<String> = (0..store.n_items())
        .filter_map(|i| store.item(i).and_downcast::<NavigationPanelItem>())
        .map(|item| item.title())
        .collect();
    assert_eq!(
        titles,
        ["Album 5", "Album 0", "Album 1", "Album 2", "Album 3", "Album 4", "Album 6", "Album 7"]
    );
    for (old, new) in [(0, 1), (1, 2), (2, 3), (3, 4), (4, 5), (6, 6), (7, 7)] {
        assert_eq!(
            after[new], before[old],
            "the row of Album {old} must stay the same widget (no blink)"
        );
    }
    // Refilled with nothing new: nothing is made again
    update_store_for_tests(&store, &items(&[5, 0, 1, 2, 3, 4, 6, 7]));
    assert_eq!(rows(8), after, "an unchanged library keeps all its rows");
    // A renamed album is made again, alone
    let renamed = items(&[5, 0, 1, 2, 3, 4, 6, 7]);
    renamed[3].set_title("Album 2 (Deluxe)".to_string());
    update_store_for_tests(&store, &renamed);
    let now = rows(8);
    assert_ne!(now[3], after[3], "the renamed row shows its new name");
    for i in [0, 1, 2, 4, 5, 6, 7] {
        assert_eq!(now[i], after[i], "row {i} stays");
    }
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

/// Renders the Now Playing panel with a sample track into CANTO_TEST_DUMP_DIR.
fn dump_now_panel_sample() {
    libadwaita::StyleManager::default().set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let cover = sample_cover();
    let theme = gtk::CssProvider::new();
    theme.load_from_string("@define-color canto_cover_accent #c0392b;");
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
    let header: super::AppHeaderBar = builder.object("app_header").unwrap();
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
        let header: super::AppHeaderBar = builder.object("app_header").unwrap();
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

    let bar: super::shell::playback::PlaybackWidget = glib::Object::new();
    bar.set_track(
        &super::now_panel::title_markup("TODOS ESSES MANOS", "1", true),
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

    // An album header over its cover tint
    let css = gtk::CssProvider::new();
    css.load_from_resource("/io/github/gusthanks/Canto/components/details_page/style.css");
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().unwrap(),
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
    let header_widget: super::DetailsHeaderWidget = glib::Object::new();
    let header =
        super::DetailsHeader::for_widget(header_widget.clone(), super::HeaderImageShape::Square);
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
        let name = if rail {
            "sidebar-rail-sample"
        } else {
            "sidebar-sample"
        };
        distinct_colors(window.upcast_ref(), name);
        window.destroy();
    }

    // Section titles stay out of the grid (a FlowBox with a model ignores
    // filter functions: the model itself must be filtered)
    let titles = super::shell::navigation_panel::grid_titles_for_tests();
    assert_eq!(titles.len(), 2, "only covers in the grid, got {:?}", titles);
    assert!(
        titles.iter().any(|t| t == "Matuê"),
        "the artist is in the grid: {:?}",
        titles
    );

    // "View As" > Grid: three covers side by side in the usual width
    let grid = super::shell::navigation_panel::grid_sample_for_tests(cover.upcast_ref());
    let window = gtk::Window::new();
    window.set_default_size(264, 420);
    window.set_child(Some(&grid));
    window.present();
    pump(1200);
    fn find_flow(widget: &gtk::Widget) -> Option<gtk::FlowBox> {
        if let Some(flow) = widget.downcast_ref::<gtk::FlowBox>() {
            return Some(flow.clone());
        }
        let mut child = widget.first_child();
        while let Some(w) = child {
            if let Some(found) = find_flow(&w) {
                return Some(found);
            }
            child = w.next_sibling();
        }
        None
    }
    let flow = find_flow(&grid).expect("the grid sample holds a FlowBox");
    let first = flow.child_at_index(0).expect("a first cell");
    let third = flow.child_at_index(2).expect("a third cell");
    // The scrolled area fills the window, so the second row shows whole
    let scrolled = flow
        .ancestor(gtk::ScrolledWindow::static_type())
        .expect("the grid scrolls");
    let fourth = flow.child_at_index(3).expect("a fourth cell");
    let row2 = fourth
        .compute_bounds(&scrolled)
        .expect("fourth cell bounds");
    assert!(
        scrolled.height() > 300 && row2.y() + row2.height() <= scrolled.height() as f32,
        "the grid's area fills the sidebar (area {} tall, second row ends at {})",
        scrolled.height(),
        row2.y() + row2.height()
    );
    let (a, c) = (
        first.compute_bounds(&flow).expect("first cell bounds"),
        third.compute_bounds(&flow).expect("third cell bounds"),
    );
    assert!(
        (a.y() - c.y()).abs() < 1.0,
        "three covers fit in one row of the usual sidebar width (first at {}, third at {})",
        a.y(),
        c.y()
    );
    distinct_colors(window.upcast_ref(), "sidebar-grid-sample");
    window.destroy();
}

/// The widths the shell checks lay the main window out at, and whether the
/// side panel is open: half (572) and all (1200) of a 1920 px screen at
/// scale 1.6, each side of 1100 (under it the panel floats as a drawer) and
/// of 550 (at it or less the panel fills the window), and 1366, where the
/// full library leaves the page beside the panel 656 or more.
const SHELL_SHOTS: [(i32, bool); 11] = [
    (550, true),
    (572, false),
    (572, true),
    (583, true),
    (760, true),
    (960, true),
    (1099, true),
    (1100, true),
    (1200, true),
    (1366, true),
    (1920, true),
];

/// The main window laid out by the real `WindowLayout`, with an album page,
/// Now Playing in the side panel and a track in the player bar.
struct Shell {
    builder: gtk::Builder,
    window: libadwaita::ApplicationWindow,
    /// What is rendered: the window, or the box standing in for a window
    /// wider than the headless screen
    frame: gtk::Widget,
    _layout: std::rc::Rc<super::WindowLayout>,
    panel: super::UtilityPanel,
    page: super::DetailsPage,
}

/// Where the shell's parts ended up (px).
#[derive(Clone, Copy, Debug, PartialEq)]
struct ShellShot {
    width: i32,
    open: bool,
    collapsed: bool,
    panel_shown: bool,
    max_panel: f64,
    /// The utility split: the whole window's width
    window_split: i32,
    /// The navigation split: what the side panel leaves to library and page
    page_split: i32,
    library: i32,
    rail: bool,
    page_card: i32,
    panel_card: i32,
    /// Where the side panel's card ends, in the utility split
    panel_end: f32,
}

impl ShellShot {
    fn tag(&self) -> String {
        format!("{}{}", self.width, if self.open { "-open" } else { "" })
    }

    fn mode(&self) -> &'static str {
        if !self.collapsed {
            "beside"
        } else if self.max_panel >= f64::from(i32::MAX) {
            "fills the window"
        } else {
            "drawer"
        }
    }
}

/// The library's show/hide action, as `Navigation` adds it to the window in
/// the app: the layout hides and brings back the library through it.
fn add_library_action(
    window: &libadwaita::ApplicationWindow,
    split: &libadwaita::OverlaySplitView,
) {
    use gio::prelude::{ActionExt, ActionMapExt};
    let action = gio::SimpleAction::new_stateful(
        "show-navigation-panel",
        None,
        &split.shows_sidebar().to_variant(),
    );
    let hidden = std::rc::Rc::new(std::cell::Cell::new(false));
    action.connect_change_state(clone!(
        #[weak]
        split,
        #[strong]
        hidden,
        move |_, state| {
            let show = state.and_then(|s| s.get::<bool>()).unwrap_or(true);
            if !split.is_collapsed() {
                hidden.set(!show);
            }
            split.set_show_sidebar(show);
        }
    ));
    split.connect_collapsed_notify(move |split| {
        split.set_show_sidebar(!split.is_collapsed() && !hidden.get());
    });
    split.connect_show_sidebar_notify(clone!(
        #[weak]
        action,
        move |split| {
            let shown = split.shows_sidebar();
            if action.state().and_then(|s| s.get::<bool>()) != Some(shown) {
                action.set_state(&shown.to_variant());
            }
        }
    ));
    window.add_action(&action);
}

/// Lays the main window out at `width` with the side panel open or not.
fn render_shell(width: i32, open: bool) -> Shell {
    use libadwaita::prelude::AdwApplicationWindowExt;
    let builder = gtk::Builder::from_resource("/io/github/gusthanks/Canto/window.ui");
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    let navigation: libadwaita::OverlaySplitView = builder.object("navigation_split").unwrap();
    add_library_action(&window, &navigation);
    // The library's filter pills, as `NavigationPanel` adds them: they give
    // the full library its least width, and hide on the rail of covers
    let sidebar: gtk::Widget = builder.object("navigation_panel").unwrap();
    let library_header: gtk::Widget = builder.object("navigation_panel_header").unwrap();
    let filters = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    filters.add_css_class("library-filters");
    for label in ["Playlists", "Albums", "Artists"] {
        let pill = gtk::ToggleButton::with_label(label);
        pill.add_css_class("pill");
        pill.add_css_class("library-filter");
        filters.append(&pill);
    }
    library_header
        .parent()
        .and_downcast::<gtk::Box>()
        .expect("the library's box")
        .insert_child_after(&filters, Some(&library_header));
    sidebar.connect_css_classes_notify(move |sidebar| {
        filters.set_visible(!sidebar.has_css_class("navigation-panel--rail"));
    });
    // The layout's first fit reads the window's default width
    window.set_default_size(width, 621);
    let layout = super::WindowLayout::new(&builder);
    let panel = super::UtilityPanel::new(
        builder.object("utility_panel").unwrap(),
        builder.object("utility_split").unwrap(),
        std::rc::Rc::clone(&layout),
    );
    let cover = sample_cover();
    let tab = |name: &str| Some(gtk::Label::new(Some(name)).upcast::<gtk::Widget>());
    panel.set_pages(&super::PanelPages {
        now: Some(super::shell::now_panel::sample_for_tests(
            cover.upcast_ref(),
        )),
        queue: tab("Queue"),
        lyrics: tab("Lyrics"),
        about: tab("About"),
        stats: tab("Stats"),
    });
    let page = super::DetailsPage::sample_for_tests(cover.upcast_ref());
    let stack: gtk::Stack = builder.object("content_stack").unwrap();
    stack.add_named(page.widget(), Some("album"));
    stack.set_visible_child_name("album");
    let bar: super::shell::playback::PlaybackWidget = builder.object("playback").unwrap();
    bar.set_track(
        &super::now_panel::title_markup("MARIA BONITA SEM LAMPIÃO", "1", true),
        "<a href=\"https://open.spotify.com/artist/1\">Niink</a>, <a href=\"https://open.spotify.com/artist/2\">Supernova Ent</a>",
    );
    bar.set_liked(Some(true));
    // Opened before it shows: a slide-in started on screen may never finish
    // on the headless display
    panel.set_open(open);
    let frame = if width > 1024 {
        // Wider than the headless screen: the window's content moves into a
        // box that wide, in a scrolled window. The layout reads the split's
        // own width, so it lays out as in a window that wide
        let content = window.content().expect("the window's content");
        window.set_content(None::<&gtk::Widget>);
        let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        host.set_size_request(width, 621);
        content.set_vexpand(true);
        host.append(&content);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&host));
        window.set_content(Some(&scroller));
        host.upcast()
    } else {
        window.clone().upcast()
    };
    window.present();
    wait_mapped(&frame, 1500);
    Shell {
        builder,
        window,
        frame,
        _layout: layout,
        panel,
        page,
    }
}

/// Reads where the shell's parts are, once they stay put (up to 5 s).
fn shoot_shell(shell: &Shell, width: i32, open: bool) -> ShellShot {
    let b = &shell.builder;
    let utility: libadwaita::OverlaySplitView = b.object("utility_split").unwrap();
    let navigation: libadwaita::OverlaySplitView = b.object("navigation_split").unwrap();
    let library: gtk::Widget = b.object("navigation_panel").unwrap();
    let panel: gtk::Widget = b.object("utility_panel").unwrap();
    let page = navigation.content().expect("the page");
    // A card is the child of its pane: the pane's margins are the gaps
    let card = |child: &gtk::Widget| if child.is_mapped() { child.width() } else { 0 };
    let read = || ShellShot {
        width,
        open,
        collapsed: utility.is_collapsed(),
        panel_shown: utility.shows_sidebar(),
        max_panel: utility.max_sidebar_width(),
        window_split: utility.width(),
        page_split: navigation.width(),
        library: if navigation.shows_sidebar() {
            card(&library)
        } else {
            0
        },
        rail: library.has_css_class("navigation-panel--rail"),
        page_card: card(&page),
        panel_card: if utility.shows_sidebar() {
            card(&panel)
        } else {
            0
        },
        panel_end: panel
            .compute_bounds(&utility)
            .map_or(0.0, |b| b.x() + b.width()),
    };
    // Laid out at that width, then unchanged for 1.2 s: a frame of the
    // headless display can take a second, and a refit takes one more
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut last = read();
    let mut same = 0;
    while same < 8 && std::time::Instant::now() < deadline {
        pump(150);
        let now = read();
        same = if now == last && now.window_split == width {
            same + 1
        } else {
            0
        };
        last = now;
    }
    last
}

/// Under 1100 px the side panel floats over the page as a drawer (at most
/// 360 wide, leaving 96 of the window uncovered) and the page keeps the
/// whole width; from 1100 up it sits beside the page as before, with the
/// library on its rail of covers where the full library would leave the
/// page card under 656 px; at 550 or less it fills the window, as before.
fn assert_drawer_layout() {
    let style = libadwaita::StyleManager::default();
    let scheme = style.color_scheme();
    style.set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let gtk_settings = gtk::Settings::default().unwrap();
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(false);
    let mut shots = Vec::new();
    for (width, open) in SHELL_SHOTS.iter().copied() {
        let shell = render_shell(width, open);
        let shot = shoot_shell(&shell, width, open);
        distinct_colors(
            &shell.frame,
            &format!("layout-{:04}{}", width, if open { "-open" } else { "" }),
        );
        shell.window.destroy();
        shots.push(shot);
    }
    for s in &shots {
        eprintln!(
            "layout {}: side panel {} {} (shown {}, max {}), library {}{}, page card {}, page split {} of {}",
            s.tag(),
            s.mode(),
            s.panel_card,
            s.panel_shown,
            s.max_panel,
            s.library,
            if s.rail { " (rail)" } else { "" },
            s.page_card,
            s.page_split,
            s.window_split
        );
    }
    assert_drawer_follows_resizes();
    assert_library_rails_beside_panel();
    assert_drawer_behavior();
    gtk_settings.set_gtk_enable_animations(animations);
    style.set_color_scheme(scheme);
    // What the side panel leaves to the page, first
    for s in &shots {
        if s.width > 550 && s.width < 1100 {
            assert!(
                s.page_split >= s.window_split - 4,
                "{}: the side panel squeezes the page: page card {} beside a side panel of {} in a {} px window",
                s.tag(),
                s.page_card,
                s.panel_card,
                s.width
            );
        } else if s.width >= 1100 {
            assert!(
                s.page_card >= 656,
                "{}: the page card is {} px beside the side panel and a library of {}{}",
                s.tag(),
                s.page_card,
                s.library,
                if s.rail { " (rail)" } else { "" }
            );
            // With the full library it would be 446 at 1100 and 546 at
            // 1200, but 712 at 1366
            let rail = [1100, 1200].contains(&s.width);
            let library = if rail {
                s.library == 60
            } else {
                s.library > 200
            };
            assert!(
                s.rail == rail && library,
                "{}: beside the side panel the library is {} px{}, not {}",
                s.tag(),
                s.library,
                if s.rail { " (rail)" } else { "" },
                if rail { "its rail of covers" } else { "full" }
            );
        }
    }
    for s in &shots {
        assert_eq!(
            s.collapsed,
            s.width < 1100,
            "{}: the side panel floats over the page under 1100 px only",
            s.tag()
        );
        assert_eq!(
            s.panel_shown,
            s.open,
            "{}: the side panel stays as it was opened",
            s.tag()
        );
        if s.width <= 550 {
            assert_eq!(
                s.max_panel,
                f64::from(i32::MAX),
                "{}: the side panel fills a window this narrow",
                s.tag()
            );
        } else if s.width < 1100 {
            assert_eq!(
                s.max_panel,
                f64::from(360.min(s.width - 96)),
                "{}: the drawer is 360 px or leaves 96 px of the window",
                s.tag()
            );
            if s.open {
                assert!(
                    s.panel_card > 0 && s.panel_end <= s.window_split as f32 + 0.5,
                    "{}: the drawer shows whole (card {} px, ends at {} of {})",
                    s.tag(),
                    s.panel_card,
                    s.panel_end,
                    s.window_split
                );
            }
        }
    }
    eprintln!("layout drawer ok");
}

/// Narrowing a window under 1100 with the side panel open closes the panel
/// and gives the page the whole width; widening it back brings the panel
/// back beside the page. And a library hidden to make room for the panel
/// beside the page (here under a page header too wide for both) comes back
/// once the panel floats. In the box standing in for a window wider than
/// the headless screen, so the panel can sit beside the page.
fn assert_drawer_follows_resizes() {
    let shell = render_shell(1200, true);
    let resize = |width: i32| {
        shell.frame.set_size_request(width, 621);
        shoot_shell(&shell, width, true)
    };
    let docked = shoot_shell(&shell, 1200, true);
    let narrowed = resize(1050);
    let widened = resize(1200);
    shell.window.destroy();

    let shell = render_shell(1200, false);
    let header: super::AppHeaderBar = shell.builder.object("app_header").unwrap();
    let registrar = super::HeaderRegistrar::new(header.clone(), Default::default());
    let wide = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    wide.set_size_request(760, -1);
    registrar.page("wide").add_end(&wide);
    header.set_active("wide");
    pump(300);
    shell.panel.set_open(true);
    let hidden = shoot_shell(&shell, 1200, true);
    shell.frame.set_size_request(1050, 621);
    let floating = shoot_shell(&shell, 1050, true);
    shell.window.destroy();

    for (step, s) in [
        ("docked", &docked),
        ("narrowed", &narrowed),
        ("widened", &widened),
        ("wide header, docked", &hidden),
        ("wide header, narrowed", &floating),
    ] {
        eprintln!(
            "layout resize, {} at {}: side panel {} {} (shown {}), library {}, page split {} of {}",
            step,
            s.width,
            s.mode(),
            s.panel_card,
            s.panel_shown,
            s.library,
            s.page_split,
            s.window_split
        );
    }
    assert!(
        !docked.collapsed && docked.panel_shown,
        "1200: the side panel sits open beside the page: {:?}",
        docked
    );
    assert!(
        narrowed.window_split == 1050
            && narrowed.collapsed
            && !narrowed.panel_shown
            && narrowed.page_split >= narrowed.window_split - 4,
        "narrowed to 1050 with the side panel open, it closes and the page takes the whole width: {:?}",
        narrowed
    );
    assert!(
        !widened.collapsed && widened.panel_shown,
        "widened back to 1200, the side panel is back beside the page: {:?}",
        widened
    );
    assert!(
        hidden.panel_shown && hidden.library == 0,
        "1200 under a wide page header: opening the side panel hides the library: {:?}",
        hidden
    );
    assert!(
        floating.collapsed && floating.library > 0,
        "narrowed to 1050, the side panel floats and the library hidden for it comes back: {:?}",
        floating
    );
}

/// With the side panel beside the page (1100 px and up), a full library that
/// would leave the page card under 656 px shrinks to its rail of covers on
/// its own, and comes back when the panel closes or the window grows enough,
/// without touching the saved choice (navigation-rail). The library icon on
/// that rail brings the full library back beside the panel until the panel
/// closes; a library collapsed by hand stays on its rail.
fn assert_library_rails_beside_panel() {
    let settings = gio::Settings::new(canto_config::settings::SETTINGS);
    let saved = || settings.boolean(canto_config::settings::NAVIGATION_RAIL);
    let saved_at_start = saved();
    let shell = render_shell(1366, true);
    let resize = |width: i32| {
        shell.frame.set_size_request(width, 621);
        shoot_shell(&shell, width, true)
    };
    let panel_open = |open: bool| {
        shell.panel.set_open(open);
        shoot_shell(&shell, 1200, open)
    };
    // The rail's library icon, and the full library's collapse button
    let library_button = || {
        WidgetExt::activate_action(&shell.window, "win.sidebar-rail", None)
            .expect("the library's rail action");
    };
    let wide = shoot_shell(&shell, 1366, true);
    let narrowed = resize(1200);
    let widened = resize(1366);
    let squeezed = resize(1200);
    let closed = panel_open(false);
    let reopened = panel_open(true);
    library_button();
    let by_hand = shoot_shell(&shell, 1200, true);
    let saved_by_hand = saved();
    distinct_colors(&shell.frame, "layout-1200-open-library-by-hand");
    let by_hand_closed = panel_open(false);
    let by_hand_reopened = panel_open(true);
    // Expanded by hand, then collapsed with its own button: that rail is the
    // user's, and saved
    library_button();
    library_button();
    let chosen = shoot_shell(&shell, 1200, true);
    let saved_chosen = saved();
    let chosen_closed = panel_open(false);
    // The full library again, as the other checks expect
    library_button();
    let saved_at_end = saved();
    shell.window.destroy();

    let steps = [
        ("1366", &wide),
        ("narrowed to 1200", &narrowed),
        ("widened to 1366", &widened),
        ("narrowed to 1200 again", &squeezed),
        ("side panel closed", &closed),
        ("side panel reopened", &reopened),
        ("library icon", &by_hand),
        ("expanded, side panel closed", &by_hand_closed),
        ("expanded, side panel reopened", &by_hand_reopened),
        ("collapsed by hand", &chosen),
        ("collapsed by hand, side panel closed", &chosen_closed),
    ];
    for (step, s) in steps.iter() {
        eprintln!(
            "layout library, {} at {}: side panel {} {} (shown {}), library {}{}, page card {}",
            step,
            s.width,
            s.mode(),
            s.panel_card,
            s.panel_shown,
            s.library,
            if s.rail { " (rail)" } else { "" },
            s.page_card
        );
    }
    eprintln!(
        "layout library, saved rail: {} at start, {} after the library icon, {} collapsed by hand, {} at the end",
        saved_at_start, saved_by_hand, saved_chosen, saved_at_end
    );
    let full = |s: &ShellShot| !s.rail && s.library > 200;
    let rail = |s: &ShellShot| s.rail && s.library == 60;
    let docked = |s: &ShellShot| !s.collapsed && s.panel_shown;
    for (step, s, want_rail) in [
        ("1366", &wide, false),
        ("narrowed to 1200", &narrowed, true),
        ("widened to 1366", &widened, false),
        ("narrowed to 1200 again", &squeezed, true),
        ("side panel reopened", &reopened, true),
    ]
    .iter()
    {
        let s: &ShellShot = s;
        let library = if *want_rail { rail(s) } else { full(s) };
        assert!(
            docked(s) && library && s.page_card >= 656,
            "{}: beside the side panel the library is {} and the page card at least 656: {:?}",
            step,
            if *want_rail {
                "its rail of covers"
            } else {
                "full"
            },
            s
        );
    }
    assert!(
        !closed.panel_shown && full(&closed),
        "side panel closed: the full library comes back: {:?}",
        closed
    );
    assert!(
        docked(&by_hand) && full(&by_hand),
        "the library icon brings the full library back beside the side panel: {:?}",
        by_hand
    );
    assert!(
        full(&by_hand_closed) && docked(&by_hand_reopened) && rail(&by_hand_reopened),
        "expanded by hand, the library stays full until the side panel closes, and shrinks again when it reopens: {:?} then {:?}",
        by_hand_closed,
        by_hand_reopened
    );
    assert!(
        rail(&chosen) && rail(&chosen_closed),
        "collapsed by hand, the library stays on its rail when the side panel closes: {:?} then {:?}",
        chosen,
        chosen_closed
    );
    assert!(
        !saved_at_start && !saved_by_hand && saved_chosen && !saved_at_end,
        "only the user's own collapse is saved (navigation-rail): {} at start, {} after the library icon, {} collapsed by hand, {} at the end",
        saved_at_start,
        saved_by_hand,
        saved_chosen,
        saved_at_end
    );
}

/// Where the main window's keyboard focus is.
#[derive(Clone, Copy, Debug, PartialEq)]
enum FocusIn {
    SidePanel,
    Page,
    Library,
    Elsewhere,
    Nowhere,
}

/// What is open over (or beside) the page, where the keyboard focus is,
/// and how far the page is scrolled (px).
#[derive(Clone, Copy, Debug, PartialEq)]
struct Overlays {
    /// The side panel is open...
    panel: bool,
    /// ...and floats over the page when it is (drawer, or the whole window)
    floats: bool,
    /// The full library is open over the page
    library_over_page: bool,
    /// The library is on its rail of covers
    rail: bool,
    /// The queue sheet is open
    sheet: bool,
    focus: FocusIn,
    page_scroll: i32,
}

/// The album page's scrolled window.
fn page_scroller(shell: &Shell) -> gtk::ScrolledWindow {
    with_class(shell.page.widget().upcast_ref(), "details-page-scroll")
        .into_iter()
        .next()
        .and_downcast::<gtk::ScrolledWindow>()
        .expect("the page's scrolled window")
}

fn read_overlays(shell: &Shell, scroller: &gtk::ScrolledWindow) -> Overlays {
    let b = &shell.builder;
    let utility: libadwaita::OverlaySplitView = b.object("utility_split").unwrap();
    let navigation: libadwaita::OverlaySplitView = b.object("navigation_split").unwrap();
    let sheet: libadwaita::BottomSheet = b.object("bottom_sheet").unwrap();
    let library: gtk::Widget = b.object("navigation_panel").unwrap();
    let panel: gtk::Widget = b.object("utility_panel").unwrap();
    let focus = match GtkWindowExt::focus(&shell.window) {
        None => FocusIn::Nowhere,
        Some(f) if f == panel || f.is_ancestor(&panel) => FocusIn::SidePanel,
        Some(f) if f.is_ancestor(shell.page.widget()) => FocusIn::Page,
        Some(f) if f.is_ancestor(&library) => FocusIn::Library,
        Some(_) => FocusIn::Elsewhere,
    };
    Overlays {
        panel: utility.shows_sidebar(),
        floats: utility.is_collapsed(),
        library_over_page: navigation.is_collapsed() && navigation.shows_sidebar(),
        rail: library.has_css_class("navigation-panel--rail"),
        sheet: sheet.is_open(),
        focus,
        page_scroll: scroller.vadjustment().value().round() as i32,
    }
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

/// Canto's Esc for the side panel floating over the page: the action of
/// the Escape shortcut in its own controller on the window, which bubbles
/// (text fields and popovers keep their Esc).
fn canto_escape(window: &impl IsA<gtk::Widget>) -> Option<gtk::ShortcutAction> {
    let controllers = window.as_ref().observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| {
            controllers
                .item(i)
                .and_downcast::<gtk::ShortcutController>()
        })
        .filter(|c| {
            c.name().as_deref() == Some("canto-floating-panel-escape")
                && c.propagation_phase() == gtk::PropagationPhase::Bubble
        })
        .flat_map(|c| {
            (0..c.n_items())
                .filter_map(|j| c.item(j).and_downcast::<gtk::Shortcut>())
                .collect::<Vec<_>>()
        })
        .find(|s| s.trigger().is_some_and(|t| t.to_str() == "Escape"))
        .and_then(|s| s.action())
}

/// Canto's click on the page dimmed under the floating side panel: its
/// gesture on the split view, before libadwaita's own (capture).
fn canto_click(split: &impl IsA<gtk::Widget>) -> Option<gtk::GestureClick> {
    let controllers = split.as_ref().observe_controllers();
    (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::GestureClick>())
        .find(|c| {
            c.name().as_deref() == Some("canto-floating-panel-click")
                && c.propagation_phase() == gtk::PropagationPhase::Capture
        })
}

/// The side panel floating over the page (under 1100 px) closes with its
/// button, with Esc and with a click on the dimmed page, and the keyboard
/// focus it had goes to the page, which stays where it was scrolled; Esc
/// leaves the panel beside the page alone. And only one thing is open over
/// the page at a time: the library opened over it (its icon on the rail,
/// or its button at 550 or less), the side panel, the queue sheet.
fn assert_drawer_behavior() {
    let mut checks: Vec<(&'static str, bool, String)> = Vec::new();

    // Gus's half screen: the library on its rail of covers
    let shell = render_shell(583, false);
    let b = &shell.builder;
    let utility: libadwaita::OverlaySplitView = b.object("utility_split").unwrap();
    let navigation: libadwaita::OverlaySplitView = b.object("navigation_split").unwrap();
    let panel: gtk::Widget = b.object("utility_panel").unwrap();
    // A row in the library, as there always is
    let rows: gtk::ListBox = b.object("home_listbox").unwrap();
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    rows.append(&row);
    // A page long enough to scroll
    let filler = gtk::Box::new(gtk::Orientation::Vertical, 0);
    filler.set_size_request(-1, 1600);
    shell.page.content().append(&filler);
    let scroller = page_scroller(&shell);
    let read = || read_overlays(&shell, &scroller);
    let library_icon = || {
        WidgetExt::activate_action(&shell.window, "win.sidebar-rail", None)
            .expect("the library's rail action");
    };

    let start = settled(&read);
    library_icon();
    let library_open = settled(&read);
    distinct_colors(&shell.frame, "layout-0583-library-over-page");
    checks.push((
        "583: the library icon on the rail opens the full library over the page",
        start.rail && library_open.library_over_page && !library_open.rail,
        format!("{:?} then {:?}", start, library_open),
    ));
    shell.panel.set_open(true);
    let panel_open = settled(&read);
    checks.push((
        "583: opening the side panel closes the library opened over the page",
        library_open.library_over_page
            && panel_open.panel
            && panel_open.floats
            && !panel_open.library_over_page
            && panel_open.rail,
        format!("{:?} then {:?}", library_open, panel_open),
    ));
    // A click on the library icon lands on the dimmed page; its action can
    // still come while the side panel floats
    library_icon();
    let library_over_panel = settled(&read);
    checks.push((
        "583: opening the library over the page closes the side panel",
        library_over_panel.library_over_page && !library_over_panel.panel,
        format!("{:?}", library_over_panel),
    ));
    navigation.set_show_sidebar(false);
    let library_closed = settled(&read);
    checks.push((
        "583: closing the library over the page brings its rail back",
        library_closed.rail && !library_closed.library_over_page && !library_closed.panel,
        format!("{:?}", library_closed),
    ));

    // The page scrolled down, and the side panel opened with the focus in
    // it (a tab clicked): from nothing focused before, or from the page's
    // first widget, now scrolled out of sight
    let tab = with_class(&panel, "utility-panel__tabs")
        .into_iter()
        .next()
        .and_then(|tabs| tabs.first_child())
        .expect("the side panel's first tab");
    let open_with_focus_inside = |page_focused_before: bool| {
        let adjustment = scroller.vadjustment();
        adjustment.set_value(0.0);
        pump(100);
        if page_focused_before {
            let _ = shell
                .page
                .widget()
                .child_focus(gtk::DirectionType::TabForward);
        } else {
            GtkWindowExt::set_focus(&shell.window, None::<&gtk::Widget>);
        }
        pump(100);
        adjustment.set_value(400.0);
        shell.panel.set_open(true);
        settled(&read);
        tab.grab_focus();
        settled(&read)
    };
    let closed_to_page = |opened: &Overlays, closed: &Overlays| {
        opened.panel
            && opened.focus == FocusIn::SidePanel
            && opened.page_scroll == 400
            && !closed.panel
            && closed.focus == FocusIn::Page
            && closed.page_scroll == opened.page_scroll
    };

    let opened = open_with_focus_inside(false);
    shell.panel.set_open(false);
    let by_button = settled(&read);
    let opened_after_page = open_with_focus_inside(true);
    shell.panel.set_open(false);
    let by_button_after_page = settled(&read);
    checks.push((
        "583: its button closes the side panel; the focus goes to the page, which stays where it was (with nothing focused before, and with the page's first widget)",
        closed_to_page(&opened, &by_button)
            && closed_to_page(&opened_after_page, &by_button_after_page),
        format!(
            "{:?} then {:?}; {:?} then {:?}",
            opened, by_button, opened_after_page, by_button_after_page
        ),
    ));

    let escape = canto_escape(&shell.window);
    let press_escape = || {
        escape
            .as_ref()
            .map(|action| action.activate(gtk::ShortcutActionFlags::EXCLUSIVE, &shell.window, None))
    };
    let opened = open_with_focus_inside(false);
    let handled = press_escape();
    let by_escape = settled(&read);
    let handled_when_closed = press_escape();
    let still_closed = settled(&read);
    checks.push((
        "583: Esc closes the side panel floating over the page; the focus goes to the page",
        handled == Some(true) && closed_to_page(&opened, &by_escape),
        format!("handled {:?}: {:?} then {:?}", handled, opened, by_escape),
    ));
    checks.push((
        "583: Esc with the side panel closed passes on",
        handled_when_closed == Some(false) && !still_closed.panel,
        format!("handled {:?}: {:?}", handled_when_closed, still_closed),
    ));

    let click = canto_click(&utility);
    let press_at = |x: f64, y: f64| {
        if let Some(click) = &click {
            click.emit_by_name::<()>("pressed", &[&1i32, &x, &y]);
        }
    };
    let opened = open_with_focus_inside(false);
    let card = panel
        .compute_bounds(&utility)
        .expect("the side panel's bounds");
    press_at(
        f64::from(card.x() + card.width() / 2.0),
        f64::from(card.y() + card.height() / 2.0),
    );
    let clicked_in_panel = settled(&read);
    let (x, y) = (20.0, f64::from(utility.height()) / 2.0);
    let dimmed = utility
        .pick(x, y, gtk::PickFlags::DEFAULT)
        .is_some_and(|w| w != panel && !w.is_ancestor(&panel));
    press_at(x, y);
    let clicked_on_page = settled(&read);
    checks.push((
        "583: a click in the side panel leaves it open",
        click.is_some() && clicked_in_panel.panel && clicked_in_panel.focus == FocusIn::SidePanel,
        format!("{:?}", clicked_in_panel),
    ));
    checks.push((
        "583: a click on the dimmed page closes the side panel; the focus goes to the page",
        click.is_some() && dimmed && closed_to_page(&opened, &clicked_on_page),
        format!(
            "dimmed page under ({}, {}): {}; {:?} then {:?}",
            x, y, dimmed, opened, clicked_on_page
        ),
    ));

    // The queue in its sheet (queue-side-panel off)
    let toggle_view = super::shell::panel_manager::toggle_view_for_tests(
        shell.panel.clone(),
        super::BottomSheet::new(b.object("bottom_sheet").unwrap()),
        b.object("app_header").unwrap(),
    );
    toggle_view("now");
    let now_open = settled(&read);
    toggle_view("queue");
    let queue_open = settled(&read);
    toggle_view("now");
    let now_again = settled(&read);
    checks.push((
        "583: the queue sheet closes the side panel floating over the page",
        now_open.panel && now_open.floats && queue_open.sheet && !queue_open.panel,
        format!("{:?} then {:?}", now_open, queue_open),
    ));
    checks.push((
        "583: the side panel opening over the page closes the queue sheet",
        now_again.panel && !now_again.sheet,
        format!("{:?}", now_again),
    ));
    shell.window.destroy();

    // At 550 or less the side panel fills the window, and the library's
    // button (or F9) opens the library over the page
    let shell = render_shell(550, true);
    let scroller = page_scroller(&shell);
    let read = || read_overlays(&shell, &scroller);
    let full = settled(&read);
    WidgetExt::activate_action(&shell.window, "win.show-navigation-panel", None)
        .expect("the library's action");
    let library = settled(&read);
    distinct_colors(&shell.frame, "layout-0550-library-over-page");
    shell.panel.set_open(true);
    let panel_again = settled(&read);
    checks.push((
        "550: opening the library over the page closes the side panel filling the window",
        full.panel && full.floats && library.library_over_page && !library.panel,
        format!("{:?} then {:?}", full, library),
    ));
    checks.push((
        "550: opening the side panel closes the library opened over the page",
        library.library_over_page && panel_again.panel && !panel_again.library_over_page,
        format!("{:?} then {:?}", library, panel_again),
    ));
    shell.window.destroy();

    // Beside the page, Esc is not the side panel's
    let shell = render_shell(1200, true);
    shoot_shell(&shell, 1200, true);
    let utility: libadwaita::OverlaySplitView = shell.builder.object("utility_split").unwrap();
    let side = || (utility.shows_sidebar(), utility.is_collapsed());
    let escape = canto_escape(&shell.window);
    let docked = side();
    let handled = escape
        .as_ref()
        .map(|action| action.activate(gtk::ShortcutActionFlags::EXCLUSIVE, &shell.window, None));
    let after = settled(side);
    checks.push((
        "1200: Esc leaves the side panel beside the page alone",
        docked == (true, false) && handled == Some(false) && after == (true, false),
        format!(
            "open and floating {:?}, Esc handled {:?}, then {:?}",
            docked, handled, after
        ),
    ));
    // Newer controllers run first: Canto's Esc comes before libadwaita's
    // own (the window's shortcut manager, which also closes a floating
    // split view)
    let controllers = shell.window.observe_controllers();
    let names: Vec<String> = (0..controllers.n_items())
        .filter_map(|i| controllers.item(i).and_downcast::<gtk::EventController>())
        .map(|c| c.name().map(|n| n.to_string()).unwrap_or_default())
        .collect();
    let ours = names
        .iter()
        .position(|n| n == "canto-floating-panel-escape");
    let libadwaitas = names
        .iter()
        .position(|n| n == "gtk-shortcut-manager-bubble");
    checks.push((
        "Canto's Esc runs before libadwaita's on the window",
        ours.is_some() && libadwaitas.is_some() && ours < libadwaitas,
        format!("{:?}", names),
    ));
    shell.window.destroy();

    for (what, ok, seen) in &checks {
        eprintln!(
            "drawer behavior, {}: {} ({})",
            what,
            if *ok { "ok" } else { "FAILED" },
            seen
        );
    }
    let failed: Vec<&str> = checks
        .iter()
        .filter(|(_, ok, _)| !ok)
        .map(|(what, _, _)| *what)
        .collect();
    assert!(
        failed.is_empty(),
        "drawer behavior: {} of {} checks failed: {:?}",
        failed.len(),
        checks.len(),
        failed
    );
    eprintln!("drawer behavior ok");
}
