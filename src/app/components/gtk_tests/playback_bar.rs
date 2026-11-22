//! The player bar: its tiers as the window narrows, the More Options menu
//! and the volume under the wheel.

use super::*;

gtk_tests! {
    volume_scrolls_in_steps => || {
        force_dark();
        assert_volume_scrolls_in_steps();
    },
    bar_tiers => assert_bar_tiers,
    more_options_rows => assert_more_options_rows,
    more_options_equalizer_visibility => assert_more_options_equalizer_visibility,
}

/// The wheel or the touchpad over the volume changes it 5% a step, as
/// Ctrl+Up and Ctrl+Down do, over the speaker and the slider and over the
/// narrow bar's volume button; the slider doesn't scroll again on its own.
/// The tooltip says the level and the keys (Q12, 20/11).
fn assert_volume_scrolls_in_steps() {
    let bar: crate::app::components::shell::playback::PlaybackWidget = glib::Object::new();
    let (slider, volume_box, volume_button) = bar.volume_parts_for_tests();
    let volume_popover: gtk::Widget = volume_button
        .downcast_ref::<gtk::MenuButton>()
        .expect("volume menu button")
        .popover()
        .expect("volume popover")
        .upcast();
    for (name, target) in [
        ("speaker and slider", &volume_box),
        ("narrow bar's volume button", &volume_button),
        ("narrow bar's volume popover", &volume_popover),
    ] {
        let controllers = target.observe_controllers();
        let scroll = (0..controllers.n_items())
            .filter_map(|i| {
                controllers
                    .item(i)
                    .and_downcast::<gtk::EventControllerScroll>()
            })
            .next();
        let Some(scroll) = scroll else {
            check(
                &format!("the wheel over the {name} changes the volume"),
                false,
                "no scroll controller",
            );
            continue;
        };
        let mut seen = Vec::new();
        for (from, steps) in [(0.40, -1.0), (0.40, 2.0), (0.98, -1.0), (0.02, 1.0)] {
            slider.set_value(from);
            let handled: bool = scroll.emit_by_name("scroll", &[&0.0f64, &steps]);
            seen.push((
                from,
                steps,
                (slider.value() * 100.0).round() as i32,
                handled,
            ));
        }
        let levels: Vec<i32> = seen.iter().map(|s| s.2).collect();
        check(
            &format!(
                "the wheel over the {name} changes the volume 5% a step, before the slider scrolls"
            ),
            levels == [45, 30, 100, 0]
                && seen.iter().all(|s| s.3)
                && scroll.propagation_phase() == gtk::PropagationPhase::Capture,
            format!("{:?}, {:?}", seen, scroll.propagation_phase()),
        );
    }
    slider.set_value(0.45);
    let tips = (slider.tooltip_text(), volume_button.tooltip_text());
    check(
        "the volume's tooltip says the level and the keys",
        tips.0.as_deref() == Some("Volume 45% · Ctrl+↑ / Ctrl+↓") && tips.0 == tips.1,
        format!("{:?}", tips),
    );
}

/// Player bar widths (px): both sides of every tier's edge, a window on half
/// of a 1200 sp screen (572 and 583) and the widest the headless screen
/// shows.
const BAR_WIDTHS: [i32; 15] = [
    360, 400, 401, 486, 487, 560, 561, 572, 583, 660, 661, 760, 860, 861, 1024,
];

/// With text at 0.91 the icons keep their size while the text shrinks: just
/// past each tier's edge, and the window on half of the screen.
const BAR_WIDTHS_SMALL_TEXT: [i32; 6] = [401, 487, 561, 572, 661, 861];

/// A track over an hour ("1∶02∶03"), whose times beside the seek bar are
/// wider than the controls over them, and one under an hour with the
/// widest times ("12∶34" of "59∶59"): at the narrow edges of the steps that
/// keep the times, the window on half of the screen, and both sides of 660,
/// under which the times get shorter.
const BAR_WIDTHS_LONG_TRACK: [i32; 5] = [401, 487, 572, 660, 661];

/// How long the player bar's track is, for the times beside its seek bar.
#[derive(Clone, Copy, PartialEq)]
enum BarTrack {
    /// 3∶35, at its start
    Short,
    /// 59∶59, at 12∶34
    Minutes,
    /// 1∶02∶03, at 1∶00∶00
    Hour,
}

/// A sleep timer's countdown in the bar: the window on half of a 1200 sp
/// screen (572 and 583), and just past the edges where the slider and
/// shuffle leave.
const BAR_WIDTHS_TIMER: [i32; 4] = [572, 583, 661, 861];

/// The edges of the bar's tiers (px): a bar at an edge or under it drops
/// that tier's parts (`bar_part_shows`).
const BAR_TIER_EDGES: [i32; 5] = [860, 660, 560, 486, 400];

/// Bars around what the narrowest tier and a sleep timer's countdown take
/// together (400 px and the countdown's 75 with its spacing): the window's
/// least, the narrowest tier's edge, and both sides of 475, where the
/// countdown comes back.
const BAR_WIDTHS_NARROW_TIMER: [i32; 5] = [360, 400, 450, 475, 476];

/// The bar's parts that its tiers hide or show, by their ids in the
/// templates.
const BAR_PARTS: [&str; 16] = [
    "shuffle",
    "prev",
    "repeat",
    "track_position",
    "track_duration",
    "like_slot",
    "lyrics_button",
    "queue_button",
    "device_selector_box",
    "volume_box",
    "volume_button",
    "mini_player_button",
    "eq_button",
    "side_panel_button",
    "more_shuffle",
    "more_repeat",
];

/// Whether `part` of the player bar shows in a bar `width` px wide, with no
/// sleep timer running.
fn bar_part_shows(part: &str, width: i32) -> bool {
    match part {
        // 860 and under: the volume slider turns into a button, and the
        // device and mini player buttons leave (both stay in Ctrl+K)
        "volume_box" | "device_selector_box" | "mini_player_button" => width > 860,
        "volume_button" => width > 486 && width <= 860,
        // 660 and under (a window on half of a 1200 sp screen): shuffle and
        // repeat move into the gear's More Options menu
        "shuffle" | "repeat" => width > 660,
        "more_shuffle" | "more_repeat" => width <= 660,
        // 560 and under: lyrics and queue, which the side panel has as tabs
        "lyrics_button" | "queue_button" => width > 560,
        // 486 and under: the like button and the volume button
        "like_slot" => width > 486,
        // 400 and under: previous, and the times beside the seek bar
        "prev" | "track_position" | "track_duration" => width > 400,
        // The gear (the way to everything else) and the side panel button
        // never leave
        _ => true,
    }
}

/// Where the player bar's parts ended up (px).
#[derive(Clone, Debug, PartialEq)]
struct BarShot {
    tag: String,
    width: i32,
    /// The bar's breakpoint bin, and the least width its row needs: past
    /// the bin's width libadwaita warns "exceeds" and cuts the row's end
    bar: i32,
    needs: i32,
    /// The least width of the row's three columns: the track (cover, title
    /// and like), the controls over the seek bar, and the buttons at the end
    columns: (i32, i32, i32),
    /// The title and artist column, and the least of it in px (100 sp)
    title: i32,
    title_least: f64,
    /// Where the side panel button ends, in the bin
    end: f32,
    /// The gaps along the buttons at the bar's end: before the first,
    /// between each two, after the last
    end_gaps: Vec<i32>,
    /// The parts in `BAR_PARTS` that show, and those the templates lack
    shown: Vec<&'static str>,
    missing: Vec<&'static str>,
    /// The sleep timer's countdown button, while it shows (else 0)
    countdown: i32,
    /// The times beside the seek bar, and whether either one that shows is
    /// cut short ("1∶0…")
    times: (String, String),
    times_cut: bool,
}

fn shoot_bar(
    bar: &crate::app::components::shell::playback::PlaybackWidget,
    tag: &str,
    width: i32,
) -> BarShot {
    let root = bar.upcast_ref::<gtk::Widget>();
    let bin = bar.first_child().expect("the bar's breakpoint bin");
    let row = bin.first_child().expect("the bar's row");
    let least =
        |w: Option<gtk::Widget>| w.map_or(0, |w| w.measure(gtk::Orientation::Horizontal, -1).0);
    let info = by_id(root, "song_info_box").expect("the title column");
    let side = by_id(root, "side_panel_button").expect("the side panel button");
    let bar_end = by_id(root, "bar_end").expect("the buttons at the bar's end");
    let mut spans = Vec::new();
    let mut child = bar_end.first_child();
    while let Some(c) = child {
        child = c.next_sibling();
        if c.is_visible() && c.width() > 0 {
            if let Some(b) = c.compute_bounds(&bar_end) {
                spans.push((b.x().round() as i32, (b.x() + b.width()).round() as i32));
            }
        }
    }
    let time = |i: usize| {
        by_id(root, ["track_position", "track_duration"][i])
            .and_then(|w| w.downcast::<gtk::Label>().ok())
            .expect("the times beside the seek bar")
    };
    let mut end_gaps = Vec::new();
    let mut at = 0;
    for (start, end) in &spans {
        end_gaps.push(start - at);
        at = *end;
    }
    end_gaps.push(bar_end.width() - at);
    let mut shown = Vec::new();
    let mut missing = Vec::new();
    for id in BAR_PARTS.iter().copied() {
        match by_id(root, id) {
            // In the bar: on screen (its slot too). In the gear's menu,
            // closed here: its own state
            Some(widget)
                if if id.starts_with("more_") {
                    widget.get_visible()
                } else {
                    widget.is_visible()
                } =>
            {
                shown.push(id)
            }
            Some(_) => {}
            None => missing.push(id),
        }
    }
    BarShot {
        tag: tag.to_string(),
        width,
        bar: bin.width(),
        needs: row.measure(gtk::Orientation::Horizontal, -1).0,
        columns: (
            least(row.first_child()),
            least(row.first_child().and_then(|c| c.next_sibling())),
            least(row.last_child()),
        ),
        title: if info.is_mapped() { info.width() } else { 0 },
        title_least: libadwaita::LengthUnit::Sp.to_px(100.0, gtk::Settings::default().as_ref()),
        end: side
            .compute_bounds(&bin)
            .map_or(f32::MAX, |b| b.x() + b.width()),
        end_gaps,
        shown,
        missing,
        countdown: by_id(root, "sleep_timer_button")
            .filter(|button| button.is_mapped())
            .map_or(0, |button| button.width()),
        times: (time(0).text().to_string(), time(1).text().to_string()),
        times_cut: (0..2).any(|i| time(i).is_mapped() && time(i).layout().is_ellipsized()),
    }
}

/// The player bar alone in a window `width` px wide, playing a track with
/// a long title and three artists (`track` long), once it stays put (up to
/// 8 s).
fn render_bar(
    width: i32,
    tag: &str,
    timer: bool,
    track: BarTrack,
) -> (
    gtk::Window,
    crate::app::components::shell::playback::PlaybackWidget,
    BarShot,
) {
    let bar: crate::app::components::shell::playback::PlaybackWidget = glib::Object::new();
    bar.set_track(
        &crate::app::components::now_panel::title_markup("MARIA BONITA SEM LAMPIÃO", "1", true),
        "<a href=\"https://open.spotify.com/artist/1\">Niink</a>, <a href=\"https://open.spotify.com/artist/2\">Jotapê</a>, <a href=\"https://open.spotify.com/artist/3\">Supernova Ent</a>",
    );
    bar.set_liked(Some(true));
    match track {
        BarTrack::Short => bar.set_song_duration(Some(215_000.0)),
        BarTrack::Minutes => {
            bar.set_song_duration(Some(3_599_000.0));
            bar.set_seek_position(754_000.0);
        }
        BarTrack::Hour => {
            bar.set_song_duration(Some(3_723_000.0));
            bar.set_seek_position(3_600_000.0);
        }
    }
    if timer {
        bar.set_sleep_timer_snapshot(&crate::app::components::SleepTimerSnapshot {
            mode: crate::app::components::SleepTimerMode::Minutes(45),
            remaining_secs: Some(45 * 60 - 1),
        });
    }
    let window = gtk::Window::new();
    window.set_decorated(false);
    window.set_default_size(width, 96);
    window.set_child(Some(&bar));
    window.present();
    wait_mapped(&bar, 300);
    // Laid out at that width, then unchanged for 1.2 s: a frame of the
    // headless display can take a second, and a new tier takes one more
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(8);
    let mut last = shoot_bar(&bar, tag, width);
    let mut same = 0;
    while same < 8 && std::time::Instant::now() < deadline {
        pump(150);
        let now = shoot_bar(&bar, tag, width);
        same = if now == last && now.bar == width {
            same + 1
        } else {
            0
        };
        last = now;
    }
    (window, bar, last)
}

/// The player bar stays on one line at every width a window can have: its
/// row fits the bar (no "exceeds", nothing cut at its end), the title and
/// artist keep at least 100 sp, and the bar drops its buttons tier by tier
/// as the window narrows; at 660 and under, shuffle and repeat move into the
/// gear's More Options menu. The gear and the side panel button never
/// leave. Also with text at 0.91, with a sleep timer's countdown in the
/// bar, and with tracks of 10 minutes or more, whose times are never cut
/// (over an hour, a narrow bar shows "1h02"). Renders player-bar-<width>.
fn assert_bar_tiers() {
    let style = libadwaita::StyleManager::default();
    let scheme = style.color_scheme();
    style.set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let gtk_settings = gtk::Settings::default().unwrap();
    let dpi = gtk_settings.gtk_xft_dpi();
    let runs = BAR_WIDTHS
        .iter()
        .map(|w| (*w, false, false, BarTrack::Short))
        .chain(
            BAR_WIDTHS_SMALL_TEXT
                .iter()
                .map(|w| (*w, true, false, BarTrack::Short)),
        )
        .chain(
            BAR_WIDTHS_TIMER
                .iter()
                .map(|w| (*w, false, true, BarTrack::Short)),
        )
        .chain(BAR_WIDTHS_LONG_TRACK.iter().flat_map(|w| {
            [
                (*w, false, false, BarTrack::Hour),
                (*w, false, false, BarTrack::Minutes),
            ]
        }));
    let mut shots = Vec::new();
    for (width, small_text, timer, track) in runs {
        if small_text {
            gtk_settings.set_gtk_xft_dpi(SMALL_TEXT_XFT_DPI);
        }
        let tag = format!(
            "player-bar-{:04}{}{}{}",
            width,
            if small_text { "-small-text" } else { "" },
            if timer { "-sleep-timer" } else { "" },
            match track {
                BarTrack::Short => "",
                BarTrack::Minutes => "-minutes-track",
                BarTrack::Hour => "-long-track",
            }
        );
        let (window, _bar, shot) = render_bar(width, &tag, timer, track);
        if track != BarTrack::Short && width > 400 {
            // Never "1∶0…": a narrow bar (660 and under) shows a track over
            // an hour in hours and minutes, and the minutes of any track
            // whole
            check(
                &format!("{}: the times beside the seek bar are whole", tag),
                !shot.times_cut,
                format!("{} and {}", shot.times.0, shot.times.1),
            );
            if track == BarTrack::Hour {
                let expected = if width <= 660 {
                    ("1h00", "1h02")
                } else {
                    ("1∶00∶00", "1∶02∶03")
                };
                check(
                    &format!("{}: the times read {} and {}", tag, expected.0, expected.1),
                    shot.times.0 == expected.0 && shot.times.1 == expected.1,
                    format!("{} and {}", shot.times.0, shot.times.1),
                );
            }
        }
        distinct_colors(window.upcast_ref(), &tag);
        window.destroy();
        gtk_settings.set_gtk_xft_dpi(dpi);
        shots.push((small_text, timer, shot));
    }
    // A sleep timer's countdown in the narrowest bars, where no tier is left
    // to make room for it: it leaves the bar (the timer stays in More
    // Options) rather than push the row past the bar's end
    for width in BAR_WIDTHS_NARROW_TIMER.iter().copied() {
        let tag = format!("player-bar-{:04}-sleep-timer", width);
        let (window, _bar, s) = render_bar(width, &tag, true, BarTrack::Short);
        distinct_colors(window.upcast_ref(), &tag);
        window.destroy();
        check(
            &format!(
                "{}: the row fits the bar with a sleep timer's countdown",
                tag
            ),
            s.needs <= s.bar && s.end <= s.bar as f32 + 0.5,
            format!(
                "bar {}, its row needs {}, side panel button ends at {}; shows {}",
                s.bar,
                s.needs,
                s.end,
                s.shown.join(" ")
            ),
        );
    }
    style.set_color_scheme(scheme);
    for (_, _, s) in &shots {
        eprintln!(
            "bar tier {}: bar {} (its row needs {}: track {}, controls {}, buttons {}), title {} (least {:.0}), side panel button ends at {}, gaps at the end {:?}; shows {}{}",
            s.tag,
            s.bar,
            s.needs,
            s.columns.0,
            s.columns.1,
            s.columns.2,
            s.title,
            s.title_least,
            s.end,
            s.end_gaps,
            s.shown.join(" "),
            if s.missing.is_empty() {
                String::new()
            } else {
                format!("; missing {}", s.missing.join(" "))
            }
        );
    }
    for (_, _, s) in &shots {
        assert!(
            s.needs <= s.bar,
            "{}: the bar's row needs {} px but gets {}: libadwaita warns \"exceeds\" and cuts its end",
            s.tag,
            s.needs,
            s.bar
        );
        assert!(
            s.end <= s.bar as f32 + 0.5,
            "{}: the side panel button ends at {}, past the bar's {}",
            s.tag,
            s.end,
            s.bar
        );
        assert!(
            f64::from(s.title) >= s.title_least - 0.5,
            "{}: the title and artist get {} px, under 100 sp ({:.0} px)",
            s.tag,
            s.title,
            s.title_least
        );
        // Evenly spaced (the bar's end has a spacing of 2): a hidden
        // button's empty slot would add its spacing to a gap
        let last = s.end_gaps.len() - 1;
        assert!(
            s.end_gaps
                .iter()
                .enumerate()
                .all(|(i, gap)| *gap == if i == 0 || i == last { 0 } else { 2 }),
            "{}: the buttons at the bar's end are not evenly spaced: gaps {:?}",
            s.tag,
            s.end_gaps
        );
    }
    for (_, timer, s) in &shots {
        assert!(
            s.missing.is_empty(),
            "{}: the bar has no {}",
            s.tag,
            s.missing.join(", ")
        );
        if *timer {
            // The countdown is an icon and its time, with no pill padding,
            // and takes room: the next tier down makes it, and no more (a
            // window on half of the screen loses lyrics and queue, never
            // the like and volume buttons)
            check(
                &format!(
                    "{}: the sleep timer's countdown is compact (80 px at most)",
                    s.tag
                ),
                s.countdown > 0 && s.countdown <= 80,
                format!("{} px", s.countdown),
            );
            let down = BAR_TIER_EDGES
                .iter()
                .copied()
                .find(|edge| *edge < s.width)
                .unwrap_or(0);
            let expected: Vec<&str> = BAR_PARTS
                .iter()
                .copied()
                .filter(|part| bar_part_shows(part, down))
                .collect();
            check(
                &format!(
                    "{}: with a sleep timer's countdown the bar drops one tier, to the one at {} px, and no more",
                    s.tag, down
                ),
                s.shown == expected,
                format!(
                    "shows {}; expected {}",
                    s.shown.join(" "),
                    expected.join(" ")
                ),
            );
            continue;
        }
        for part in BAR_PARTS.iter().copied() {
            assert_eq!(
                s.shown.contains(&part),
                bar_part_shows(part, s.width),
                "{}: {} should {}show in a bar {} px wide",
                s.tag,
                part,
                if bar_part_shows(part, s.width) {
                    ""
                } else {
                    "not "
                },
                s.width
            );
        }
    }
    eprintln!("bar tiers ok");
}

/// In a bar too narrow for them, shuffle and repeat are rows of the gear's
/// More Options menu: they run the actions of Ctrl+S and Ctrl+R, keep the
/// menu open, and their marks (and repeat's icon) follow the state.
/// Renders player-bar-0572-more-options with shuffle on.
fn assert_more_options_rows() {
    use crate::app::models::RepeatMode;
    use gio::prelude::ActionMapExt;
    let style = libadwaita::StyleManager::default();
    let scheme = style.color_scheme();
    style.set_color_scheme(libadwaita::ColorScheme::ForceDark);
    let (window, bar, _) = render_bar(572, "player-bar-0572-more-options", false, BarTrack::Short);
    let root = bar.upcast_ref::<gtk::Widget>();
    let part = |id: &str| by_id(root, id).unwrap_or_else(|| panic!("the bar has no {}", id));
    // Stand-ins for the app's actions, which turn the state and tell the
    // bar, as the app does
    let shuffled = std::rc::Rc::new(std::cell::Cell::new(false));
    let toggle_shuffle = gio::SimpleAction::new("toggle_shuffle", None);
    toggle_shuffle.connect_activate(clone!(
        #[weak]
        bar,
        #[strong]
        shuffled,
        move |_, _| {
            shuffled.set(!shuffled.get());
            bar.set_shuffled(shuffled.get());
        }
    ));
    let repeat = std::rc::Rc::new(std::cell::Cell::new(RepeatMode::Off));
    let toggle_repeat = gio::SimpleAction::new("toggle_repeat", None);
    toggle_repeat.connect_activate(clone!(
        #[weak]
        bar,
        #[strong]
        repeat,
        move |_, _| {
            repeat.set(match repeat.get() {
                RepeatMode::Off => RepeatMode::Context,
                RepeatMode::Context => RepeatMode::Track,
                RepeatMode::Track => RepeatMode::Off,
            });
            bar.set_repeat_mode(repeat.get());
        }
    ));
    let app = gio::SimpleActionGroup::new();
    app.add_action(&toggle_shuffle);
    app.add_action(&toggle_repeat);
    window.insert_action_group("app", Some(&app));

    let shuffle_row = part("more_shuffle").downcast::<gtk::Button>().unwrap();
    let repeat_row = part("more_repeat").downcast::<gtk::Button>().unwrap();
    let shuffle_check = part("more_shuffle_check");
    let repeat_check = part("more_repeat_check");
    let repeat_icon = part("more_repeat_icon").downcast::<gtk::Image>().unwrap();
    assert_eq!(
        shuffle_row.action_name().as_deref(),
        Some("app.toggle_shuffle"),
        "the shuffle row runs the action of Ctrl+S"
    );
    assert_eq!(
        repeat_row.action_name().as_deref(),
        Some("app.toggle_repeat"),
        "the repeat row runs the action of Ctrl+R"
    );
    let popover = part("eq_popover").downcast::<gtk::Popover>().unwrap();
    popover.popup();
    wait_mapped(&shuffle_row, 500);
    assert!(
        shuffle_row.is_mapped() && repeat_row.is_mapped(),
        "the rows show in More Options at 572 px"
    );
    assert!(!shuffle_check.get_visible() && !repeat_check.get_visible());
    shuffle_row.emit_clicked();
    pump(300);
    assert!(
        shuffled.get() && shuffle_check.get_visible(),
        "the shuffle row turns shuffle on and shows its mark"
    );
    assert!(popover.is_visible(), "the menu stays open after a row");
    for (mode, icon, on) in [
        (RepeatMode::Context, "media-playlist-repeat-symbolic", true),
        (
            RepeatMode::Track,
            "media-playlist-repeat-song-symbolic",
            true,
        ),
        (
            RepeatMode::Off,
            "media-playlist-consecutive-symbolic",
            false,
        ),
    ]
    .iter()
    .copied()
    {
        repeat_row.emit_clicked();
        pump(100);
        assert_eq!(repeat.get(), mode);
        assert_eq!(
            repeat_icon.icon_name().as_deref(),
            Some(icon),
            "the repeat row's icon follows the mode"
        );
        assert_eq!(
            repeat_check.get_visible(),
            on,
            "the repeat row's mark follows the mode"
        );
    }
    repeat_row.emit_clicked();
    pump(500);
    distinct_colors(popover.upcast_ref(), "player-bar-0572-more-options");
    bar.set_shuffled(false);
    assert!(
        !shuffle_check.get_visible(),
        "the shuffle mark follows the bar"
    );
    popover.popdown();
    pump(300);

    // More Options > Sleep Timer › shows the timer's choices, and they stay
    // up: 572 is within 38 px of a tier's edge (560), where the round sleep
    // timer button would push the bar into the next tier, and that bar
    // hides its parts for a frame, closing any menu open from it
    let chosen = std::rc::Rc::new(std::cell::Cell::new(None));
    bar.connect_sleep_timer(clone!(
        #[strong]
        chosen,
        move |mode| chosen.set(Some(mode))
    ));
    let sleep_row = part("more_sleep_timer").downcast::<gtk::Button>().unwrap();
    // The menu slides to the timer's page, which the headless display may
    // never finish drawing: without the slide, its render shows that page
    let gtk_settings = gtk::Settings::default().unwrap();
    let animations = gtk_settings.is_gtk_enable_animations();
    gtk_settings.set_gtk_enable_animations(false);
    popover.popup();
    wait_mapped(&sleep_row, 500);
    sleep_row.emit_clicked();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while std::time::Instant::now() < deadline {
        window.queue_draw();
        pump(100);
    }
    let choice = ["more_sleep_45", "sleep_45_btn"]
        .iter()
        .filter_map(|id| by_id(root, id))
        .find(|button| button.is_mapped());
    let after = shoot_bar(&bar, "player-bar-0572-sleep-timer-menu", 572);
    let shows = |id: &str| after.shown.iter().any(|part| *part == id);
    check(
        "572: More Options > Sleep Timer shows the timer's choices, and they stay up over the same bar",
        choice.is_some() && shows("lyrics_button") && after.needs <= after.bar,
        format!(
            "choices shown {}; bar {}, its row needs {}; shows {}",
            choice.is_some(),
            after.bar,
            after.needs,
            after.shown.join(" ")
        ),
    );
    if let Some(choice) = &choice {
        if let Some(menu) = choice.ancestor(gtk::Popover::static_type()) {
            distinct_colors(&menu, "player-bar-0572-sleep-timer-menu");
        }
        if let Some(button) = choice.downcast_ref::<gtk::Button>() {
            button.emit_clicked();
        }
        pump(300);
    }
    let sleep_popover = part("sleep_popover");
    check(
        "572: 45 min from there sets the timer and closes the menu",
        chosen.get() == Some(crate::app::components::SleepTimerMode::Minutes(45))
            && !popover.is_visible()
            && !sleep_popover.is_visible(),
        format!(
            "chose {:?}; More Options open {}, the timer's own menu open {}",
            chosen.get(),
            popover.is_visible(),
            sleep_popover.is_visible()
        ),
    );
    gtk_settings.set_gtk_enable_animations(animations);
    window.destroy();

    // For screen readers the rows are toggles from the start, and repeat's
    // row tells which repeat it is (all songs or this one)
    let fresh: crate::app::components::shell::playback::PlaybackWidget = glib::Object::new();
    let fresh_root = fresh.upcast_ref::<gtk::Widget>();
    let row = |id: &str| by_id(fresh_root, id).unwrap_or_else(|| panic!("the bar has no {}", id));
    let toggles = ["more_shuffle", "more_repeat"]
        .iter()
        .all(|id| gtk::test_accessible_has_state(&row(id), gtk::AccessibleState::Pressed));
    fresh.set_repeat_mode(RepeatMode::Track);
    let told = gtk::test_accessible_has_property(
        &row("more_repeat"),
        gtk::AccessibleProperty::Description,
    );
    check(
        "the shuffle and repeat rows are toggles for screen readers from the start, and repeat's tells its mode",
        toggles && told,
        format!("toggles from the start {}, repeat mode told {}", toggles, told),
    );
    style.set_color_scheme(scheme);
    eprintln!("bar more options rows checked");
}

/// The Equalizer entry in More Options hides whenever playback uses Soloist
/// or a remote Spotify Connect device, and exits the presets subpage if it
/// was open; Audio Settings points directly at Preferences > Audio.
fn assert_more_options_equalizer_visibility() {
    force_dark();
    no_animations();
    let bar: crate::app::components::shell::playback::PlaybackWidget = glib::Object::new();
    let root = bar.upcast_ref::<gtk::Widget>();
    let eq_row = by_id(root, "eq_open_presets")
        .expect("eq_open_presets button")
        .downcast::<gtk::Button>()
        .unwrap();
    let audio_settings = by_id(root, "eq_open_settings")
        .expect("eq_open_settings button")
        .downcast::<gtk::Button>()
        .unwrap();
    let stack = by_id(root, "more_stack")
        .expect("more_stack")
        .downcast::<gtk::Stack>()
        .unwrap();

    check(
        "Audio Settings opens Preferences directly on the Audio page",
        audio_settings.action_name().as_deref() == Some("win.preferences-audio"),
        format!("{:?}", audio_settings.action_name()),
    );
    check(
        "Equalizer row starts visible for local playback",
        eq_row.get_visible(),
        format!("visible={}", eq_row.get_visible()),
    );

    eq_row.emit_clicked();
    pump(50);
    check(
        "clicking Equalizer opens the presets subpage",
        stack.visible_child_name().as_deref() == Some("presets"),
        format!("{:?}", stack.visible_child_name()),
    );

    bar.set_equalizer_available(false);
    check(
        "hiding the Equalizer removes the row and returns the menu from presets to main",
        !eq_row.get_visible() && stack.visible_child_name().as_deref() == Some("main"),
        format!(
            "visible={}, page={:?}",
            eq_row.get_visible(),
            stack.visible_child_name()
        ),
    );

    bar.set_equalizer_available(true);
    check(
        "re-enabling the Equalizer restores the row",
        eq_row.get_visible(),
        format!("visible={}", eq_row.get_visible()),
    );
}
