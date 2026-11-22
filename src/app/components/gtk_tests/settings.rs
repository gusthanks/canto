//! The preferences, Soloist guide/banner, and login advanced Client ID option.

use super::*;

gtk_tests! {
    genius_guide_copies_each_value => assert_genius_guide_copies_each_value,
    genius_lyrics_switch => assert_genius_lyrics_switch,
    lastfm_love_switch => assert_lastfm_love_switch,
    preferences_pages_and_search => assert_preferences_pages_and_search,
    soloist_guide_and_banner => assert_soloist_guide_and_banner,
    login_custom_client_id => assert_login_custom_client_id,
    theme_and_cover_accent => assert_theme_and_cover_accent,
    visual_bridge_runtime => assert_visual_bridge_runtime,
}

/// The Genius guide offers a copy button for each value to type in.
fn assert_genius_guide_copies_each_value() {
    let guide = crate::app::components::genius_guide_for_tests();
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
        use crate::app::components::genius_fallback;
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
    // The sample render (a PNG only with CANTO_TEST_DUMP_DIR)
    {
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
    // The sample render (a PNG only with CANTO_TEST_DUMP_DIR)
    {
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

/// Preferences has built-in search enabled, actually finds rows across pages
/// while skipping hidden librespot rows in external-player mode, and is
/// organized into three pages: General, Audio, and Integrations.
fn assert_preferences_pages_and_search() {
    use libadwaita::prelude::{AdwDialogExt, ComboRowExt, PreferencesDialogExt};
    force_dark();
    no_animations();
    let dialog = SettingsDialog::new();
    check(
        "Preferences enables built-in search",
        dialog.is_search_enabled(),
        format!("search-enabled={}", dialog.is_search_enabled()),
    );
    let pages = dialog.page_names_for_tests();
    check(
        "Preferences splits into General, Audio, and Integrations pages",
        pages == vec!["general", "audio", "integrations"],
        format!("{:?}", pages),
    );

    dialog.set_visible_page_name("audio");
    check(
        "Preferences switches to the Audio page by name",
        dialog.visible_page_name().as_deref() == Some("audio"),
        format!("{:?}", dialog.visible_page_name()),
    );

    let window = libadwaita::Window::new();
    window.set_default_size(720, 760);
    window.present();
    dialog.present(Some(&window));

    // Verify both local-player and external-player (Soloist) Audio page layouts
    dialog.set_external_player_for_tests(false);
    wait_frames(&window, 3);
    distinct_colors(window.upcast_ref(), "settings-audio-local-sample");

    let gsettings_cf = gio::Settings::new(canto_config::settings::SETTINGS);
    gsettings_cf.reset(canto_config::settings::CROSSFADE_SECONDS);
    let crossfade_row = dialog.crossfade_seconds_row();
    check(
        "Crossfade duration SpinRow starts at 0 seconds and writes live updates",
        crossfade_row.value() as u32 == 0,
        format!("value={}", crossfade_row.value()),
    );
    crossfade_row.adjustment().set_value(5.0);
    pump(50);
    check(
        "Crossfade SpinRow updates crossfade-seconds GSetting",
        gsettings_cf.uint(canto_config::settings::CROSSFADE_SECONDS) == 5,
        format!(
            "gsettings={}",
            gsettings_cf.uint(canto_config::settings::CROSSFADE_SECONDS)
        ),
    );
    gsettings_cf.reset(canto_config::settings::CROSSFADE_SECONDS);

    // Verify Music Source switcher in Integrations hides/shows Local vs Subsonic rows and Spotify-only Soloist group.
    let gsettings_src = gio::Settings::new(canto_config::settings::SETTINGS);
    gsettings_src.reset(canto_config::settings::MUSIC_SOURCE);
    let source_combo = dialog.music_source_combo_row();
    let local_row = dialog.local_folders_action_row();
    let subsonic_row = dialog.subsonic_url_entry_row();
    let soloist_group = dialog.soloist_group_for_tests();
    check(
        "Music source starts on Spotify with Local/Subsonic rows hidden",
        source_combo.selected() == 0
            && !local_row.is_visible()
            && !subsonic_row.is_visible()
            && soloist_group.parent().is_some(),
        format!(
            "sel={}, local={}, sub={}, soloist={}",
            source_combo.selected(),
            local_row.is_visible(),
            subsonic_row.is_visible(),
            soloist_group.parent().is_some()
        ),
    );
    source_combo.set_selected(1);
    pump(50);
    check(
        "Selecting Local Files shows local folder row and detaches Spotify Soloist group",
        local_row.is_visible() && !subsonic_row.is_visible() && soloist_group.parent().is_none(),
        format!(
            "local={}, sub={}, soloist_parent={}",
            local_row.is_visible(),
            subsonic_row.is_visible(),
            soloist_group.parent().is_some()
        ),
    );
    source_combo.set_selected(2);
    pump(50);
    check(
        "Selecting Navidrome / Subsonic shows Subsonic fields and hides local row",
        !local_row.is_visible() && subsonic_row.is_visible() && soloist_group.parent().is_none(),
        format!(
            "local={}, sub={}, soloist_parent={}",
            local_row.is_visible(),
            subsonic_row.is_visible(),
            soloist_group.parent().is_some()
        ),
    );
    source_combo.set_selected(0);
    gsettings_src.reset(canto_config::settings::MUSIC_SOURCE);
    pump(50);

    dialog.set_external_player_for_tests(true);
    wait_frames(&window, 3);
    distinct_colors(window.upcast_ref(), "settings-audio-sample");

    dialog.set_visible_page_name("general");
    wait_frames(&window, 3);
    distinct_colors(window.upcast_ref(), "settings-general-sample");

    // Actually exercise Preferences built-in search across pages.
    fn find_search_widgets(
        root: &gtk::Widget,
    ) -> (Option<gtk::ToggleButton>, Option<gtk::SearchEntry>) {
        let mut toggle = None;
        let mut entry = None;
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if toggle.is_none() {
                if let Some(btn) = w.downcast_ref::<gtk::ToggleButton>() {
                    if btn.icon_name().as_deref() == Some("system-search-symbolic")
                        || btn.icon_name().as_deref() == Some("edit-find-symbolic")
                    {
                        toggle = Some(btn.clone());
                    }
                }
            }
            if entry.is_none() {
                if let Some(se) = w.downcast_ref::<gtk::SearchEntry>() {
                    entry = Some(se.clone());
                }
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                stack.push(c);
            }
        }
        (toggle, entry)
    }

    fn visible_search_result_titles(root: &gtk::Widget) -> Vec<String> {
        let mut titles = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if !w.get_visible() || !w.is_mapped() {
                continue;
            }
            if let Some(row) = w.downcast_ref::<libadwaita::ActionRow>() {
                let t = libadwaita::prelude::PreferencesRowExt::title(row).to_string();
                if !t.is_empty() {
                    titles.push(t);
                }
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                stack.push(c);
            }
        }
        titles
    }

    let (search_toggle, search_entry) = find_search_widgets(dialog.upcast_ref());
    check(
        "Preferences exposes a search button and SearchEntry",
        search_toggle.is_some() && search_entry.is_some(),
        format!(
            "toggle={}, entry={}",
            search_toggle.is_some(),
            search_entry.is_some()
        ),
    );
    if let (Some(toggle), Some(entry)) = (search_toggle, search_entry) {
        toggle.set_active(true);
        wait_frames(&window, 2);

        entry.set_text("Soloist");
        wait_frames(&window, 3);
        let results = visible_search_result_titles(dialog.upcast_ref());
        check(
            "Preferences search finds Soloist rows",
            results.iter().any(|t| t.contains("Soloist")),
            format!("results={:?}", results),
        );

        entry.set_text("Scrobbling");
        wait_frames(&window, 3);
        let results = visible_search_result_titles(dialog.upcast_ref());
        check(
            "Preferences search finds Integrations rows across pages",
            results.iter().any(|t| t.contains("Scrobbling")),
            format!("results={:?}", results),
        );

        entry.set_text("Cover Accent");
        wait_frames(&window, 3);
        let results = visible_search_result_titles(dialog.upcast_ref());
        check(
            "Preferences search finds Cover Accent on Controls",
            results.iter().any(|t| t.contains("Cover Accent")),
            format!("results={:?}", results),
        );

        // In external-player mode, detached librespot-only rows must not appear in search.
        for hidden_query in ["Gapless", "Pre-gain", "Access Point", "Audio Cache"] {
            entry.set_text(hidden_query);
            wait_frames(&window, 3);
            let results = visible_search_result_titles(dialog.upcast_ref());
            check(
                &format!(
                    "Preferences search skips hidden librespot row '{}' in external-player mode",
                    hidden_query
                ),
                !results.iter().any(|t| t.contains(hidden_query)),
                format!("results={:?}", results),
            );
        }

        // In local-player mode, local librespot rows reappear in search.
        dialog.set_external_player_for_tests(false);
        entry.set_text("");
        wait_frames(&window, 2);
        entry.set_text("Gapless");
        wait_frames(&window, 3);
        let results = visible_search_result_titles(dialog.upcast_ref());
        check(
            "Preferences search finds local Audio rows when playing locally",
            results.iter().any(|t| t.contains("Gapless")),
            format!("results={:?}", results),
        );
    }

    dialog.force_close();
    window.destroy();
}

/// The Soloist setup guide adapts to x86_64 vs non-x86_64 architectures, opens
/// cleanly as a real Adw.Dialog whose "Soloist Settings" button triggers
/// `win.preferences-audio`, and the contextual Soloist banner in the main
/// window fits from 360 px up.
fn assert_soloist_guide_and_banner() {
    use crate::connect::soloist::AutoDownloadSupport;
    use gio::prelude::ActionMapExt;
    use libadwaita::prelude::AdwDialogExt;
    force_dark();
    no_animations();

    fn collect_texts(root: &gtk::Widget) -> (Vec<String>, Vec<String>) {
        let mut labels = Vec::new();
        let mut buttons = Vec::new();
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if let Some(btn) = w.downcast_ref::<gtk::Button>() {
                if let Some(label) = btn.label() {
                    buttons.push(label.to_string());
                }
            } else if let Some(lbl) = w.downcast_ref::<gtk::Label>() {
                labels.push(lbl.text().to_string());
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                stack.push(c);
            }
        }
        (labels, buttons)
    }

    fn find_button_by_label(root: &gtk::Widget, target: &str) -> Option<gtk::Button> {
        let mut stack = vec![root.clone()];
        while let Some(w) = stack.pop() {
            if let Some(btn) = w.downcast_ref::<gtk::Button>() {
                if btn.label().as_deref() == Some(target) {
                    return Some(btn.clone());
                }
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                child = c.next_sibling();
                stack.push(c);
            }
        }
        None
    }

    let x86_guide =
        crate::app::components::soloist_guide_for_tests(AutoDownloadSupport::Available, true);
    let (x86_labels, x86_buttons) = collect_texts(&x86_guide);
    check(
        "x86_64 Soloist guide explains automatic download and includes Soloist Settings button",
        x86_labels
            .iter()
            .any(|t| t.contains("downloads Soloist automatically"))
            && x86_buttons.iter().any(|b| b == "Soloist Settings")
            && x86_buttons.iter().any(|b| b == "Open Dashboard"),
        format!("labels={:?}, buttons={:?}", x86_labels, x86_buttons),
    );

    let arm_guide = crate::app::components::soloist_guide_for_tests(
        AutoDownloadSupport::UnsupportedArch,
        false,
    );
    let (arm_labels, arm_buttons) = collect_texts(&arm_guide);
    check(
        "ARM64 Soloist guide explains manual install and omits Settings button when opened from Settings",
        arm_labels
            .iter()
            .any(|t| t.contains("x86_64") && t.contains("CANTO_SOLOIST_CMD"))
            && !arm_buttons.iter().any(|b| b == "Soloist Settings"),
        format!("labels={:?}, buttons={:?}", arm_labels, arm_buttons),
    );

    // Mount the real Adw.Dialog inside an Adw.ApplicationWindow and verify both
    // narrow (360 px) and standard (560 px) widths plus the Soloist Settings action.
    let host = libadwaita::ApplicationWindow::builder().build();
    let audio_prefs_opened = std::rc::Rc::new(std::cell::Cell::new(false));
    let opened_flag = std::rc::Rc::clone(&audio_prefs_opened);
    let audio_action = gio::SimpleAction::new("preferences-audio", None);
    audio_action.connect_activate(move |_, _| opened_flag.set(true));
    host.add_action(&audio_action);

    host.set_default_size(560, 520);
    host.present();
    let guide_dialog = crate::app::components::soloist_guide_dialog_for_tests(
        &host,
        AutoDownloadSupport::Available,
        true,
    );
    guide_dialog.present(Some(&host));
    wait_frames(&host, 3);
    distinct_colors(host.upcast_ref(), "soloist-guide-sample");

    host.set_default_size(360, 520);
    wait_frames(&host, 3);

    let settings_btn = find_button_by_label(guide_dialog.upcast_ref(), "Soloist Settings");
    check(
        "Soloist guide dialog contains Soloist Settings button",
        settings_btn.is_some(),
        format!("found={}", settings_btn.is_some()),
    );
    if let Some(btn) = settings_btn {
        btn.emit_clicked();
        wait_frames(&host, 2);
        check(
            "clicking Soloist Settings activates win.preferences-audio",
            audio_prefs_opened.get(),
            format!("activated={}", audio_prefs_opened.get()),
        );
    }
    guide_dialog.force_close();
    host.destroy();

    // Check the contextual Soloist banner in window.ui at narrow and wide widths
    let builder = gtk::Builder::from_resource("/io/github/gusthanks/Canto/window.ui");
    let main: libadwaita::ApplicationWindow = builder.object("window").unwrap();
    main.add_action(&gio::SimpleAction::new("soloist-guide", None));
    let revealer: gtk::Revealer = builder.object("soloist_banner_revealer").unwrap();
    let banner: gtk::Box = builder.object("soloist_banner").unwrap();
    let action: gtk::Button = builder.object("soloist_banner_action").unwrap();
    let dismiss: gtk::Button = builder.object("soloist_banner_dismiss").unwrap();

    check(
        "Soloist banner starts hidden and points directly at win.soloist-guide",
        !revealer.reveals_child()
            && action.action_name().as_deref() == Some("win.soloist-guide")
            && dismiss.get_visible(),
        format!(
            "reveals={}, action={:?}, dismiss_visible={}",
            revealer.reveals_child(),
            action.action_name(),
            dismiss.get_visible()
        ),
    );

    revealer.set_reveal_child(true);
    for (width, tag) in [
        (360, Some("soloist-banner-360-sample")),
        (583, None),
        (1024, Some("soloist-banner-sample")),
    ]
    .iter()
    .copied()
    {
        main.set_default_size(width, 600);
        main.present();
        wait_mapped(&banner, 300);
        wait_frames(&main, 3);
        let bounds = banner.compute_bounds(&main);
        let fits = bounds
            .is_some_and(|b| b.width() > 0.0 && b.x() + b.width() <= main.width() as f32 + 0.5);
        check(
            &format!("Soloist banner fits inside a {} px window", width),
            fits,
            format!("window={}, bounds={:?}", main.width(), bounds),
        );
        if let Some(name) = tag {
            distinct_colors(main.upcast_ref(), name);
        }
    }
    main.destroy();
}

/// The login window hides the custom Spotify Client ID option by default,
/// persists trimmed values to GSettings, and reflects CANTO_SPOTIFY_CLIENT_ID
/// without overwriting the saved setting.
fn assert_login_custom_client_id() {
    use canto_config::settings::{spotify_client_id, SETTINGS, SPOTIFY_CLIENT_ID};
    force_dark();
    no_animations();

    let gsettings = gio::Settings::new(SETTINGS);
    gsettings.reset(SPOTIFY_CLIENT_ID);

    let login = crate::app::components::LoginWindow::new();
    let (toggle, revealer, entry, hint) = login.custom_client_id_parts_for_tests();
    login.apply_custom_client_id_for_tests(None, None);

    check(
        "custom Client ID section starts collapsed when no custom ID is configured",
        !toggle.is_active() && !revealer.reveals_child() && entry.is_sensitive(),
        format!(
            "active={}, reveals={}, sensitive={}",
            toggle.is_active(),
            revealer.reveals_child(),
            entry.is_sensitive()
        ),
    );

    login.present();
    wait_mapped(&toggle, 300);
    wait_frames(&login, 2);
    distinct_colors(login.upcast_ref(), "login-default-sample");

    toggle.set_active(true);
    wait_frames(&login, 2);
    entry.set_text("  custom-client-id-42  ");
    pump(50);

    check(
        "typing a custom Client ID reveals the field and persists the trimmed value",
        revealer.reveals_child() && spotify_client_id().as_deref() == Some("custom-client-id-42"),
        format!(
            "reveals={}, saved={:?}",
            revealer.reveals_child(),
            spotify_client_id()
        ),
    );
    distinct_colors(login.upcast_ref(), "login-custom-client-id-sample");

    login.apply_custom_client_id_for_tests(Some("env-client-id-99"), Some("custom-client-id-42"));
    pump(50);
    check(
        "CANTO_SPOTIFY_CLIENT_ID locks the entry without overwriting the saved GSettings value",
        !entry.is_sensitive()
            && entry.text() == "env-client-id-99"
            && hint.text().contains("CANTO_SPOTIFY_CLIENT_ID")
            && spotify_client_id().as_deref() == Some("custom-client-id-42"),
        format!(
            "sensitive={}, text={}, hint={}, saved={:?}",
            entry.is_sensitive(),
            entry.text(),
            hint.text(),
            spotify_client_id()
        ),
    );

    login.destroy();
    gsettings.reset(SPOTIFY_CLIENT_ID);
}

/// Color Theme describes the full system cascade, "Cover Accent on Controls"
/// follows "Adaptive Cover Colors" sensitivity and GSettings persistence, and
/// ThemeEngine resolves Follow System live from native fallback to pywal/cover
/// accents without leaking into unrelated themes.
fn assert_theme_and_cover_accent() {
    use crate::app::theme::{wcag_contrast_ratio_hex, SystemThemePaths, ThemeEngine};
    use canto_config::settings::{
        ADAPTIVE_COVER_ACCENT, ADAPTIVE_COVER_COLORS, APP_THEME, SETTINGS,
    };
    use libadwaita::prelude::{ActionRowExt, AdwWindowExt};
    use std::cell::Cell;
    use std::rc::Rc;

    force_dark();
    no_animations();

    let gsettings = gio::Settings::new(SETTINGS);
    gsettings.reset(APP_THEME);
    gsettings.reset(ADAPTIVE_COVER_COLORS);
    gsettings.reset(ADAPTIVE_COVER_ACCENT);

    let dialog = SettingsDialog::new();
    let (app_theme, cover_colors, cover_accent) = dialog.theme_rows_for_tests();
    let subtitle = app_theme.subtitle().unwrap_or_default().to_string();
    check(
        "Color Theme subtitle documents Omarchy, matugen, pywal, wallust, and desktop theme",
        subtitle.contains("Omarchy")
            && subtitle.contains("matugen")
            && subtitle.contains("pywal")
            && subtitle.contains("wallust"),
        format!("subtitle={:?}", subtitle),
    );
    check(
        "Cover Accent on Controls starts off and sensitive while Adaptive Cover Colors is on",
        cover_colors.is_active() && !cover_accent.is_active() && cover_accent.is_sensitive(),
        format!(
            "colors_active={}, accent_active={}, accent_sensitive={}",
            cover_colors.is_active(),
            cover_accent.is_active(),
            cover_accent.is_sensitive()
        ),
    );

    cover_colors.set_active(false);
    pump(40);
    check(
        "Turning off Adaptive Cover Colors disables Cover Accent on Controls",
        !cover_accent.is_sensitive() && !gsettings.boolean(ADAPTIVE_COVER_COLORS),
        format!(
            "sensitive={}, gsettings={}",
            cover_accent.is_sensitive(),
            gsettings.boolean(ADAPTIVE_COVER_COLORS)
        ),
    );

    cover_colors.set_active(true);
    pump(40);
    cover_accent.set_active(true);
    pump(40);
    check(
        "Turning on Cover Accent on Controls persists to GSettings",
        cover_accent.is_sensitive() && gsettings.boolean(ADAPTIVE_COVER_ACCENT),
        format!(
            "sensitive={}, gsettings={}",
            cover_accent.is_sensitive(),
            gsettings.boolean(ADAPTIVE_COVER_ACCENT)
        ),
    );
    drop(dialog);

    // Exercise ThemeEngine with isolated temp paths: native fallback -> live pywal -> cover accent.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let dir = std::env::temp_dir().join(format!(
        "canto_gtk_theme_test_{}_{}",
        std::process::id(),
        nanos
    ));
    let wal_dir = dir.join("wal");
    std::fs::create_dir_all(&wal_dir).expect("create wal dir");
    let pywal_path = wal_dir.join("colors.json");
    let paths = SystemThemePaths {
        omarchy: dir.join("omarchy/current/theme/colors.toml"),
        matugen: vec![dir.join("matugen/colors.json")],
        pywal: vec![pywal_path.clone()],
        wallust: vec![dir.join("wallust/colors.json")],
    };

    gsettings.set_string(APP_THEME, "omarchy").unwrap();
    gsettings.set_boolean(ADAPTIVE_COVER_COLORS, true).unwrap();
    gsettings.set_boolean(ADAPTIVE_COVER_ACCENT, false).unwrap();

    let engine = ThemeEngine::install_with_paths(paths);
    let snap = engine.current_snapshot();
    check(
        "Follow System without palette files falls back to native system theme (not SpotifyGreen)",
        snap.kind == "system"
            && libadwaita::StyleManager::default().color_scheme()
                == libadwaita::ColorScheme::Default,
        format!(
            "kind={}, scheme={:?}",
            snap.kind,
            libadwaita::StyleManager::default().color_scheme()
        ),
    );

    // Live file monitor: creating pywal colors.json in the watched directory updates ThemeEngine.
    let notified = Rc::new(Cell::new(0u32));
    let n_clone = Rc::clone(&notified);
    engine.subscribe(Rc::new(move || {
        n_clone.set(n_clone.get() + 1);
    }));
    std::fs::write(
        &pywal_path,
        r##"{"special":{"background":"#121316","foreground":"#c5c8c6"},"colors":{"color4":"#5f819d"}}"##,
    )
    .expect("write pywal colors");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    while engine.current_snapshot().kind != "pywal" && std::time::Instant::now() < deadline {
        pump(30);
    }
    let pywal_snap = engine.current_snapshot();
    check(
        "Follow System live-reloads when pywal colors.json appears",
        pywal_snap.kind == "pywal" && pywal_snap.background == "#121316" && notified.get() > 0,
        format!(
            "kind={}, bg={}, notified={}",
            pywal_snap.kind,
            pywal_snap.background,
            notified.get()
        ),
    );

    // Atomic rename in the same directory (standard write-tmp + rename pattern) also reloads live.
    let tmp_pywal = wal_dir.join("colors.json.tmp");
    std::fs::write(
        &tmp_pywal,
        r##"{"special":{"background":"#1c1824","foreground":"#ece6f5"},"colors":{"color4":"#9b7ee6"}}"##,
    )
    .expect("write tmp pywal colors");
    std::fs::rename(&tmp_pywal, &pywal_path).expect("atomic rename pywal colors");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1200);
    while engine.current_snapshot().background != "#1c1824" && std::time::Instant::now() < deadline
    {
        pump(30);
    }
    let renamed_snap = engine.current_snapshot();
    check(
        "Follow System live-reloads on atomic same-directory rename of colors.json",
        renamed_snap.kind == "pywal" && renamed_snap.background == "#1c1824",
        format!("kind={}, bg={}", renamed_snap.kind, renamed_snap.background),
    );

    // First-time creation of a previously non-existent palette directory (`matugen/`)
    // attaches a monitor live via its parent directory and overrides pywal by priority.
    let matugen_dir = dir.join("matugen");
    let matugen_path = matugen_dir.join("colors.json");
    std::fs::create_dir_all(&matugen_dir).expect("create matugen dir");
    std::fs::write(
        &matugen_path,
        r##"{"colors":{"dark":{"background":"#10131a","on_background":"#e2e2e9","primary":"#a8c7fa"}}}"##,
    )
    .expect("write matugen colors");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while engine.current_snapshot().kind != "matugen" && std::time::Instant::now() < deadline {
        pump(30);
    }
    let matugen_snap = engine.current_snapshot();
    check(
        "Follow System attaches monitor and live-reloads when matugen/ is created for the first time",
        matugen_snap.kind == "matugen" && matugen_snap.background == "#10131a",
        format!("kind={}, bg={}", matugen_snap.kind, matugen_snap.background),
    );

    // Moving a palette file out of its directory (MovedOut) falls back live to pywal.
    let moved_out = dir.join("matugen_moved.json");
    std::fs::rename(&matugen_path, &moved_out).expect("move matugen colors out");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while engine.current_snapshot().kind != "pywal" && std::time::Instant::now() < deadline {
        pump(30);
    }
    let moved_snap = engine.current_snapshot();
    check(
        "Follow System live-reloads and falls back when a higher-priority palette file is moved out",
        moved_snap.kind == "pywal" && moved_snap.background == "#1c1824",
        format!("kind={}, bg={}", moved_snap.kind, moved_snap.background),
    );

    // Replacing the palette directory (`rm -rf` + `mkdir` + write) reattaches monitors live.
    std::fs::remove_dir_all(&wal_dir).expect("remove wal dir");
    std::fs::create_dir_all(&wal_dir).expect("recreate wal dir");
    std::fs::write(
        &pywal_path,
        r##"{"special":{"background":"#101815","foreground":"#e4f2ea"},"colors":{"color4":"#4ec48b"}}"##,
    )
    .expect("write recreated pywal colors");
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1500);
    while engine.current_snapshot().background != "#101815" && std::time::Instant::now() < deadline
    {
        pump(30);
    }
    let replaced_snap = engine.current_snapshot();
    check(
        "Follow System reattaches monitors and reloads after palette directory replacement",
        replaced_snap.kind == "pywal" && replaced_snap.background == "#101815",
        format!(
            "kind={}, bg={}",
            replaced_snap.kind, replaced_snap.background
        ),
    );

    // Enabling Cover Accent on Controls adjusts a dark cover color to WCAG AA (>= 4.5:1)
    // and updates real mounted GTK widgets (.suggested-action and .navigation-row__liked).
    gsettings.set_boolean(ADAPTIVE_COVER_ACCENT, true).unwrap();
    engine.set_cover_accent(Some("#182448".into()));
    pump(40);
    let cover_snap = engine.current_snapshot();
    let ratio = wcag_contrast_ratio_hex(&cover_snap.accent, &cover_snap.background).unwrap_or(0.0);
    check(
        "Dynamic Cover Accent applies contrast-safe accent (>= 4.5:1) to UI snapshot",
        cover_snap.accent != "#182448" && ratio >= 4.5,
        format!(
            "accent={}, bg={}, ratio={:.2}",
            cover_snap.accent, cover_snap.background, ratio
        ),
    );

    // Mount real widgets and verify bright cover accents switch foreground text/icons
    // to dark (`@accent_fg_color`), never unreadable white, on both suggested buttons
    // and `.navigation-row__liked`.
    let preview = libadwaita::Window::new();
    preview.set_default_size(320, 120);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let btn = gtk::Button::with_label("Play");
    btn.add_css_class("suggested-action");
    let liked = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    liked.add_css_class("navigation-row__liked");
    row.append(&btn);
    row.append(&liked);
    preview.set_content(Some(&row));
    preview.present();
    engine.set_cover_accent(Some("#f0e27b".into()));
    wait_frames(&preview, 3);
    let btn_fg = btn.color();
    let liked_fg = liked.color();
    check(
        "Bright cover accent switches .suggested-action and .navigation-row__liked foreground to dark text",
        btn_fg.red() < 0.2
            && btn_fg.green() < 0.2
            && btn_fg.blue() < 0.2
            && liked_fg.red() < 0.2
            && liked_fg.green() < 0.2
            && liked_fg.blue() < 0.2,
        format!("btn_fg={:?}, liked_fg={:?}", btn_fg, liked_fg),
    );
    preview.destroy();

    drop(engine);
    let _ = std::fs::remove_dir_all(&dir);
    gsettings.reset(APP_THEME);
    gsettings.reset(ADAPTIVE_COVER_COLORS);
    gsettings.reset(ADAPTIVE_COVER_ACCENT);
    force_dark();
}

/// End-to-end runtime verification of VisualBridge:
/// - strictly dormant when disabled (zero file writes, level meter off)
/// - publishes active synced lyric immediately on enable, seek, and offset change
/// - clears stale lyric on track change and refreshes when AppEvent::LyricsUpdated arrives
/// - writes a clean inactive frame on toggle-off and on Drop (shutdown)
fn assert_visual_bridge_runtime() {
    use crate::app::components::{EventListener, VisualBridge};
    use crate::app::models::make_track;
    use crate::app::state::{AppState, PlaybackAction, PlaybackEvent};
    use crate::app::theme::{SystemThemePaths, ThemeEngine};
    use crate::app::{AppEvent, AppModel};
    use canto_api::{LyricLine, Lyrics};
    use canto_audio_engine::LevelMeterController;
    use canto_config::settings::{LYRICS_OFFSET_MS, SETTINGS, VISUAL_BRIDGE_ENABLED};
    use std::rc::Rc;
    use std::sync::Arc;

    force_dark();
    no_animations();

    let gsettings = gio::Settings::new(SETTINGS);
    gsettings.set_boolean(VISUAL_BRIDGE_ENABLED, false).unwrap();
    gsettings.set_int(LYRICS_OFFSET_MS, 0).unwrap();

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let tag = format!("{}_{}", std::process::id(), nanos);
    let dir = std::env::temp_dir().join(format!("canto_gtk_vb_test_{tag}"));
    let state_path = dir.join("state.json");

    let token_provider: Arc<dyn canto_api::TokenProvider> = Arc::new(|| None::<String>);
    let api = Arc::new(canto_api::spotify_service(
        Arc::clone(&token_provider),
        token_provider,
        1024 * 1024,
        1024 * 1024,
    ));

    let id1 = format!("canto_vb_t1_{tag}");
    let id2 = format!("canto_vb_t2_{tag}");
    let mut track1 = make_track(&id1);
    track1.title = "First Song".into();
    let mut track2 = make_track(&id2);
    track2.title = "Second Song".into();

    let lyrics_cache = api.lyrics_service().cache().clone();
    let synced1 = Lyrics::Synced(vec![
        LyricLine::new(5_000, "First line"),
        LyricLine::new(15_000, "Second line"),
    ]);
    let plain2 = Lyrics::Plain(vec!["Unsynced line".into()]);
    let synced2 = Lyrics::Synced(vec![LyricLine::new(2_000, "Added synced line")]);
    lyrics_cache.store(&id1, &synced1).expect("store synced1");
    lyrics_cache.store(&id2, &plain2).expect("store plain2");

    let mut state = AppState::new();
    let album = crate::app::SongsSource::Album("vb_album".into());
    state.update_state(PlaybackAction::LoadContextSongs(album, vec![track1, track2]).into());
    state.update_state(PlaybackAction::Load(id1.clone()).into());
    state.update_state(PlaybackAction::SyncSeek(6_000).into());

    let app_model = Rc::new(AppModel::new(state, Arc::clone(&api)));
    let theme_engine = ThemeEngine::install_with_paths(SystemThemePaths {
        omarchy: dir.join("omarchy/current/theme/colors.toml"),
        matugen: vec![dir.join("matugen/colors.json")],
        pywal: vec![dir.join("wal/colors.json")],
        wallust: vec![dir.join("wallust/colors.json")],
    });
    let level_meter = LevelMeterController::new(false);
    let mut bridge = VisualBridge::new_with_path(
        Rc::clone(&app_model),
        Rc::clone(&theme_engine),
        level_meter.clone(),
        state_path.clone(),
    );

    // 1. Strictly dormant when disabled.
    bridge.on_event(&AppEvent::Started);
    bridge.on_event(&AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(
        id1.clone(),
    )));
    bridge.on_event(&AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(6_000)));
    bridge.on_event(&AppEvent::LyricsUpdated(id1.clone()));
    pump(50);
    check(
        "VisualBridge stays dormant with zero disk writes and level meter off when disabled",
        !state_path.exists() && !level_meter.is_enabled(),
        format!(
            "exists={}, meter_enabled={}",
            state_path.exists(),
            level_meter.is_enabled()
        ),
    );

    let read_state = || -> serde_json::Value {
        let text = std::fs::read_to_string(&state_path).expect("state.json readable");
        serde_json::from_str(&text).expect("valid state.json")
    };

    // 2. Enabling mid-track writes active state + synced lyric immediately.
    gsettings.set_boolean(VISUAL_BRIDGE_ENABLED, true).unwrap();
    pump(60);
    let snap1 = read_state();
    check(
        "Enabling VisualBridge mid-track immediately publishes track and active synced lyric",
        snap1["active"] == true
            && snap1["track"]["id"] == id1
            && snap1["lyric"] == "First line"
            && level_meter.is_enabled(),
        format!("snap1={snap1}"),
    );

    // 3. Seek and lyrics offset update `"lyric"` immediately.
    app_model.update_state(PlaybackAction::SyncSeek(16_000).into());
    bridge.on_event(&AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(16_000)));
    let snap_seek = read_state();
    check(
        "Seeking updates the active synced lyric in state.json",
        snap_seek["lyric"] == "Second line",
        format!("snap_seek={snap_seek}"),
    );

    gsettings.set_int(LYRICS_OFFSET_MS, -2_000).unwrap();
    pump(50);
    let snap_offset = read_state();
    check(
        "Changing lyrics-offset-ms shifts the published synced lyric live",
        snap_offset["lyric"] == "First line",
        format!("snap_offset={snap_offset}"),
    );
    gsettings.set_int(LYRICS_OFFSET_MS, 0).unwrap();
    pump(40);

    // 4. Switching to an unsynced track clears stale lyrics to null.
    app_model.update_state(PlaybackAction::Load(id2.clone()).into());
    app_model.update_state(PlaybackAction::SyncSeek(4_000).into());
    bridge.on_event(&AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(
        id2.clone(),
    )));
    let snap2 = read_state();
    check(
        "Switching to a track without synced lyrics clears stale lyric to null",
        snap2["track"]["id"] == id2 && snap2["lyric"].is_null(),
        format!("snap2={snap2}"),
    );

    // 5. Publishing/reloading synced lyrics for the active track updates state.json via LyricsUpdated.
    lyrics_cache.store(&id2, &synced2).expect("store synced2");
    bridge.on_event(&AppEvent::LyricsUpdated(id2.clone()));
    let snap2_updated = read_state();
    check(
        "AppEvent::LyricsUpdated refreshes state.json with newly added synced lyrics",
        snap2_updated["lyric"] == "Added synced line",
        format!("snap2_updated={snap2_updated}"),
    );

    // 6. Disabling writes a clean inactive frame once and turns off level meter.
    gsettings.set_boolean(VISUAL_BRIDGE_ENABLED, false).unwrap();
    pump(50);
    let snap_off = read_state();
    check(
        "Turning VisualBridge off writes a clean inactive frame and disables level meter",
        snap_off["active"] == false
            && snap_off["playing"] == false
            && snap_off["track"].is_null()
            && snap_off["lyric"].is_null()
            && !level_meter.is_enabled(),
        format!("snap_off={snap_off}"),
    );

    // 7. Re-enabling then dropping VisualBridge (app exit) writes a clean inactive frame.
    gsettings.set_boolean(VISUAL_BRIDGE_ENABLED, true).unwrap();
    pump(50);
    check(
        "Re-enabling VisualBridge restores active state",
        read_state()["active"] == true,
        format!("state={}", read_state()),
    );
    drop(bridge);
    let snap_dropped = read_state();
    check(
        "Dropping an active VisualBridge writes a clean inactive frame on shutdown",
        snap_dropped["active"] == false
            && snap_dropped["playing"] == false
            && snap_dropped["track"].is_null()
            && snap_dropped["lyric"].is_null()
            && !level_meter.is_enabled(),
        format!("snap_dropped={snap_dropped}"),
    );

    drop(theme_engine);
    let _ = std::fs::remove_file(lyrics_cache.cache_path(&id1));
    let _ = std::fs::remove_file(lyrics_cache.cache_path(&id2));
    let _ = std::fs::remove_dir_all(&dir);
    gsettings.reset(VISUAL_BRIDGE_ENABLED);
    gsettings.reset(LYRICS_OFFSET_MS);
    force_dark();
}
