//! The lyrics page: its options, the line naming the source, and the
//! window to add lyrics by hand.

use super::*;

gtk_tests! {
    lyrics_options => assert_lyrics_options,
    lyrics_source_line => assert_lyrics_source_line,
    add_lyrics_window => || {
        force_dark();
        assert_add_lyrics_window();
    },
}

/// Word by word or whole line, and the lyrics coming back after a scroll
/// by hand (or staying put).
fn assert_lyrics_options() {
    use crate::app::components::pages::lyrics::LyricsPageWidget;
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
    use crate::app::components::{PanelPages, UtilityPanelWidget};
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
    // No source had the lyrics: the way out is a web search, not Genius
    // (Q13a, 20/11)
    pump(300);
    let mut labels = Vec::new();
    let mut stack = vec![lyrics.clone().upcast::<gtk::Widget>()];
    while let Some(widget) = stack.pop() {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            if button.is_mapped() {
                if let Some(label) = button.label() {
                    labels.push(label.to_string());
                }
            }
        }
        let mut child = widget.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            stack.push(c);
        }
    }
    check(
        "with no lyrics found, the buttons search the web and add lyrics, not Genius",
        labels.iter().any(|label| label == "Search the Web")
            && labels.iter().any(|label| label == "Add Lyrics")
            && !labels.iter().any(|label| label.contains("Genius")),
        format!("{:?}", labels),
    );
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
    let (window, lyrics) =
        crate::app::components::shell::fullscreen_lyrics::sample_with_lyrics_for_tests(
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

/// "Add Lyrics", shown for approval before sending is built: it opens
/// empty, its button waits for some text, and the confirmation says what
/// would go out. Renders add-lyrics-*.png (Q13b, sample lines only).
fn assert_add_lyrics_window() {
    use crate::app::components::pages::lyrics::add_lyrics;
    use libadwaita::prelude::{AdwDialogExt, AdwWindowExt};
    let song = add_lyrics::SongForLyrics {
        title: "Disco Voador".to_string(),
        artist: "Niink".to_string(),
        album: "Flowra FM".to_string(),
        duration_ms: 225_000,
    };
    // An Adw window hosts the dialog inside it, as the main window does
    let window = libadwaita::Window::new();
    window.set_default_size(560, 720);
    window.set_content(Some(&gtk::Box::new(gtk::Orientation::Vertical, 0)));
    window.present();
    wait_frames(&window, 2);
    let sent = std::rc::Rc::new(std::cell::Cell::new(0));
    let dialog = {
        let sent = sent.clone();
        add_lyrics::open(&window, &song, move |_, _| sent.set(sent.get() + 1))
    };
    wait_frames(&window, 3);
    pump(300);
    let view = add_lyrics::text_view_for_tests(&dialog).expect("the lyrics box");
    let buffer = view.buffer();
    let (start, end) = buffer.bounds();
    let empty = buffer.text(&start, &end, false).is_empty();
    distinct_colors(window.upcast_ref(), "add-lyrics-empty");
    buffer.set_text(
        "[00:12.40] First sample line\n[00:15.10] Second sample line\n[00:18.90] Third sample line",
    );
    wait_frames(&window, 2);
    pump(300);
    distinct_colors(window.upcast_ref(), "add-lyrics-filled");
    let draft = add_lyrics::parse_draft(
        "[00:12.40] First sample line\n[00:15.10] Second sample line\n[00:18.90] Third sample line",
    );
    let confirm = add_lyrics::confirmation(&song, &draft);
    confirm.present(Some(&dialog));
    wait_frames(&window, 3);
    pump(300);
    distinct_colors(window.upcast_ref(), "add-lyrics-confirm");
    check(
        "Add Lyrics opens empty, waits for text, and sends nothing on its own",
        empty && draft.synced.is_some() && draft.lines == 3 && sent.get() == 0,
        format!("empty {}, draft {:?}, sent {}", empty, draft, sent.get()),
    );
    confirm.force_close();
    dialog.force_close();
    window.destroy();
}
