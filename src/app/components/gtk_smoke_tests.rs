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
    css.load_from_resource("/dev/diegovsky/Riff/app.css");
    gtk::style_context_add_provider_for_display(
        &display,
        &css,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );

    let icon_theme = gtk::IconTheme::for_display(&display);
    icon_theme.add_resource_path("/dev/diegovsky/Riff/icons");
    assert!(
        icon_theme.has_icon("canto-genius-symbolic"),
        "canto-genius-symbolic icon must resolve via resource icon path"
    );
    assert!(
        icon_theme.has_icon("alarm-symbolic"),
        "alarm-symbolic icon must resolve via resource icon path"
    );

    for res_path in [
        "/dev/diegovsky/Riff/window.ui",
        "/dev/diegovsky/Riff/components/shortcuts.ui",
        "/dev/diegovsky/Riff/components/playback_widget.ui",
        "/dev/diegovsky/Riff/components/lyrics_page.ui",
        "/dev/diegovsky/Riff/components/settings.ui",
        "/dev/diegovsky/Riff/components/command_palette.ui",
        "/dev/diegovsky/Riff/components/command_palette.css",
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
    lyrics.set_about_prompt(
        "Test Track",
        "Test Artist",
        Some("Test Album"),
        "https://genius.com/search?q=Test%20Artist%20Test%20Track",
    );
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
