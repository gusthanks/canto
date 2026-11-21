#![allow(mismatched_lifetime_syntaxes)]
// Edition 2018 traps, as errors: a lone "{x}" in assert!/panic! is printed
// literally instead of formatted, and array.into_iter() yields references.
#![deny(non_fmt_panics, array_into_iter)]
#[macro_use(clone)]
extern crate glib;
#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;
extern crate gettextrs;

use app::state::{HomePage, Route};
use futures::channel::mpsc::UnboundedSender;
use gettextrs::*;
use gio::prelude::*;
use gio::ApplicationFlags;
use gio::SimpleAction;
use gtk::prelude::*;
use std::sync::atomic::{AtomicBool, Ordering};

mod app;
mod config;
mod connect;
mod dbus;
mod inhibitor;
mod player;
mod session_log;
mod settings;

use crate::app::components::expose_custom_widgets;
use crate::app::dispatch::DispatchLoop;
use crate::app::{state::PlaybackAction, App, AppAction, BrowserAction};

// Steps of the seek and volume shortcuts
const SEEK_STEP_MS: i32 = 5_000;
const VOLUME_STEP: f64 = 0.05;
#[cfg(target_env = "gnu")]
const MALLOC_ARENAS: libc::c_int = 4;

fn main() {
    #[cfg(target_env = "gnu")]
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, MALLOC_ARENAS);
    }

    let migrated = canto_config::settings::migrate_from_old_app_id();
    if migrated > 0 {
        // Before the logger starts
        eprintln!("canto: copied {migrated} settings from the old app id");
    }
    let settings = settings::CantoSettings::new_from_gsettings().unwrap_or_default();
    setup_gtk(&settings);

    // Looks like there's a side effect to declaring widgets that allows them to be referenced them in ui/blueprint files
    // so here goes!
    expose_custom_widgets();

    let gtk_app = gtk::Application::new(Some(config::APPID), ApplicationFlags::HANDLES_OPEN);
    let builder = gtk::Builder::from_resource("/io/github/gusthanks/Canto/window.ui");
    let window: libadwaita::ApplicationWindow = builder.object("window").unwrap();

    // In debug mode, the app id is different (see meson config) so we fix the resource path (and add a distinctive style)
    // Having a different app id allows running both the stable and development version at the same time
    if cfg!(debug_assertions) {
        window.add_css_class("devel");
        gtk_app.set_resource_base_path(Some("/io/github/gusthanks/Canto"));
    }

    let context = glib::MainContext::default();
    let dispatch_loop = DispatchLoop::new();
    let sender = dispatch_loop.make_dispatcher();

    // Couple of actions used with shortcuts
    register_actions(&gtk_app, sender.clone());
    // Displayed as the accelerator hint next to "Preferences" in the main menu.
    gtk_app.set_accels_for_action("win.preferences", &["<Ctrl>comma"]);
    // Displayed as the accelerator hint next to "Keyboard Shortcuts" in the main menu.
    gtk_app.set_accels_for_action("win.show-shortcuts", &["<Ctrl>question"]);
    // Displayed as the accelerator hint next to "Command Palette" in the main menu.
    gtk_app.set_accels_for_action("win.command-palette", &["<Ctrl>k"]);
    app::components::setup_about(builder.object::<libadwaita::AboutDialog>("about").unwrap());
    setup_arrow_shortcuts(&window, builder.object("arrow_shortcuts").unwrap());

    // Main app logic is hooked up here
    let app = App::new(settings, builder, sender.clone());
    context.spawn_local(app.attach(dispatch_loop));

    let sender_clone = sender.clone();
    gtk_app.connect_activate(move |gtk_app| {
        debug!("activate");
        if window.application().is_some() {
            // The main window, even when the mini player is the active one
            // or the main window was hidden for it
            window.set_visible(true);
            window.present();
        } else {
            // Only send the Start action if we've just created the window
            window.set_application(Some(gtk_app));
            gtk_app.add_window(&window);
            sender_clone.unbounded_send(AppAction::Start).unwrap();
        }
    });

    // Canto plays through a hidden Spotify app on accounts that need it:
    // quitting Canto quits that app too, so it doesn't keep playing unseen
    gtk_app.connect_shutdown(|_| {
        connect::quit_official_client();
    });

    gtk_app.connect_open(move |gtk_app, targets, _| {
        gtk_app.activate();

        // There should only be one target because %u is used in desktop file
        let Some(target) = targets.first() else {
            return;
        };
        let uri = target.uri().to_string();
        let action = AppAction::OpenURI(uri)
            .unwrap_or_else(|| AppAction::ShowNotification(gettext("Failed to open link!")));
        sender.unbounded_send(action).unwrap();
    });

    context.invoke_local(move || {
        gtk_app.run();
    });

    if RESTART_REQUESTED.load(Ordering::Relaxed) {
        relaunch();
    }

    std::process::exit(0);
}

static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

fn relaunch() {
    use std::os::unix::process::CommandExt;

    let Ok(exe) = std::env::current_exe() else {
        error!("Cannot restart: unknown executable path");
        return;
    };
    let err = std::process::Command::new(exe)
        .args(std::env::args_os().skip(1))
        .exec();
    error!("Cannot restart: {err}");
}

#[derive(Debug, PartialEq, Eq)]
struct LanguageEnvPlan {
    language_var: &'static str,
    locale_candidates: &'static [&'static str],
}

const ORIG_LOCALE_SAVED_ENV: &str = "CANTO_ORIG_LOCALE_SAVED";
const ORIG_LANGUAGE_ENV: &str = "CANTO_ORIG_LANGUAGE";
const ORIG_LC_ALL_ENV: &str = "CANTO_ORIG_LC_ALL";
const ORIG_LC_MESSAGES_ENV: &str = "CANTO_ORIG_LC_MESSAGES";
const ORIG_LANG_ENV: &str = "CANTO_ORIG_LANG";
const UNSET_ENV_SENTINEL: &str = "__CANTO_UNSET__";

fn is_c_locale(locale: &str) -> bool {
    let base = locale.split(['.', '@']).next().unwrap_or(locale).trim();
    base.eq_ignore_ascii_case("C") || base.eq_ignore_ascii_case("POSIX")
}

fn snapshot_original_locale_env() {
    if std::env::var_os(ORIG_LOCALE_SAVED_ENV).is_some() {
        return;
    }
    for (orig_key, backup_key) in [
        ("LANGUAGE", ORIG_LANGUAGE_ENV),
        ("LC_ALL", ORIG_LC_ALL_ENV),
        ("LC_MESSAGES", ORIG_LC_MESSAGES_ENV),
        ("LANG", ORIG_LANG_ENV),
    ] {
        match std::env::var_os(orig_key) {
            Some(val) => std::env::set_var(backup_key, val),
            None => std::env::set_var(backup_key, UNSET_ENV_SENTINEL),
        }
    }
    std::env::set_var(ORIG_LOCALE_SAVED_ENV, "1");
}

fn restore_original_locale_env() {
    if std::env::var_os(ORIG_LOCALE_SAVED_ENV).is_none() {
        return;
    }
    for (orig_key, backup_key) in [
        ("LANGUAGE", ORIG_LANGUAGE_ENV),
        ("LC_ALL", ORIG_LC_ALL_ENV),
        ("LC_MESSAGES", ORIG_LC_MESSAGES_ENV),
        ("LANG", ORIG_LANG_ENV),
    ] {
        if let Some(val) = std::env::var_os(backup_key) {
            if val == UNSET_ENV_SENTINEL {
                std::env::remove_var(orig_key);
            } else {
                std::env::set_var(orig_key, val);
            }
        }
    }
}

fn language_env_plan(pref: &str) -> Option<LanguageEnvPlan> {
    match pref.trim() {
        "en" => Some(LanguageEnvPlan {
            // Trailing `:C` stops GNU gettext from falling back to host LC_MESSAGES
            // when no English `.mo` catalog exists on the system.
            language_var: "en_US:en:C",
            locale_candidates: &["en_US.UTF-8", "en_US.utf8", "C.UTF-8", "C.utf8", ""],
        }),
        "pt_BR" | "pt-br" => Some(LanguageEnvPlan {
            language_var: "pt_BR:pt-br:pt_PT:pt",
            // GNU gettext ignores `LANGUAGE` whenever `LC_MESSAGES` is `C`/`C.UTF-8`,
            // so try non-C UTF-8 locales before falling back to the environment `""`.
            locale_candidates: &[
                "pt_BR.UTF-8",
                "pt_BR.utf8",
                "en_US.UTF-8",
                "en_US.utf8",
                "",
                "C.UTF-8",
            ],
        }),
        _ => None,
    }
}

fn apply_language_preference() {
    let pref = gio::SettingsSchemaSource::default()
        .and_then(|src| src.lookup(canto_config::settings::SETTINGS, true))
        .map(|_| {
            gio::Settings::new(canto_config::settings::SETTINGS)
                .string(canto_config::settings::APP_LANGUAGE)
                .to_string()
        })
        .unwrap_or_else(|| "system".to_string());

    apply_language_preference_for(&pref);
}

fn apply_language_preference_for(pref: &str) {
    snapshot_original_locale_env();

    if let Some(plan) = language_env_plan(pref) {
        // Ensure `LC_ALL` does not override `LC_MESSAGES` when `gtk::init()` later calls `setlocale(LC_ALL, "")`.
        if let Some(orig_lc_all) = std::env::var_os("LC_ALL") {
            if std::env::var_os("LANG").is_none() {
                std::env::set_var("LANG", &orig_lc_all);
            }
            std::env::remove_var("LC_ALL");
        }
        std::env::set_var("LANGUAGE", plan.language_var);

        let needs_non_c_locale = !plan.language_var.split(':').any(|item| item == "C");
        let mut chosen_locale: Option<String> = None;

        // First try to obtain a non-C locale when `LANGUAGE` translations need to be honored.
        if needs_non_c_locale {
            for candidate in plan.locale_candidates {
                if let Some(bytes) = unsafe { setlocale(LocaleCategory::LcMessages, *candidate) } {
                    let resolved = String::from_utf8_lossy(&bytes).into_owned();
                    if !is_c_locale(&resolved) {
                        chosen_locale = Some(resolved);
                        break;
                    }
                }
            }
        }

        if chosen_locale.is_none() {
            for candidate in plan.locale_candidates {
                if let Some(bytes) = unsafe { setlocale(LocaleCategory::LcMessages, *candidate) } {
                    let resolved = String::from_utf8_lossy(&bytes).into_owned();
                    chosen_locale = Some(resolved);
                    break;
                }
            }
        }

        if let Some(locale) = chosen_locale {
            if !locale.is_empty() {
                std::env::set_var("LC_MESSAGES", locale);
            }
        }
    } else {
        restore_original_locale_env();
        unsafe {
            setlocale(LocaleCategory::LcAll, "");
        }
    }
}

/// Canto's caches used to live under ~/.cache/riff: move what is there to
/// ~/.cache/canto once, so nothing has to be downloaded again.
fn move_old_cache() {
    move_old_cache_in(&glib::user_cache_dir());
}

fn move_old_cache_in(cache: &std::path::Path) {
    let old = cache.join("riff");
    if !old.is_dir() {
        return;
    }
    let new = cache.join("canto");
    if let Err(e) = std::fs::create_dir_all(&new) {
        log::warn!("Could not create {}: {e}", new.display());
        return;
    }
    for part in ["img", "net", "librespot"] {
        let (from, to) = (old.join(part), new.join(part));
        if from.exists() && !to.exists() {
            match std::fs::rename(&from, &to) {
                Ok(()) => log::info!("Moved {} to {}", from.display(), to.display()),
                Err(e) => log::warn!("Could not move {}: {e}", from.display()),
            }
        }
    }
    // Only goes if nothing else is left in it
    let _ = std::fs::remove_dir(&old);
}

fn setup_gtk(settings: &settings::CantoSettings) {
    // Setup logging (also kept on disk, see session_log)
    session_log::init();
    move_old_cache();

    // Set application and program names so desktop environment and window manager identify Canto
    glib::set_application_name("Canto");
    glib::set_prgname(Some("canto"));

    // Apply interface language override before initializing gettext and GTK.
    apply_language_preference();

    // Setup translations
    textdomain("canto")
        .and_then(|_| bindtextdomain("canto", config::LOCALEDIR))
        .and_then(|_| bind_textdomain_codeset("canto", "UTF-8"))
        .expect("Could not setup localization");

    // Setup Gtk, Adwaita...
    gtk::init().unwrap_or_else(|_| panic!("Failed to initialize GTK"));
    libadwaita::init().unwrap_or_else(|_| panic!("Failed to initialize libadwaita"));

    let manager = libadwaita::StyleManager::default();
    manager.set_color_scheme(settings.theme_preference);

    let res = gio::Resource::load(config::PKGDATADIR.to_owned() + "/canto.gresource")
        .expect("Could not load resources");
    gio::resources_register(&res);

    let provider = gtk::CssProvider::new();
    provider.load_from_resource("/io/github/gusthanks/Canto/app.css");

    gtk::style_context_add_provider_for_display(
        &gdk::Display::default().unwrap(),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

fn setup_arrow_shortcuts(
    window: &libadwaita::ApplicationWindow,
    shortcuts: gtk::ShortcutController,
) {
    window.connect_focus_widget_notify(move |window| {
        let typing = GtkWindowExt::focus(window)
            .is_some_and(|focus| focus.is::<gtk::Text>() || focus.is::<gtk::TextView>());
        shortcuts.set_propagation_phase(if typing {
            gtk::PropagationPhase::None
        } else {
            gtk::PropagationPhase::Capture
        });
    });
}

fn register_actions(app: &gtk::Application, sender: UnboundedSender<AppAction>) {
    let quit = SimpleAction::new("quit", None);
    quit.connect_activate(clone!(
        #[weak]
        app,
        move |_, _| {
            if let Some(existing_window) = app.active_window() {
                existing_window.close();
            }
            app.quit();
        }
    ));
    app.add_action(&quit);

    let restart = SimpleAction::new("restart", None);
    restart.connect_activate(clone!(
        #[weak]
        app,
        move |_, _| {
            RESTART_REQUESTED.store(true, Ordering::Relaxed);
            app.quit();
        }
    ));
    app.add_action(&restart);

    let resize_window =
        SimpleAction::new("resize_window", Some(glib::VariantTy::new("(ii)").unwrap()));
    resize_window.connect_activate(clone!(
        #[weak]
        app,
        move |_, parameter| {
            let Some((width, height)) = parameter.and_then(|p| p.get::<(i32, i32)>()) else {
                return;
            };
            if let Some(window) = app.active_window() {
                window.unmaximize();
                window.set_default_size(width, height);
            }
        }
    ));
    app.add_action(&resize_window);

    app.add_action(&make_action(
        "toggle_playback",
        PlaybackAction::TogglePlay.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "player_prev",
        PlaybackAction::Previous.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "player_next",
        PlaybackAction::Next.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_shuffle",
        PlaybackAction::ToggleShuffle.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_repeat",
        PlaybackAction::ToggleRepeat.into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "seek_backward",
        PlaybackAction::SeekBy(-SEEK_STEP_MS).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "seek_forward",
        PlaybackAction::SeekBy(SEEK_STEP_MS).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "volume_down",
        PlaybackAction::AdjustVolume(-VOLUME_STEP).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "volume_up",
        PlaybackAction::AdjustVolume(VOLUME_STEP).into(),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "toggle_mute",
        PlaybackAction::ToggleMute.into(),
        sender.clone(),
    ));

    // An artist's page by id: the artist links under album covers
    let view_artist = SimpleAction::new("view_artist", Some(glib::VariantTy::STRING));
    view_artist.connect_activate(clone!(
        #[strong]
        sender,
        move |_, parameter| {
            if let Some(id) = parameter.and_then(|p| p.get::<String>()) {
                sender.unbounded_send(AppAction::ViewArtist(id)).unwrap();
            }
        }
    ));
    app.add_action(&view_artist);

    for (name, page) in [
        ("nav_artists", HomePage::SavedArtists),
        ("nav_albums", HomePage::Library),
        ("nav_playlists", HomePage::SavedPlaylists),
        ("nav_tracks", HomePage::SavedTracks),
    ] {
        // Same as picking the page in the navigation panel
        app.add_action(&make_action(
            name,
            BrowserAction::Navigate(Route::Home(page)).into(),
            sender.clone(),
        ));
    }

    app.add_action(&make_action(
        "nav_back",
        AppAction::BrowserAction(BrowserAction::NavigationBack),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "nav_forward",
        AppAction::BrowserAction(BrowserAction::NavigationForward),
        sender.clone(),
    ));

    app.add_action(&make_action(
        "search",
        AppAction::BrowserAction(BrowserAction::Navigate(Route::Search)),
        sender.clone(),
    ));
}

fn make_action(
    name: &str,
    app_action: AppAction,
    sender: UnboundedSender<AppAction>,
) -> SimpleAction {
    let action = SimpleAction::new(name, None);
    action.connect_activate(move |_, _| {
        sender.unbounded_send(app_action.clone()).unwrap();
    });
    action
}

#[cfg(test)]
mod language_tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_test_env() {
        for k in [
            "LANGUAGE",
            "LC_ALL",
            "LC_MESSAGES",
            "LANG",
            ORIG_LOCALE_SAVED_ENV,
            ORIG_LANGUAGE_ENV,
            ORIG_LC_ALL_ENV,
            ORIG_LC_MESSAGES_ENV,
            ORIG_LANG_ENV,
        ] {
            std::env::remove_var(k);
        }
    }

    #[test]
    fn language_env_plan_maps_supported_choices() {
        assert_eq!(language_env_plan("system"), None);
        assert_eq!(
            language_env_plan("en"),
            Some(LanguageEnvPlan {
                language_var: "en_US:en:C",
                locale_candidates: &["en_US.UTF-8", "en_US.utf8", "C.UTF-8", "C.utf8", ""],
            })
        );
        assert_eq!(
            language_env_plan("pt_BR"),
            Some(LanguageEnvPlan {
                language_var: "pt_BR:pt-br:pt_PT:pt",
                locale_candidates: &[
                    "pt_BR.UTF-8",
                    "pt_BR.utf8",
                    "en_US.UTF-8",
                    "en_US.utf8",
                    "",
                    "C.UTF-8",
                ],
            })
        );
    }

    #[test]
    fn apply_language_handles_pt_br_under_c_utf8_and_restores_system_on_relaunch() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_test_env();
        std::env::set_var("LANG", "C.UTF-8");
        std::env::set_var("LC_ALL", "C.UTF-8");

        apply_language_preference_for("pt_BR");
        assert_eq!(
            std::env::var("LANGUAGE").as_deref(),
            Ok("pt_BR:pt-br:pt_PT:pt")
        );
        assert!(std::env::var_os("LC_ALL").is_none());
        if let Ok(lc_msg) = std::env::var("LC_MESSAGES") {
            if unsafe { setlocale(LocaleCategory::LcMessages, "en_US.UTF-8") }.is_some() {
                assert!(
                    !is_c_locale(&lc_msg),
                    "expected non-C LC_MESSAGES, got {}",
                    lc_msg
                );
            }
        }

        apply_language_preference_for("system");
        assert!(std::env::var_os("LANGUAGE").is_none());
        assert_eq!(std::env::var("LC_ALL").as_deref(), Ok("C.UTF-8"));
        assert_eq!(std::env::var("LANG").as_deref(), Ok("C.UTF-8"));

        clear_test_env();
    }

    #[test]
    fn apply_language_en_preserves_stop_fallback_and_restores_caller_language() {
        let _guard = ENV_LOCK.lock().unwrap();
        clear_test_env();
        std::env::set_var("LANG", "en_US.UTF-8");
        std::env::set_var("LANGUAGE", "es:pt_BR");

        apply_language_preference_for("en");
        assert_eq!(std::env::var("LANGUAGE").as_deref(), Ok("en_US:en:C"));

        apply_language_preference_for("system");
        assert_eq!(std::env::var("LANGUAGE").as_deref(), Ok("es:pt_BR"));

        clear_test_env();
    }
}

#[cfg(test)]
mod old_cache_tests {
    use super::move_old_cache_in;

    #[test]
    fn moves_the_old_cache_without_overwriting() {
        let root = std::env::temp_dir().join(format!("canto-cache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join("riff/img")).unwrap();
        std::fs::write(root.join("riff/img/a"), "old").unwrap();
        std::fs::create_dir_all(root.join("riff/net")).unwrap();
        std::fs::create_dir_all(root.join("canto/net")).unwrap();
        std::fs::write(root.join("canto/net/b"), "new").unwrap();

        move_old_cache_in(&root);

        assert_eq!(
            std::fs::read_to_string(root.join("canto/img/a")).unwrap(),
            "old"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("canto/net/b")).unwrap(),
            "new"
        );
        // The old net stays where it was, so the old folder stays too
        assert!(root.join("riff/net").is_dir());
        let _ = std::fs::remove_dir_all(&root);
    }
}
