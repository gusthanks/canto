//! The main window's layout: the side panel beside the page or floating
//! over it as a drawer, the library's rail, the page's bar and scrollbar,
//! and the Space key.

use super::*;

gtk_tests! {
    scrollbar_stays_put => assert_scrollbar_stays_put,
    space_plays_unless_typing => assert_space_plays_unless_typing,
    page_bar_covers_the_list => || {
        force_dark();
        assert_page_bar_covers_the_list();
    },
    drawer_layout => assert_drawer_layout,
    drawer_follows_resizes => || {
        force_dark();
        no_animations();
        assert_drawer_follows_resizes();
    },
    library_rails_beside_panel => || {
        force_dark();
        no_animations();
        assert_library_rails_beside_panel();
    },
    drawer_behavior => || {
        force_dark();
        no_animations();
        assert_drawer_behavior();
    },
}

/// Space plays or pauses unless it's typed into a text field.
fn assert_space_plays_unless_typing() {
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

/// Over a floating page (album, playlist, artist) the page's bar is
/// see-through while the cover shows. Once the title shows in it, the bar
/// takes the page card's background: the list scrolls up under the bar, and
/// its text crossed the title.
fn assert_page_bar_covers_the_list() {
    let gtk_settings = gtk::Settings::default().unwrap();
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(false);
    let display = gtk::gdk::Display::default().unwrap();
    let red = gtk::CssProvider::new();
    red.load_from_string(".page-bar-test-under { background-color: #ff0000; }");
    gtk::style_context_add_provider_for_display(
        &display,
        &red,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 2,
    );
    // The page under the bar: plain red, so any of it showing through reads
    let under = gtk::Box::new(gtk::Orientation::Vertical, 0);
    under.add_css_class("page-bar-test-under");
    under.set_hexpand(true);
    under.set_vexpand(true);
    let header = crate::app::components::AppHeaderBar::new();
    header.set_valign(gtk::Align::Start);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&under));
    overlay.add_overlay(&header);
    header.set_content(&under);
    let registrar =
        crate::app::components::HeaderRegistrar::new(header.clone(), Default::default());
    let page = registrar.page("album");
    page.set_title("Flowra FM Disco Voador");
    page.set_floating(true);
    page.set_title_revealed(false);
    header.set_active("album");
    let window = gtk::Window::new();
    window.set_default_size(600, 300);
    window.set_child(Some(&overlay));
    window.present();
    wait_frames(&window, 3);
    pump(300);
    // Above the title and away from the buttons: only the bar's background
    let bar = || {
        let x = header.width() * 3 / 4;
        pixel_at(overlay.upcast_ref(), x, 4)
    };
    let clear = bar();
    page.set_title_revealed(true);
    wait_frames(&window, 3);
    pump(300);
    let solid = bar();
    let is_red = |px: Option<[u8; 4]>| px.is_some_and(|p| p[0] > 200 && p[1] < 80 && p[2] < 80);
    check(
        "the page's bar is see-through over a floating page while its title is hidden",
        is_red(clear),
        format!("{:?}", clear),
    );
    check(
        "with the title shown, the page's bar covers the page scrolling under it",
        solid.is_some() && !is_red(solid),
        format!("{:?}", solid),
    );
    window.destroy();
    gtk::style_context_remove_provider_for_display(&display, &red);
    gtk_settings.set_gtk_enable_animations(animations);
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
pub(super) struct Shell {
    pub(super) builder: gtk::Builder,
    pub(super) window: libadwaita::ApplicationWindow,
    /// What is rendered: the window, or the box standing in for a window
    /// wider than the headless screen
    pub(super) frame: gtk::Widget,
    /// What sizes the window's content to a width under the headless
    /// screen's, when the shell has to be resized there (`resize_clamped`)
    pub(super) clamp: Option<libadwaita::Clamp>,
    pub(super) _layout: std::rc::Rc<crate::app::components::WindowLayout>,
    pub(super) panel: crate::app::components::UtilityPanel,
    pub(super) page: crate::app::components::DetailsPage,
}

impl Shell {
    /// Lays the window's content out at `width` (from `render_shell_clamped`).
    fn resize_clamped(&self, width: i32) {
        let clamp = self
            .clamp
            .as_ref()
            .expect("a shell from render_shell_clamped");
        clamp.set_maximum_size(width);
        clamp.set_tightening_threshold(width);
    }
}

/// Where the shell's parts ended up (px).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ShellShot {
    pub(super) width: i32,
    pub(super) open: bool,
    pub(super) collapsed: bool,
    pub(super) panel_shown: bool,
    pub(super) max_panel: f64,
    /// The utility split: the whole window's width
    pub(super) window_split: i32,
    /// The navigation split: what the side panel leaves to library and page
    pub(super) page_split: i32,
    pub(super) library: i32,
    pub(super) rail: bool,
    pub(super) page_card: i32,
    pub(super) panel_card: i32,
    /// Where the side panel's card ends, in the utility split
    pub(super) panel_end: f32,
}

impl ShellShot {
    fn tag(&self) -> String {
        format!("{}{}", self.width, if self.open { "-open" } else { "" })
    }

    pub(super) fn mode(&self) -> &'static str {
        if !self.collapsed {
            "beside"
        } else if self.max_panel >= f64::from(i32::MAX) {
            "fills the window"
        } else {
            "drawer"
        }
    }
}

/// The open side panel's card is drawn whole where its sizes say: 348 px
/// beside the page (360 less its gaps), its width less its 8 px frame as a
/// drawer, ending inside the window.
fn panel_card_drawn(s: &ShellShot) -> bool {
    let card = if s.collapsed {
        s.max_panel as i32 - 16
    } else {
        348
    };
    s.panel_shown && s.panel_card == card && s.panel_end <= s.window_split as f32 + 0.5
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
pub(super) fn render_shell(width: i32, open: bool) -> Shell {
    render_shell_in(width, open, false)
}

/// As `render_shell`, in a headless window as wide as its screen whose
/// content is laid out `width` wide, so `Shell::resize_clamped` can widen or
/// narrow it under 1024 px as a window would (the window's own breakpoint
/// stays at the screen's width).
fn render_shell_clamped(width: i32, open: bool) -> Shell {
    render_shell_in(width, open, true)
}

fn render_shell_in(width: i32, open: bool, clamped: bool) -> Shell {
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
    let layout = crate::app::components::WindowLayout::new(&builder);
    let panel = crate::app::components::UtilityPanel::new(
        builder.object("utility_panel").unwrap(),
        builder.object("utility_split").unwrap(),
        std::rc::Rc::clone(&layout),
    );
    let cover = sample_cover();
    let tab = |name: &str| Some(gtk::Label::new(Some(name)).upcast::<gtk::Widget>());
    panel.set_pages(&crate::app::components::PanelPages {
        now: Some(crate::app::components::shell::now_panel::sample_for_tests(
            cover.upcast_ref(),
        )),
        queue: tab("Queue"),
        lyrics: tab("Lyrics"),
        about: tab("About"),
        stats: tab("Stats"),
    });
    let page = crate::app::components::DetailsPage::sample_for_tests(cover.upcast_ref());
    let stack: gtk::Stack = builder.object("content_stack").unwrap();
    stack.add_named(page.widget(), Some("album"));
    stack.set_visible_child_name("album");
    let bar: crate::app::components::shell::playback::PlaybackWidget =
        builder.object("playback").unwrap();
    bar.set_track(
        &crate::app::components::now_panel::title_markup("MARIA BONITA SEM LAMPIÃO", "1", true),
        "<a href=\"https://open.spotify.com/artist/1\">Niink</a>, <a href=\"https://open.spotify.com/artist/2\">Supernova Ent</a>",
    );
    bar.set_liked(Some(true));
    // Opened before it shows: a slide-in started on screen may never finish
    // on the headless display
    panel.set_open(open);
    let mut clamp = None;
    let frame = if clamped {
        // As wide as the headless screen, its content laid out `width` wide
        let content = window.content().expect("the window's content");
        window.set_content(None::<&gtk::Widget>);
        window.set_default_size(1024, 621);
        let sizer = libadwaita::Clamp::new();
        sizer.set_maximum_size(width);
        sizer.set_tightening_threshold(width);
        sizer.set_child(Some(&content));
        window.set_content(Some(&sizer));
        clamp = Some(sizer);
        content
    } else if width > 1024 {
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
        clamp,
        _layout: layout,
        panel,
        page,
    }
}

/// Reads where the shell's parts are, once they stay put (up to 5 s).
pub(super) fn shoot_shell(shell: &Shell, width: i32, open: bool) -> ShellShot {
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
    // At 550 or less the side panel fills the window flush, as before the
    // drawer, and so does the page under it: the drawer's card and its
    // shaded frame are for wider windows
    for s in shots.iter().filter(|s| s.width <= 550 && s.open) {
        check(
            &format!(
                "layout {}: the side panel and the page fill the window, flush",
                s.tag()
            ),
            s.panel_card == s.window_split && s.page_split == s.window_split,
            format!(
                "side panel {}, page split {}, of {}",
                s.panel_card, s.page_split, s.window_split
            ),
        );
    }
    // Drawn whole where its sizes say: its card 348 beside the page (inside
    // its gaps), its width less its 8 px frame as a drawer
    let misdrawn: Vec<String> = shots
        .iter()
        .filter(|s| s.open && s.width > 550)
        .filter(|s| !panel_card_drawn(s))
        .map(|s| {
            format!(
                "{}: card {} ending at {}",
                s.tag(),
                s.panel_card,
                s.panel_end
            )
        })
        .collect();
    check(
        "layout: the side panel's card is drawn whole, 348 px beside the page, its width less 16 as a drawer",
        misdrawn.is_empty(),
        format!("{:?}", misdrawn),
    );
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
    let header: crate::app::components::AppHeaderBar = shell.builder.object("app_header").unwrap();
    let registrar =
        crate::app::components::HeaderRegistrar::new(header.clone(), Default::default());
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
    // Opened again after the window showed, it is drawn beside the page
    let misdrawn: Vec<String> = [
        ("docked", &docked),
        ("widened", &widened),
        ("wide header, docked", &hidden),
    ]
    .iter()
    .filter(|(_, s)| !panel_card_drawn(s))
    .map(|(step, s)| format!("{}: card {} ending at {}", step, s.panel_card, s.panel_end))
    .collect();
    check(
        "layout resize: the side panel's card is drawn whole beside the page (348 px) whenever it is open there",
        misdrawn.is_empty(),
        format!("{:?}", misdrawn),
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
    let full = |s: &ShellShot| !s.rail && s.library > 200;
    let rail = |s: &ShellShot| s.rail && s.library == 60;
    let docked = |s: &ShellShot| !s.collapsed && s.panel_shown;

    // Closing the side panel beside the page, as on screen (animated): the
    // panel slides out over a page that only grows, the library keeping its
    // rail until the panel has left (a full library at once would leave the
    // page 546 px under the panel still there, and drop the album column)
    let docked_before_close = panel_open(true);
    let gtk_settings = gtk::Settings::default().unwrap();
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(true);
    let library_widget: gtk::Widget = shell.builder.object("navigation_panel").unwrap();
    let panel_widget: gtk::Widget = shell.builder.object("utility_panel").unwrap();
    let page_widget = shell
        .builder
        .object::<libadwaita::OverlaySplitView>("navigation_split")
        .unwrap()
        .content()
        .expect("the page");
    // The page's width and the library's rail at each frame
    let frames = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let tick = page_widget.add_tick_callback(clone!(
        #[strong]
        frames,
        #[weak]
        library_widget,
        #[upgrade_or]
        glib::ControlFlow::Break,
        move |page, _| {
            frames.borrow_mut().push((
                page.width(),
                library_widget.has_css_class("navigation-panel--rail"),
            ));
            glib::ControlFlow::Continue
        }
    ));
    shell.panel.set_open(false);
    let sliding = panel_widget
        .parent()
        .is_some_and(|pane| pane.is_child_visible());
    let rail_while_sliding = library_widget.has_css_class("navigation-panel--rail");
    let after_close = shoot_shell(&shell, 1200, false);
    tick.remove();
    gtk_settings.set_gtk_enable_animations(animations);
    let frames = frames.borrow().clone();
    check(
        "layout library, 1200: closing the side panel beside the page, the library keeps its rail while the panel slides out, and is full once it has",
        docked(&docked_before_close)
            && rail(&docked_before_close)
            && sliding
            && rail_while_sliding
            && !after_close.panel_shown
            && full(&after_close),
        format!(
            "before {:?}; just after closing: panel sliding {}, rail {}; frames (page, rail) {:?}; after {:?}",
            docked_before_close, sliding, rail_while_sliding, frames, after_close
        ),
    );
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
    // Opened after the window showed (reopened, widened), it is drawn beside
    // the page, not just said to be there
    let misdrawn: Vec<String> = steps
        .iter()
        .filter(|(_, s)| s.panel_shown && !panel_card_drawn(s))
        .map(|(step, s)| format!("{}: card {} ending at {}", step, s.panel_card, s.panel_end))
        .collect();
    check(
        "layout library: the side panel's card is drawn whole beside the page (348 px) in every step it is open",
        misdrawn.is_empty(),
        format!("{:?}", misdrawn),
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

    // Half of a 1200 sp screen: the library on its rail of covers
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

    // A keyboard user's place on the page: a widget further down, focused
    // and then scrolled out of sight. Closing the side panel gives it the
    // focus back, and the page stays where it was scrolled (libadwaita,
    // left alone, scrolls the page back to that widget)
    let further = gtk::Button::with_label("Further down");
    shell.page.content().append(&further);
    let adjustment = scroller.vadjustment();
    adjustment.set_value(adjustment.upper());
    pump(300);
    further.grab_focus();
    pump(100);
    adjustment.set_value(400.0);
    let before = settled(&read);
    shell.panel.set_open(true);
    settled(&read);
    tab.grab_focus();
    let opened = settled(&read);
    shell.panel.set_open(false);
    let closed = settled(&read);
    let focus = GtkWindowExt::focus(&shell.window);
    checks.push((
        "583: closing the side panel gives the focus back to the page's widget that had it, out of sight, and the page stays where it was",
        before.focus == FocusIn::Page
            && opened.panel
            && opened.focus == FocusIn::SidePanel
            && !closed.panel
            && focus.as_ref() == Some(further.upcast_ref())
            && closed.page_scroll == before.page_scroll,
        format!(
            "{:?} then {:?} then {:?}, focus on {:?}",
            before,
            opened,
            closed,
            focus.map(|f| f.type_().name())
        ),
    ));

    // Another page (Ctrl+F, Alt+2..5, a link in the side panel) with the
    // side panel floating over the page: the panel closes, so the new page
    // can take the focus
    let stack: gtk::Stack = b.object("content_stack").unwrap();
    let other_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let entry = gtk::SearchEntry::new();
    other_page.append(&entry);
    stack.add_named(&other_page, Some("other-page"));
    let opened = open_with_focus_inside(false);
    stack.set_visible_child_name("other-page");
    // As `Navigation::go` does once the page shows
    glib::idle_add_local_once(clone!(
        #[weak]
        entry,
        move || {
            entry.grab_focus();
        }
    ));
    let navigated = settled(&read);
    let focus = GtkWindowExt::focus(&shell.window);
    let on_entry = focus
        .as_ref()
        .is_some_and(|f| f == entry.upcast_ref::<gtk::Widget>() || f.is_ancestor(&entry));
    checks.push((
        "583: going to another page closes the side panel floating over the page, and the new page takes the focus",
        opened.panel && opened.floats && !navigated.panel && on_entry,
        format!(
            "{:?} then {:?}, focus on {:?}",
            opened,
            navigated,
            focus.map(|f| f.type_().name())
        ),
    ));
    stack.set_visible_child_name("album");
    stack.remove(&other_page);
    shell.panel.set_open(false);
    settled(&read);

    // The queue in its sheet (queue-side-panel off)
    let toggle_view = crate::app::components::shell::panel_manager::toggle_view_for_tests(
        shell.panel.clone(),
        crate::app::components::BottomSheet::new(b.object("bottom_sheet").unwrap()),
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

    // The full library opened over the page has its own collapse button:
    // there it closes the library (back to its rail), and saves no rail
    // the user never chose (navigation-rail)
    let settings = gio::Settings::new(canto_config::settings::SETTINGS);
    let saved_before = settings.boolean(canto_config::settings::NAVIGATION_RAIL);
    shell.panel.set_open(false);
    settled(&read);
    library_icon();
    let over_page = settled(&read);
    let collapse: gtk::Button = b.object("rail_collapse").unwrap();
    let collapse_shown = collapse.is_mapped();
    collapse.emit_clicked();
    let collapsed = settled(&read);
    let saved_after = settings.boolean(canto_config::settings::NAVIGATION_RAIL);
    checks.push((
        "583: the collapse button of the library opened over the page closes it, and saves no rail",
        over_page.library_over_page
            && collapse_shown
            && !collapsed.library_over_page
            && collapsed.rail
            && saved_after == saved_before,
        format!(
            "{:?} (its collapse button shown {}) then {:?}; saved rail {} then {}",
            over_page, collapse_shown, collapsed, saved_before, saved_after
        ),
    ));
    // As it was, for the checks after
    let _ = settings.set_boolean(canto_config::settings::NAVIGATION_RAIL, saved_before);
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

    // Widened from the half screen with the library opened over the page:
    // once the full library fits beside the page, it is back there
    let shell = render_shell_clamped(583, false);
    let scroller = page_scroller(&shell);
    let read = || read_overlays(&shell, &scroller);
    settled(&read);
    WidgetExt::activate_action(&shell.window, "win.sidebar-rail", None)
        .expect("the library's rail action");
    let over_page = settled(&read);
    shell.resize_clamped(1000);
    let widened = shoot_shell(&shell, 1000, false);
    let widened_overlays = settled(&read);
    checks.push((
        "widened from 583 to 1000, the library opened over the page goes back beside it, full",
        over_page.library_over_page
            && !widened_overlays.library_over_page
            && !widened.rail
            && widened.library > 200
            && widened.window_split == 1000,
        format!("{:?} then {:?}; {:?}", over_page, widened_overlays, widened),
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
        check(&format!("drawer behavior, {}", what), *ok, seen);
    }
    eprintln!("drawer behavior checked");
}
