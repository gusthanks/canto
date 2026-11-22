//! The preferences, Soloist guide/banner, and login advanced Client ID option.

use super::*;

gtk_tests! {
    genius_guide_copies_each_value => assert_genius_guide_copies_each_value,
    genius_lyrics_switch => assert_genius_lyrics_switch,
    lastfm_love_switch => assert_lastfm_love_switch,
    preferences_pages_and_search => assert_preferences_pages_and_search,
    soloist_guide_and_banner => assert_soloist_guide_and_banner,
    login_custom_client_id => assert_login_custom_client_id,
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
    use libadwaita::prelude::{AdwDialogExt, PreferencesDialogExt};
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
