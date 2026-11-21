//! The queue page: Recently Played above the queue.

use super::*;

gtk_tests! {
    recently_played => assert_recently_played,
    recently_played_opens_at_newest => assert_recently_played_opens_at_newest,
}

/// Recently Played sits folded above the queue; unfolded, it lists songs
/// with their covers.
fn assert_recently_played() {
    use crate::app::components::{QueuePageWidget, RecentRow};
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
    // The sample render (a PNG only with CANTO_TEST_DUMP_DIR)
    {
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
fn assert_recent_hearts(
    page: &crate::app::components::QueuePageWidget,
    rows: &[crate::app::components::RecentRow],
) {
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
    use crate::app::components::{QueuePageWidget, RecentRow};
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
        // Three songs at once, so Now Playing and the next one stay in view
        // below it (20/11): whole rows, the fourth hidden
        let rows_in_view = list_rows
            .iter()
            .filter_map(|row| row.compute_bounds(&scroller))
            .filter(|b| b.y() >= -1.0 && b.y() + b.height() <= height + 1.0)
            .count();
        check(
            "Recently Played shows three songs at once",
            rows_in_view == crate::app::components::RECENT_VISIBLE_ROWS as usize,
            format!(
                "{} rows in a {} px list (rows first: {})",
                rows_in_view, height, rows_first
            ),
        );
        check(
            "Recently Played leaves no room for a fourth song",
            height <= last.height() * 3.0 + 12.0,
            format!("the list is {} px for {} px rows", height, last.height()),
        );
        window.destroy();
    }
    gtk_settings.set_gtk_enable_animations(animations);
}
