//! The bundled resources and icons resolve, and the custom widgets build.

use super::*;

gtk_tests! {
    icons_resolve => assert_icons_resolve,
    resources_are_bundled => assert_resources_are_bundled,
    custom_widgets_build => assert_custom_widgets_build,
    composite_templates_build => assert_composite_templates_build,
}

/// Every icon the UI names resolves through the resource icon path.
fn assert_icons_resolve() {
    let display = gtk::gdk::Display::default().expect("default GDK display must exist");
    let icon_theme = gtk::IconTheme::for_display(&display);
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
    assert!(
        icon_theme.has_icon("edit-copy-symbolic"),
        "copy icon must resolve"
    );
    for icon in ["canto-heart-symbolic", "canto-heart-filled-symbolic"] {
        assert!(icon_theme.has_icon(icon), "{} must resolve", icon);
    }
}

/// The UI files and style sheets the app loads are in the gresource.
fn assert_resources_are_bundled() {
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
}

/// The player bar (by its type name, as the UI files name it), the lyrics
/// page, the preferences (on their Integrations page) and the command
/// palette build from their templates.
fn assert_custom_widgets_build() {
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
}

/// Every widget built from a UI template builds: a template child whose id
/// is missing from its Blueprint file fails it (GTK logs a critical, which
/// the runner counts as a failure). The types private to their module are
/// looked up by name once the others (whose templates use them) are built.
fn assert_composite_templates_build() {
    let build = |type_: glib::Type| {
        let object = glib::Object::with_type(type_);
        assert!(
            object.is::<gtk::Widget>(),
            "{} builds as a widget",
            type_.name()
        );
        eprintln!("template type {} builds", type_.name());
        if let Some(window) = object.downcast_ref::<gtk::Window>() {
            window.destroy();
        }
    };
    let types = [
        crate::app::components::AppHeaderBar::static_type(),
        crate::app::components::BottomSheetContent::static_type(),
        crate::app::components::CardWidget::static_type(),
        crate::app::components::widgets::card_list::CardListWidget::static_type(),
        crate::app::components::HistoryMenuRow::static_type(),
        crate::app::components::shell::navigation_panel::NavigationPanelRow::static_type(),
        crate::app::components::widgets::card_list::card_view_menu::CardViewMenuPopoverWidget::static_type(),
        crate::app::components::CommandPaletteDialog::static_type(),
        crate::app::components::DetailsHeaderWidget::static_type(),
        crate::app::components::DetailsPageWidget::static_type(),
        crate::app::components::DetailsSectionWidget::static_type(),
        crate::app::components::DeviceSelectorWidget::static_type(),
        crate::app::components::DiscHeaderRow::static_type(),
        crate::app::components::Drawer::static_type(),
        crate::app::components::EqualizerWidget::static_type(),
        crate::app::components::LoginWindow::static_type(),
        crate::app::components::LyricsPageWidget::static_type(),
        crate::app::components::PanWidget::static_type(),
        crate::app::components::PitchWidget::static_type(),
        crate::app::components::PlaybackWidget::static_type(),
        crate::app::components::PlaylistDialog::static_type(),
        crate::app::components::PlaylistPicker::static_type(),
        crate::app::components::QueuePageWidget::static_type(),
        crate::app::components::ReleaseDetailsDialog::static_type(),
        crate::app::components::SearchResultsWidget::static_type(),
        crate::app::components::SettingsDialog::static_type(),
        crate::app::components::TrackRow::static_type(),
        crate::app::components::UtilityPanelWidget::static_type(),
    ];
    let built = types.len();
    for type_ in types {
        build(type_);
    }
    let private = [
        "PlaybackControlsWidget",
        "PlaybackInfoWidget",
        "SegmentedButtonWidget",
        "SelectionToolbarWidget",
    ];
    let missing: Vec<_> = private
        .iter()
        .filter(|name| glib::Type::from_name(**name).is_none())
        .collect();
    assert!(
        missing.is_empty(),
        "template types not registered: {:?}",
        missing
    );
    for name in private {
        build(glib::Type::from_name(name).unwrap());
    }
    eprintln!("{} template types built", built + private.len());
}
