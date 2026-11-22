mod close_dialog;
mod shell_box;
pub use shell_box::{expose_widgets, ShellBox};

use gio::prelude::SettingsExt;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::app::components::EventListener;
use crate::app::state::{LoginEvent, SettingsEvent};
use crate::app::{AppEvent, AppModel};
use crate::settings::{CloseWindowBehavior, WindowGeometry};

use canto_config::settings::{SETTINGS, SOLOIST_ENABLED};

thread_local! {
    static WINDOW_GEOMETRY: RefCell<WindowGeometry> = const { RefCell::new(WindowGeometry {
        width: 0, height: 0, is_maximized: false
    }) };
}

/// Whether the contextual Soloist setup banner should show.
pub(crate) fn should_show_soloist_banner(
    logged_in: bool,
    needs_official: bool,
    soloist_enabled: bool,
    key_saved: bool,
    dismissed: bool,
) -> bool {
    logged_in && needs_official && soloist_enabled && !key_saved && !dismissed
}

pub struct MainWindow {
    initial_window_geometry: WindowGeometry,
    window: libadwaita::ApplicationWindow,
    logged_in: Rc<Cell<bool>>,
    soloist_banner_revealer: gtk::Revealer,
    soloist_dismissed: Rc<Cell<bool>>,
    official_client_detected: Rc<Cell<bool>>,
    _soloist_settings: gio::Settings,
}

impl MainWindow {
    pub fn new(
        initial_window_geometry: WindowGeometry,
        app_model: Rc<AppModel>,
        window: libadwaita::ApplicationWindow,
        soloist_banner_revealer: gtk::Revealer,
        soloist_banner_dismiss: gtk::Button,
    ) -> Self {
        window.connect_close_request(clone!(
            #[weak]
            app_model,
            #[upgrade_or]
            glib::Propagation::Proceed,
            move |window| {
                let state = app_model.get_state();
                if !state.playback.is_playing() {
                    return glib::Propagation::Proceed;
                }

                let settings = gio::Settings::new(SETTINGS);
                let behavior = CloseWindowBehavior::from_gsettings_enum(
                    settings.enum_("close-window-behavior"),
                );

                match behavior {
                    CloseWindowBehavior::MinimizeToBackground => {
                        window.set_visible(false);
                        glib::Propagation::Stop
                    }
                    CloseWindowBehavior::StopAndQuit => glib::Propagation::Proceed,
                    CloseWindowBehavior::Ask => {
                        close_dialog::show(window);
                        glib::Propagation::Stop
                    }
                }
            }
        ));

        window.connect_default_height_notify(Self::save_window_geometry);
        window.connect_default_width_notify(Self::save_window_geometry);
        window.connect_maximized_notify(Self::save_window_geometry);

        window.connect_unrealize(clone!(
            #[weak]
            app_model,
            move |_| {
                debug!("saving geometry");
                WINDOW_GEOMETRY.with(|g| g.borrow().save());
                Self::keep_last_played(&app_model);
            }
        ));

        let logged_in = Rc::new(Cell::new(false));
        let soloist_dismissed = Rc::new(Cell::new(false));
        let official_client_detected = Rc::new(Cell::new(false));

        {
            let revealer = soloist_banner_revealer.downgrade();
            let dismissed = Rc::clone(&soloist_dismissed);
            soloist_banner_dismiss.connect_clicked(move |_| {
                dismissed.set(true);
                if let Some(r) = revealer.upgrade() {
                    r.set_reveal_child(false);
                }
            });
        }

        let soloist_settings = gio::Settings::new(SETTINGS);
        {
            let revealer = soloist_banner_revealer.downgrade();
            let logged_in = Rc::clone(&logged_in);
            let dismissed = Rc::clone(&soloist_dismissed);
            let detected = Rc::clone(&official_client_detected);
            soloist_settings.connect_changed(Some(SOLOIST_ENABLED), move |_, _| {
                if let Some(r) = revealer.upgrade() {
                    sync_soloist_banner(&r, &logged_in, &detected, &dismissed);
                }
            });
        }

        Self {
            initial_window_geometry,
            window,
            logged_in,
            soloist_banner_revealer,
            soloist_dismissed,
            official_client_detected,
            _soloist_settings: soloist_settings,
        }
    }

    fn refresh_soloist_banner(&self) {
        sync_soloist_banner(
            &self.soloist_banner_revealer,
            &self.logged_in,
            &self.official_client_detected,
            &self.soloist_dismissed,
        );
    }

    /// Keeps the open track and its spot for the next start: Spotify forgets
    /// them once Canto's player stops.
    fn keep_last_played(app_model: &AppModel) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        let line = {
            let state = app_model.get_state();
            crate::app::components::last_played_line(&state.playback, now)
        };
        if let Some(line) = line {
            debug!("keeping the last track: {}", line.replace('\t', " "));
            let settings = gio::Settings::new(SETTINGS);
            let _ = settings.set_string(canto_config::settings::LAST_PLAYED, &line);
            gio::Settings::sync();
        }
    }

    fn start(&self) {
        self.window.set_default_size(
            self.initial_window_geometry.width,
            self.initial_window_geometry.height,
        );
        if self.initial_window_geometry.is_maximized {
            self.window.maximize();
        }
    }

    fn raise(&self) {
        if self.logged_in.get() {
            self.window.present();
        }
    }

    fn show(&self) {
        self.logged_in.set(true);
        self.window.present();
        self.refresh_soloist_banner();
    }

    fn hide(&self) {
        self.logged_in.set(false);
        self.soloist_dismissed.set(false);
        self.official_client_detected.set(false);
        self.soloist_banner_revealer.set_reveal_child(false);
        self.window.set_visible(false);
    }

    fn save_window_geometry<W: GtkWindowExt>(window: &W) {
        let (width, height) = window.default_size();
        let is_maximized = window.is_maximized();
        WINDOW_GEOMETRY.with(|g| {
            let mut g = g.borrow_mut();
            g.is_maximized = is_maximized;
            if !is_maximized {
                g.width = width;
                g.height = height;
            }
        });
    }
}

fn sync_soloist_banner(
    revealer: &gtk::Revealer,
    logged_in: &Rc<Cell<bool>>,
    official_client_detected: &Rc<Cell<bool>>,
    dismissed: &Rc<Cell<bool>>,
) {
    let needs_official =
        official_client_detected.get() || crate::connect::account_needs_official_client();
    let enabled = crate::connect::soloist::is_enabled();
    if !should_show_soloist_banner(
        logged_in.get(),
        needs_official,
        enabled,
        false,
        dismissed.get(),
    ) {
        revealer.set_reveal_child(false);
        return;
    }

    let revealer = revealer.downgrade();
    let logged_in = Rc::clone(logged_in);
    let detected = Rc::clone(official_client_detected);
    let dismissed = Rc::clone(dismissed);
    glib::spawn_future_local(async move {
        let key_saved = gio::spawn_blocking(canto_auth::soloist_key::is_saved_blocking)
            .await
            .unwrap_or(false);
        let Some(revealer) = revealer.upgrade() else {
            return;
        };
        let still_needs = detected.get() || crate::connect::account_needs_official_client();
        let still_enabled = crate::connect::soloist::is_enabled();
        revealer.set_reveal_child(should_show_soloist_banner(
            logged_in.get(),
            still_needs,
            still_enabled,
            key_saved,
            dismissed.get(),
        ));
    });
}

impl EventListener for MainWindow {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::Started => self.start(),
            AppEvent::Raised => self.raise(),
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => self.show(),
            AppEvent::LoginEvent(LoginEvent::LogoutCompleted) => self.hide(),
            AppEvent::OfficialClientRequested => {
                self.official_client_detected.set(true);
                self.refresh_soloist_banner();
            }
            AppEvent::SettingsEvent(SettingsEvent::SoloistKeyChanged) => {
                self.refresh_soloist_banner();
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::should_show_soloist_banner;

    #[test]
    fn soloist_banner_shows_only_when_needed_enabled_and_unconfigured() {
        assert!(should_show_soloist_banner(true, true, true, false, false));
        assert!(!should_show_soloist_banner(false, true, true, false, false));
        assert!(!should_show_soloist_banner(true, false, true, false, false));
        assert!(!should_show_soloist_banner(true, true, false, false, false));
        assert!(!should_show_soloist_banner(true, true, true, true, false));
        assert!(!should_show_soloist_banner(true, true, true, false, true));
    }
}
