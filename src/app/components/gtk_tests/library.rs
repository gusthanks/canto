//! The library and its pages: cards, card grids, the release filters, the
//! album page and the sidebar's grid of covers.

use super::shell_layout::{render_shell, shoot_shell, ShellShot};
use super::*;

gtk_tests! {
    card_playing_badge_shows_bars => assert_card_playing_badge_shows_bars,
    library_keeps_rows_on_play => || {
        force_dark();
        assert_library_keeps_rows_on_play();
    },
    filtered_card_grid_keeps_its_columns => || {
        force_dark();
        assert_filtered_card_grid_keeps_its_columns();
    },
    release_filters_are_buttons => || {
        force_dark();
        assert_release_filters_are_buttons();
    },
    card_artist_links => || {
        force_dark();
        assert_card_artist_links();
    },
    no_lines_between_songs => assert_no_lines_between_songs,
    album_page_fits => assert_album_page_fits,
    sidebar_grid => assert_sidebar_grid,
    track_list_filter_and_sort => || {
        force_dark();
        assert_track_list_filter_and_sort();
    },
}

/// The sidebar's grid of covers: section titles stay out of it, and in the
/// usual sidebar width three covers sit side by side, the second row whole.
/// Renders sidebar-grid-sample.
fn assert_sidebar_grid() {
    let _look = SampleLook::new();
    let cover = sample_cover();
    // Section titles stay out of the grid (a FlowBox with a model ignores
    // filter functions: the model itself must be filtered)
    let titles = crate::app::components::shell::navigation_panel::grid_titles_for_tests();
    assert_eq!(titles.len(), 2, "only covers in the grid, got {:?}", titles);
    assert!(
        titles.iter().any(|t| t == "Matuê"),
        "the artist is in the grid: {:?}",
        titles
    );

    // "View As" > Grid: three covers side by side in the usual width
    let grid =
        crate::app::components::shell::navigation_panel::grid_sample_for_tests(cover.upcast_ref());
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

/// The round "playing" badge on album cards must show the bars on top of
/// its background, not just a plain dark disc.
fn assert_card_playing_badge_shows_bars() {
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

/// A song from another album plays: Recents moves that album up and every
/// other row keeps its widget, and so its cover (20/11: every cover of the
/// library blinked on each play, as all rows were made again).
fn assert_library_keeps_rows_on_play() {
    use crate::app::components::shell::navigation_panel::{
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

/// An artist's releases with only "Albums" shown: the cards the filter hides
/// don't take a column. 20/11: the shown cards moved a column right, the
/// first column stayed empty and the last ones piled up on each other.
fn assert_filtered_card_grid_keeps_its_columns() {
    let (grid, flowbox) = crate::app::components::widgets::card_list::aligned_grid_for_tests();
    flowbox.set_min_children_per_line(1);
    flowbox.set_max_children_per_line(24);
    flowbox.set_selection_mode(gtk::SelectionMode::None);
    // Albums and singles mixed, as on an artist page
    // A single first: where it was, it ties with the first album, which
    // then took the second column (an artist page often starts with singles)
    let kinds = [
        "single", "album", "single", "album", "album", "single", "album", "album", "album",
    ];
    for (i, kind) in kinds.iter().enumerate() {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.set_size_request(100, 120);
        card.set_widget_name(&format!("{kind}-{i}"));
        flowbox.append(&card);
    }
    let window = gtk::Window::new();
    // Three 100 px cards fit, four don't
    window.set_default_size(340, 600);
    let scrolled = gtk::ScrolledWindow::new();
    scrolled.set_hscrollbar_policy(gtk::PolicyType::Never);
    scrolled.set_child(Some(&grid));
    window.set_child(Some(&scrolled));
    window.present();
    let cells = || -> Vec<(String, f32, f32)> {
        let mut out = Vec::new();
        let mut child = flowbox.first_child();
        while let Some(c) = child {
            child = c.next_sibling();
            if !c.is_child_visible() || !c.is_visible() {
                continue;
            }
            let name = c
                .first_child()
                .map(|w| w.widget_name().to_string())
                .unwrap_or_default();
            if let Some(b) = c.compute_bounds(&grid) {
                out.push((name, b.x().round(), b.y().round()));
            }
        }
        out
    };
    // All laid out once (the hidden ones keep this place)
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while cells().len() < kinds.len() && std::time::Instant::now() < deadline {
        pump(50);
    }
    pump(300);
    // Only albums, as the view filter does. The grid is laid out again on
    // the next frames, slow on the headless display: a tick callback keeps
    // frames coming, and three pass before the places are read. Waiting
    // only for the places to hold still passed at once, on the places from
    // before the filter
    let ticks = std::rc::Rc::new(std::cell::Cell::new(0));
    let ticker = {
        let ticks = ticks.clone();
        window.add_tick_callback(move |_, _| {
            ticks.set(ticks.get() + 1);
            glib::ControlFlow::Continue
        })
    };
    flowbox.set_filter_func(|child| {
        child
            .first_child()
            .is_some_and(|w| w.widget_name().starts_with("album"))
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while ticks.get() < 3 && std::time::Instant::now() < deadline {
        pump(50);
    }
    ticker.remove();
    check(
        "frames pass after filtering the card grid",
        ticks.get() >= 3,
        format!("{} frames", ticks.get()),
    );
    let settled = |cells: &[(String, f32, f32)]| cells.len() == 6;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    let mut last = cells();
    let mut same = 0;
    while (same < 3 || !settled(&last)) && std::time::Instant::now() < deadline {
        pump(100);
        let now = cells();
        same = if now == last { same + 1 } else { 0 };
        last = now;
    }
    eprintln!("filtered card grid: {:?}", last);
    check(
        "the filtered card grid shows the six albums",
        last.len() == 6,
        last.len(),
    );
    // No two cards in the same place
    let mut piled = Vec::new();
    for (i, a) in last.iter().enumerate() {
        for b in &last[i + 1..] {
            if (a.1, a.2) == (b.1, b.2) {
                piled.push(format!("{} and {} at x={} y={}", a.0, b.0, a.1, a.2));
            }
        }
    }
    check(
        "no two filtered cards are drawn on top of each other",
        piled.is_empty(),
        format!("{:?}", piled),
    );
    // Every row starts at the left edge, and the rows are full: a hidden
    // card left a hole where it used to be (the six took three rows)
    let mut rows: Vec<f32> = last.iter().map(|c| c.2).collect();
    rows.sort_by(|a, b| a.total_cmp(b));
    rows.dedup();
    check(
        "the six filtered albums fill two rows of three",
        rows.len() == 2,
        format!("rows at y={:?}", rows),
    );
    let mut off_edge = Vec::new();
    for y in rows {
        let left = last
            .iter()
            .filter(|c| c.2 == y)
            .map(|c| c.1)
            .fold(f32::MAX, f32::min);
        if left > 1.0 {
            off_edge.push(format!("the row at y={} starts at x={}", y, left));
        }
    }
    check(
        "every row of the filtered card grid starts at the left edge",
        off_edge.is_empty(),
        format!("{:?}", off_edge),
    );
    window.destroy();
}

/// An artist's release filters are buttons under the section's title, as
/// the Library's are: each shows only while its kind is in the list (no
/// Compilations on most artists), the row only with two kinds or more.
/// Clicking one shows only its kind; clicking it again, everything. On a
/// narrow page the row wraps (Q15, 20/11).
fn assert_release_filters_are_buttons() {
    let picked = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let chips = {
        let picked = picked.clone();
        crate::app::components::FilterChips::new(
            &crate::app::models::FilterOption::release_types(),
            move |category| picked.borrow_mut().push(category.to_string()),
        )
    };
    let window = gtk::Window::new();
    window.set_default_size(160, 200);
    window.set_child(Some(chips.widget()));
    window.present();
    wait_frames(&window, 3);
    let kinds = |list: &[&str]| {
        list.iter()
            .map(|kind| kind.to_string())
            .collect::<std::collections::HashSet<_>>()
    };
    chips.show_categories(&kinds(&["album", "single"]));
    let two = chips.shown_for_tests();
    chips.click_for_tests("Singles");
    let singles = chips.shown_for_tests();
    chips.click_for_tests("Albums");
    let albums = chips.shown_for_tests();
    chips.click_for_tests("Albums");
    let all = chips.shown_for_tests();
    chips.show_categories(&kinds(&["album"]));
    let one = chips.shown_for_tests();
    chips.show_categories(&kinds(&["album", "single", "ep", "compilation"]));
    let four = chips.shown_for_tests();
    check(
        "an artist's release filters are buttons, each shown while its kind is there",
        two == (
            true,
            vec!["Albums".to_string(), "Singles".to_string()],
            None,
        ) && singles.2.as_deref() == Some("Singles")
            && albums.2.as_deref() == Some("Albums")
            && all.2.is_none()
            && !one.0
            && four.0
            && four.1.len() == 4,
        format!(
            "{:?} {:?} {:?} {:?} {:?} {:?}",
            two, singles, albums, all, one, four
        ),
    );
    check(
        "clicking a release filter shows its kind, clicking it again shows everything",
        *picked.borrow() == ["single", "album", ""],
        format!("{:?}", picked.borrow()),
    );
    wait_frames(&window, 3);
    let (min, _, _, _) = chips.widget().measure(gtk::Orientation::Horizontal, -1);
    check(
        "the four release filters wrap on a narrow page",
        min > 0 && min <= 160,
        format!("narrowest {} px", min),
    );
    window.destroy();
}

/// The artists under an album's cover are links: each opens its artist's
/// page in the app (the app.view_artist action), not the browser; any
/// other link is left to the label (Q14, 20/11).
fn assert_card_artist_links() {
    use canto_api::models::{ArtistRef, Provider, ResourceId};
    let card = crate::app::components::card::CardWidget::new(
        crate::app::components::card::ImageShape::Square,
        crate::app::models::CardLayout::Vertical,
    );
    let label = card.subtitle_label_for_tests();
    let artists = [
        ArtistRef {
            rri: ResourceId::new(Provider::Spotify, "a1"),
            name: "Niink".to_string(),
        },
        ArtistRef {
            rri: ResourceId::new(Provider::Spotify, "a2"),
            name: "Supernova Ent".to_string(),
        },
    ];
    label.set_markup(&crate::app::components::now_panel::artist_links_markup(
        &artists,
    ));
    let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    let group = gio::SimpleActionGroup::new();
    let view_artist = gio::SimpleAction::new("view_artist", Some(glib::VariantTy::STRING));
    {
        let opened = opened.clone();
        view_artist.connect_activate(move |_, parameter| {
            if let Some(id) = parameter.and_then(|p| p.get::<String>()) {
                opened.borrow_mut().push(id);
            }
        });
    }
    group.add_action(&view_artist);
    let window = gtk::Window::new();
    window.insert_action_group("app", Some(&group));
    window.set_child(Some(&card));
    window.present();
    wait_frames(&window, 2);
    distinct_colors(window.upcast_ref(), "card-artist-links");
    check(
        "the artists under a cover are dim, their links read as the text around them",
        label.has_css_class("dim-label") && label.has_css_class("track-links"),
        format!("{:?}", label.css_classes()),
    );
    // Stands in for the label's own handling, which would open the browser:
    // a link the card leaves alone ends here, and nothing is opened
    let left_to_label = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
    {
        let left_to_label = left_to_label.clone();
        label.connect_activate_link(move |_, uri| {
            left_to_label.borrow_mut().push(uri.to_string());
            glib::Propagation::Stop
        });
    }
    let _: bool = label.emit_by_name("activate-link", &[&"https://open.spotify.com/artist/a2"]);
    let _: bool = label.emit_by_name("activate-link", &[&"https://example.org/"]);
    check(
        "the artists under an album's cover link to their pages in the app",
        label.text().as_str() == "Niink, Supernova Ent"
            && label.uses_markup()
            && *opened.borrow() == ["a2"]
            && *left_to_label.borrow() == ["https://example.org/"],
        format!(
            "text {:?}, opened in the app {:?}, left to the label {:?}",
            label.text(),
            opened.borrow(),
            left_to_label.borrow()
        ),
    );
    window.destroy();
}

/// No line between the songs of a track list (as Spotify): at a fractional
/// display scale the 1px line moved between pixels while scrolling and
/// flickered (Q6). The color right at the edge between two songs is the
/// color just above it.
fn assert_no_lines_between_songs() {
    let style = libadwaita::StyleManager::default();
    let scheme = style.color_scheme();
    style.set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let cover = sample_cover();
    let page = crate::app::components::DetailsPage::sample_for_tests(cover.upcast_ref());
    page.add_sample_tracks_for_tests(4);
    let window = gtk::Window::new();
    window.set_decorated(false);
    window.set_default_size(900, 1100);
    window.set_child(Some(page.widget()));
    window.present();
    wait_mapped(page.widget(), 300);
    wait_frames(&window, 3);
    pump(500);
    let cards = with_class(page.widget().upcast_ref(), "song__card");
    let mut bounds: Vec<_> = cards
        .iter()
        .filter_map(|card| card.compute_bounds(&window))
        .collect();
    bounds.sort_by(|a, b| a.y().total_cmp(&b.y()));
    // A song in the middle: the first and the last are the card's ends
    let seen = bounds.get(1).map(|middle| {
        let x = (middle.x() + middle.width() * 0.55) as i32;
        let edge = (middle.y() + middle.height()) as i32 - 1;
        (
            pixel_at(window.upcast_ref(), x, edge),
            pixel_at(window.upcast_ref(), x, edge - 5),
        )
    });
    let same = |a: [u8; 4], b: [u8; 4]| a.iter().zip(b.iter()).all(|(p, q)| p.abs_diff(*q) <= 2);
    check(
        "no line between the songs of a track list",
        bounds.len() >= 3 && matches!(seen, Some((Some(edge), Some(above))) if same(edge, above)),
        format!("{} rows, edge and above: {:?}", bounds.len(), seen),
    );
    window.destroy();
    style.set_color_scheme(scheme);
}

/// A title that takes two lines beside the cover at the widths below.
const TWO_LINE_TITLE: &str = "O Veneno do Escorpião V.2";

/// Over this width (sp) the album header puts its cover beside the text
/// (header.blp).
const HEADER_BESIDE_OVER_SP: f64 = 510.0;

/// The least width (px) of the album header with the cover beside the text,
/// whatever the text scale: the cover 200, its gap 12 and the row of
/// buttons 266. The header stacks under it.
const HEADER_BESIDE_LEAST_PX: f64 = 478.0;

/// Album pages at text 0.91 around where the header turns beside its cover
/// (inside their 24 px paddings: headers of 464 to 492).
const ALBUM_PAGE_WIDTHS_SMALL_TEXT: [i32; 5] = [512, 518, 525, 526, 540];

/// The widths an album page gets, with the title it shows (`None`: the
/// sample's): its least (the header's 280 inside its 24 px paddings), the
/// window's least, 440, 499 (the page in half of a 1920 px screen at scale
/// 1.6), and 607 and 900 with the cover beside the text; and with a title on
/// two lines, both sides of where the header turns beside its cover (over
/// 510 inside the paddings: a page of 558).
const ALBUM_PAGE_SHOTS: &[(i32, Option<&str>)] = &[
    (328, None),
    (360, None),
    (440, None),
    (499, None),
    (607, None),
    (900, None),
    (480, Some(TWO_LINE_TITLE)),
    (500, Some(TWO_LINE_TITLE)),
    (525, Some(TWO_LINE_TITLE)),
    (560, Some(TWO_LINE_TITLE)),
    (600, Some(TWO_LINE_TITLE)),
];

/// Where an album page's header and first track ended up (px).
#[derive(Clone, Copy, Debug, PartialEq)]
struct AlbumPageShot {
    /// The page's own width
    page: i32,
    /// The header's breakpoint bin, and the least its layout needs
    bin: i32,
    layout_min: i32,
    /// The cover beside the text (not above it), and the bin's width over
    /// which it belongs there: 510 sp at the current text scale, and never
    /// under the 478 px it takes there
    beside: bool,
    beside_over: f64,
    /// The header's height, and how far down it draws: the end of its
    /// buttons' row (with its margin) or of the cover
    header: i32,
    drawn: f32,
    title_lines: i32,
    /// In the page's scrolled content: where the buttons end, and where
    /// the first track row starts
    buttons_end: f32,
    first_row: f32,
    /// The rightmost edge of the cover, the title and the menu button, in
    /// a viewport this wide
    right: f32,
    viewport: i32,
}

fn shoot_album_page(page: &crate::app::components::DetailsPage) -> AlbumPageShot {
    let parts = page.parts_for_tests();
    let viewport = parts.scrolled_window.child().expect("the page's viewport");
    let bottom = |b: Option<gtk::graphene::Rect>| b.map_or(0.0, |b| b.y() + b.height());
    let right = |w: &gtk::Widget| {
        w.compute_bounds(&viewport)
            .map_or(0.0, |b| b.x() + b.width())
    };
    let buttons_bottom = bottom(parts.buttons.compute_bounds(&parts.header));
    let first_row = with_class(page.content().upcast_ref(), "song__card")
        .iter()
        .filter_map(|card| card.compute_bounds(&parts.scroll_child))
        .map(|b| b.y())
        .fold(f32::MAX, f32::min);
    AlbumPageShot {
        page: page.widget().width(),
        bin: parts.breakpoint_bin.width(),
        layout_min: parts.header_box.measure(gtk::Orientation::Horizontal, -1).0,
        beside: parts.header_box.orientation() == gtk::Orientation::Horizontal,
        beside_over: libadwaita::LengthUnit::Sp
            .to_px(HEADER_BESIDE_OVER_SP, gtk::Settings::default().as_ref())
            .max(HEADER_BESIDE_LEAST_PX - 1.0),
        header: parts.header.height(),
        drawn: (buttons_bottom + parts.buttons.margin_bottom() as f32)
            .max(bottom(parts.image_box.compute_bounds(&parts.header))),
        title_lines: parts.title.layout().line_count(),
        buttons_end: bottom(parts.buttons.compute_bounds(&parts.scroll_child)),
        first_row,
        right: [
            parts.image_box.upcast_ref::<gtk::Widget>(),
            parts.title.upcast_ref(),
            parts.menu.upcast_ref(),
        ]
        .iter()
        .map(|w| right(*w))
        .fold(0.0, f32::max),
        viewport: viewport.width(),
    }
}

/// Reads the album page once it stays put: the same for 1.2 s and three
/// frames (up to 10 s). A frame of the headless display can take a second,
/// and a header that turns beside its cover (or under it) is measured again
/// on the next frame.
fn settle_album_page(
    window: &gtk::Window,
    page: &crate::app::components::DetailsPage,
) -> AlbumPageShot {
    let clock = window.frame_clock().expect("the window's frame clock");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    let mut last = shoot_album_page(page);
    let (mut same, mut since) = (0, clock.frame_counter());
    while (same < 8 || clock.frame_counter() < since + 3) && std::time::Instant::now() < deadline {
        window.queue_draw();
        pump(150);
        let now = shoot_album_page(page);
        if now == last {
            same += 1;
        } else {
            same = 0;
            since = clock.frame_counter();
        }
        last = now;
    }
    last
}

/// The album page in the main window at 1200 px (all of a 1920 px screen at
/// scale 1.6) with the side panel beside it and the full library (opened
/// from its rail's icon), text at 0.91 and a title on two lines: the cover
/// beside a title that wraps, where the first track covered the play
/// button. Returns the layout and the page.
fn shoot_album_page_beside_panels() -> (ShellShot, AlbumPageShot) {
    let gtk_settings = gtk::Settings::default().unwrap();
    let dpi = gtk_settings.gtk_xft_dpi();
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_xft_dpi(SMALL_TEXT_XFT_DPI);
    gtk_settings.set_gtk_enable_animations(false);
    let shell = render_shell(1200, true);
    WidgetExt::activate_action(&shell.window, "win.sidebar-rail", None)
        .expect("the library's rail action");
    let layout = shoot_shell(&shell, 1200, true);
    shell.page.header().set_title(TWO_LINE_TITLE);
    // One track, whole in view. The headless display's renderer assumes
    // each render node sits in one place, but GTK shares one node between
    // the same icons (the menu's dots of the header and of every row):
    // with a copy cut off by the page's edge it can lose track of a node
    // and abort the display server (broadway-server.c, node != NULL). The
    // windows of `assert_album_page_fits` show their four tracks whole.
    shell.page.add_sample_tracks_for_tests(1);
    let page = settle_album_page(shell.window.upcast_ref(), &shell.page);
    distinct_colors(&shell.frame, "album-page-1200-both-panels");
    shell.window.destroy();
    gtk_settings.set_gtk_xft_dpi(dpi);
    gtk_settings.set_gtk_enable_animations(animations);
    (layout, page)
}

/// An album page in its own parents (page > scrolled window > header and
/// track list) fits every width it gets: its header's layout fits the
/// header (nothing past the page's edge), the title keeps to two lines, the
/// header is as tall as what it draws, and the first track starts under
/// the header's buttons. Alone in a window at each width, with four tracks,
/// and in the main window beside both panels with a two-line title and one
/// track.
fn assert_album_page_fits() {
    let style = libadwaita::StyleManager::default();
    let scheme = style.color_scheme();
    style.set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let cover = sample_cover();
    // Alone in a window `width` px wide, with four tracks
    let shoot_alone = |width: i32, title: Option<&str>, tag: &str| {
        let page = crate::app::components::DetailsPage::sample_for_tests(cover.upcast_ref());
        page.add_sample_tracks_for_tests(4);
        if let Some(title) = title {
            page.header().set_title(title);
        }
        let window = gtk::Window::new();
        window.set_decorated(false);
        window.set_default_size(width, 760);
        window.set_child(Some(page.widget()));
        window.present();
        wait_mapped(page.widget(), 300);
        let shot = settle_album_page(&window, &page);
        distinct_colors(window.upcast_ref(), tag);
        window.destroy();
        shot
    };
    let mut shots = Vec::new();
    for (width, title) in ALBUM_PAGE_SHOTS.iter().copied() {
        let suffix = if title.is_some() { "-two-lines" } else { "" };
        let shot = shoot_alone(width, title, &format!("album-page-{:04}{}", width, suffix));
        let label = format!(
            "{}{}",
            width,
            if title.is_some() {
                " (two-line title)"
            } else {
                ""
            }
        );
        shots.push((label, shot));
    }
    // With text at 0.91, where the header turns beside its cover
    let gtk_settings = gtk::Settings::default().unwrap();
    let dpi = gtk_settings.gtk_xft_dpi();
    gtk_settings.set_gtk_xft_dpi(SMALL_TEXT_XFT_DPI);
    let small_text: Vec<AlbumPageShot> = ALBUM_PAGE_WIDTHS_SMALL_TEXT
        .iter()
        .map(|width| {
            shoot_alone(
                *width,
                Some(TWO_LINE_TITLE),
                &format!("album-page-{:04}-small-text", width),
            )
        })
        .collect();
    gtk_settings.set_gtk_xft_dpi(dpi);
    let (layout, beside_panels) = shoot_album_page_beside_panels();
    let in_shell = "1200 beside the side panel and the full library, text at 0.91 (two-line title)";
    shots.push((in_shell.to_string(), beside_panels));
    style.set_color_scheme(scheme);
    for s in &small_text {
        eprintln!(
            "album page {} at text 0.91 (two-line title): header {} in {} (its layout needs {}), {} tall, draws {}; buttons end at {}, first track at {}; right edge {} of {}",
            s.page,
            if s.beside { "beside the cover" } else { "under the cover" },
            s.bin,
            s.layout_min,
            s.header,
            s.drawn,
            s.buttons_end,
            s.first_row,
            s.right,
            s.viewport
        );
    }
    let misfits: Vec<String> = small_text
        .iter()
        .filter(|s| {
            s.layout_min > s.bin
                || s.right > s.viewport as f32 + 0.5
                || (s.drawn - s.header as f32).abs() > 1.0
                || s.first_row < s.buttons_end + 17.0
                || s.beside != (f64::from(s.bin) >= HEADER_BESIDE_LEAST_PX)
        })
        .map(|s| {
            format!(
                "page {}: header {} in {} (needs {}), right edge {} of {}",
                s.page,
                if s.beside { "beside" } else { "under" },
                s.bin,
                s.layout_min,
                s.right,
                s.viewport
            )
        })
        .collect();
    check(
        "album pages of 512 to 540 px with text at 0.91: the header fits, and puts its cover beside the text once it fits there (478 px)",
        small_text.len() == ALBUM_PAGE_WIDTHS_SMALL_TEXT.len() && misfits.is_empty(),
        format!("{:?}", misfits),
    );
    for (label, s) in &shots {
        eprintln!(
            "album page {}: page {}, header {} in {} (its layout needs {}), {} tall, draws {}, title {} lines; buttons end at {}, first track at {}; right edge {} of {}",
            label,
            s.page,
            if s.beside { "beside the cover" } else { "under the cover" },
            s.bin,
            s.layout_min,
            s.header,
            s.drawn,
            s.title_lines,
            s.buttons_end,
            s.first_row,
            s.right,
            s.viewport
        );
    }
    eprintln!(
        "album page {}: side panel {} {}, library {}{}, page card {}",
        in_shell,
        layout.mode(),
        layout.panel_card,
        layout.library,
        if layout.rail { " (rail)" } else { "" },
        layout.page_card
    );
    assert!(
        !layout.collapsed && layout.panel_shown && !layout.rail && layout.library > 200,
        "album page {}: the side panel beside the page and the full library: {:?}",
        in_shell,
        layout
    );
    assert!(
        beside_panels.beside && beside_panels.title_lines == 2,
        "album page {}: the cover beside a title on two lines: {:?}",
        in_shell,
        beside_panels
    );
    for (label, s) in &shots {
        assert!(
            s.layout_min <= s.bin,
            "album page {}: the header's layout needs {} px but gets {}: it spills past the page's edge",
            label,
            s.layout_min,
            s.bin
        );
        assert!(
            s.right <= s.viewport as f32 + 0.5,
            "album page {}: the header reaches {} in a page {} wide: cut at its edge",
            label,
            s.right,
            s.viewport
        );
        assert!(
            s.title_lines <= 2,
            "album page {}: the title takes {} lines",
            label,
            s.title_lines
        );
        assert!(
            (s.drawn - s.header as f32).abs() <= 1.0,
            "album page {}: the header is {} px tall but draws {}",
            label,
            s.header,
            s.drawn
        );
        assert!(
            s.first_row < f32::MAX && s.first_row >= s.buttons_end + 17.0,
            "album page {}: the first track starts at {}, over the header's buttons (they end at {})",
            label,
            s.first_row,
            s.buttons_end
        );
        assert_eq!(
            s.beside,
            f64::from(s.bin) > s.beside_over,
            "album page {}: the header puts its cover beside the text only over {} px (510 sp, at least 477)",
            label,
            s.beside_over
        );
    }
    eprintln!("album page ok");
}

/// Filtering and sorting a TrackList maps activated visible rows back to
/// their true underlying track indices and recovers cleanly after clearing.
fn assert_track_list_filter_and_sort() {
    use crate::app::components::{DetailsSectionWidget, TrackList, TrackListModel, TrackSortOrder};
    use crate::app::models::{make_track, SongListModel};
    use crate::app::ProvidesApi;
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::sync::Arc;

    struct DemoModel {
        songs: SongListModel,
        played: RefCell<Vec<(usize, String)>>,
        api: Arc<canto_api::ApiService>,
    }

    impl ProvidesApi for DemoModel {
        fn api_service(&self) -> Arc<canto_api::ApiService> {
            Arc::clone(&self.api)
        }
    }

    impl TrackListModel for DemoModel {
        fn is_paused(&self) -> bool {
            false
        }
        fn song_list_model(&self) -> SongListModel {
            self.songs.clone()
        }
        fn current_song_id(&self) -> Option<String> {
            None
        }
        fn play_song_at(&self, pos: usize, id: &str) {
            self.played.borrow_mut().push((pos, id.to_string()));
        }
    }

    let token_provider: Arc<dyn canto_api::TokenProvider> = Arc::new(|| None::<String>);
    let api = Arc::new(canto_api::spotify_service(
        Arc::clone(&token_provider),
        token_provider,
        1024 * 1024,
        1024 * 1024,
    ));

    let make = |id: &str, title: &str, artist: &str, duration_ms: u32| {
        use canto_api::models::{ArtistRef, Provider, ResourceId};
        let mut t = make_track(id);
        t.title = title.to_string();
        t.artists = vec![ArtistRef {
            rri: ResourceId::new(Provider::Spotify, format!("art-{id}")),
            name: artist.to_string(),
        }];
        t.duration_ms = duration_ms;
        t
    };

    let mut songs = SongListModel::new(50);
    songs
        .append(vec![
            make("id-0", "Zulu Echo", "Beta", 220_000),
            make("id-1", "Sunset Drive", "Zeta", 190_000),
            make("id-2", "Midnight Sun", "Alpha", 150_000),
        ])
        .commit();

    let model = Rc::new(DemoModel {
        songs,
        played: RefCell::new(Vec::new()),
        api,
    });

    let listview = gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>);
    let section = DetailsSectionWidget::new(Some("Tracks"), &listview);
    let track_list = TrackList::new(listview.clone(), Rc::clone(&model));
    let controller = track_list.controller();
    controller.connect_filter_state_changed(clone!(
        #[weak]
        section,
        #[weak]
        listview,
        move |active, count| {
            let empty = active && count == 0;
            section.set_empty(empty);
            listview.set_visible(!empty);
        }
    ));

    let window = gtk::Window::new();
    window.set_default_size(640, 480);
    window.set_child(Some(&section));
    window.present();
    wait_frames(&window, 2);

    assert_eq!(listview.model().unwrap().n_items(), 3);

    // Filter to songs matching "sun" ("Sunset Drive" at index 1, "Midnight Sun" at index 2)
    // and sort by Title -> "Midnight Sun" (index 2) becomes visible row 0.
    controller.set_filter_query("sun");
    controller.set_sort_order(TrackSortOrder::Title);
    pump(100);
    assert_eq!(listview.model().unwrap().n_items(), 2);

    listview.emit_by_name::<()>("activate", &[&0u32]);
    pump(50);
    assert_eq!(
        model.played.borrow().as_slice(),
        &[(2, "id-2".to_string())],
        "activating filtered+sorted row 0 must play underlying track index 2"
    );

    // Empty filter result hides list and shows empty label cleanly
    controller.set_filter_query("nonexistent");
    pump(50);
    assert_eq!(listview.model().unwrap().n_items(), 0);
    assert!(!listview.is_visible());

    // Clearing filter + sorting back to Original restores all 3 rows
    controller.set_filter_query("");
    controller.set_sort_order(TrackSortOrder::Original);
    pump(50);
    assert!(listview.is_visible());
    assert_eq!(listview.model().unwrap().n_items(), 3);

    drop(track_list);
    window.destroy();
    eprintln!("track list filter and sort ok");
}
