use canto_api::SubsonicConfig;
use canto_config::settings::subsonic_settings;
use gettextrs::gettext;
use gio::prelude::*;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::CompositeTemplate;
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;
use url::Url;

use crate::app::components::EventListener;
use crate::app::state::{LoginEvent, LoginStartedEvent};
use crate::app::AppEvent;

use super::LoginModel;
mod imp {

    use libadwaita::subclass::prelude::AdwWindowImpl;

    use super::*;

    #[derive(Debug, Default, CompositeTemplate)]
    #[template(resource = "/io/github/gusthanks/Canto/components/login.ui")]
    pub struct LoginWindow {
        #[template_child]
        pub login_with_spotify_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub use_local_folder_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub connect_subsonic_toggle: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub subsonic_revealer: TemplateChild<gtk::Revealer>,

        #[template_child]
        pub subsonic_url_entry: TemplateChild<libadwaita::EntryRow>,

        #[template_child]
        pub subsonic_user_entry: TemplateChild<libadwaita::EntryRow>,

        #[template_child]
        pub subsonic_pass_entry: TemplateChild<libadwaita::PasswordEntryRow>,

        #[template_child]
        pub subsonic_connect_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub subsonic_status_label: TemplateChild<gtk::Label>,

        #[template_child]
        pub status_stack: TemplateChild<gtk::Stack>,

        #[template_child]
        pub custom_client_id_toggle: TemplateChild<gtk::ToggleButton>,

        #[template_child]
        pub custom_client_id_revealer: TemplateChild<gtk::Revealer>,

        #[template_child]
        pub custom_client_id_entry: TemplateChild<libadwaita::EntryRow>,

        #[template_child]
        pub copy_redirect_uri_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub custom_client_id_hint: TemplateChild<gtk::Label>,

        pub syncing_client_id: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LoginWindow {
        const NAME: &'static str = "LoginWindow";
        type Type = super::LoginWindow;
        type ParentType = libadwaita::Window;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for LoginWindow {
        fn constructed(&self) {
            self.parent_constructed();
            self.obj().setup_custom_client_id();
        }
    }
    impl WidgetImpl for LoginWindow {}
    impl AdwWindowImpl for LoginWindow {}
    impl WindowImpl for LoginWindow {}
}

glib::wrapper! {
    pub struct LoginWindow(ObjectSubclass<imp::LoginWindow>) @extends gtk::Widget, gtk::Window, libadwaita::Window,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Native, gtk::Root, gtk::ShortcutManager;
}

impl Default for LoginWindow {
    fn default() -> Self {
        Self::new()
    }
}

impl LoginWindow {
    pub fn new() -> Self {
        glib::Object::new()
    }

    fn setup_custom_client_id(&self) {
        let imp = self.imp();
        let weak_self = self.downgrade();
        imp.custom_client_id_entry.connect_changed(move |entry| {
            let Some(window) = weak_self.upgrade() else {
                return;
            };
            if window.imp().syncing_client_id.get() || !entry.is_sensitive() {
                return;
            }
            canto_config::settings::set_spotify_client_id(entry.text().as_str());
        });

        imp.copy_redirect_uri_button.connect_clicked(|button| {
            button.clipboard().set_text(canto_auth::REDIRECT_URI);
            button.set_icon_name("object-select-symbolic");
            let weak_btn = button.downgrade();
            glib::timeout_add_local_once(Duration::from_millis(1500), move || {
                if let Some(btn) = weak_btn.upgrade() {
                    btn.set_icon_name("edit-copy-symbolic");
                }
            });
        });

        self.sync_custom_client_id();
    }

    /// Loads the persisted Client ID (or `CANTO_SPOTIFY_CLIENT_ID` override)
    /// into the discreet advanced section on the login window.
    pub fn sync_custom_client_id(&self) {
        let env_override = std::env::var(canto_auth::ENV_CLIENT_ID)
            .ok()
            .map(|v| v.trim().to_string())
            .filter(|v| !v.is_empty());
        let saved = canto_config::settings::spotify_client_id();
        self.apply_custom_client_id_state(env_override.as_deref(), saved.as_deref());
    }

    fn apply_custom_client_id_state(&self, env_override: Option<&str>, saved: Option<&str>) {
        let imp = self.imp();
        imp.syncing_client_id.set(true);
        if let Some(env_id) = env_override {
            imp.custom_client_id_entry.set_text(env_id);
            imp.custom_client_id_entry.set_sensitive(false);
            // Translators: Shown on the login window when the CANTO_SPOTIFY_CLIENT_ID environment variable overrides the Client ID field.
            imp.custom_client_id_hint.set_label(&gettext(
                "Using CANTO_SPOTIFY_CLIENT_ID from the environment.",
            ));
            imp.custom_client_id_toggle.set_active(true);
        } else {
            let value = saved.unwrap_or("");
            imp.custom_client_id_entry.set_text(value);
            imp.custom_client_id_entry.set_sensitive(true);
            // Translators: Hint under the optional custom Spotify Client ID field on the login window.
            imp.custom_client_id_hint.set_label(&gettext(
                "Optional. Leave empty to use Canto’s built-in Client ID.",
            ));
            imp.custom_client_id_toggle.set_active(!value.is_empty());
        }
        imp.syncing_client_id.set(false);
    }

    #[cfg(test)]
    pub(crate) fn custom_client_id_parts_for_tests(
        &self,
    ) -> (
        gtk::ToggleButton,
        gtk::Revealer,
        libadwaita::EntryRow,
        gtk::Label,
    ) {
        let imp = self.imp();
        (
            imp.custom_client_id_toggle.get(),
            imp.custom_client_id_revealer.get(),
            imp.custom_client_id_entry.get(),
            imp.custom_client_id_hint.get(),
        )
    }

    #[cfg(test)]
    pub(crate) fn apply_custom_client_id_for_tests(
        &self,
        env_override: Option<&str>,
        saved: Option<&str>,
    ) {
        self.apply_custom_client_id_state(env_override, saved);
    }

    fn connect_close<F>(&self, on_close: F)
    where
        F: Fn() + 'static,
    {
        let window = self.upcast_ref::<libadwaita::Window>();
        window.connect_close_request(move |_| {
            on_close();
            glib::Propagation::Proceed
        });
    }

    fn connect_login_oauth_spotify<F>(&self, on_login_with_spotify_button: F)
    where
        F: Fn() + 'static,
    {
        self.imp()
            .login_with_spotify_button
            .connect_clicked(move |_| on_login_with_spotify_button());
    }

    fn connect_local_folder<F>(&self, on_choose: F)
    where
        F: Fn(Vec<PathBuf>) + 'static,
    {
        let weak_win = self.downgrade();
        let on_choose = Rc::new(on_choose);
        self.imp()
            .use_local_folder_button
            .connect_clicked(move |_| {
                let Some(win) = weak_win.upgrade() else {
                    return;
                };
                let dialog = gtk::FileDialog::builder()
                    .title(gettext("Choose Music Folder"))
                    .modal(true)
                    .build();
                let cb = Rc::clone(&on_choose);
                dialog.select_folder(Some(&win), gio::Cancellable::NONE, move |res| {
                    if let Ok(folder) = res {
                        if let Some(path) = folder.path() {
                            cb(vec![path]);
                        }
                    }
                });
            });
    }

    fn connect_subsonic<F>(&self, on_connect: F)
    where
        F: Fn(SubsonicConfig) + 'static,
    {
        let weak_win = self.downgrade();
        let on_connect = Rc::new(on_connect);
        self.imp()
            .subsonic_connect_button
            .connect_clicked(move |btn| {
                let Some(win) = weak_win.upgrade() else {
                    return;
                };
                let imp = win.imp();
                let cfg = SubsonicConfig {
                    server_url: imp.subsonic_url_entry.text().trim().to_string(),
                    username: imp.subsonic_user_entry.text().trim().to_string(),
                    password: imp.subsonic_pass_entry.text().to_string(),
                };
                if !cfg.is_valid() {
                    imp.subsonic_status_label
                        .set_label(&gettext("Enter URL, username, and password."));
                    return;
                }
                btn.set_sensitive(false);
                imp.subsonic_status_label.set_label(&gettext("Connecting…"));
                let weak_win2 = win.downgrade();
                let btn_clone = btn.clone();
                let cb = Rc::clone(&on_connect);
                glib::spawn_future_local(async move {
                    let test_cfg = cfg.clone();
                    let res = tokio::spawn(async move {
                        canto_api::test_subsonic_connection(&test_cfg).await
                    })
                    .await;
                    btn_clone.set_sensitive(true);
                    let Some(win) = weak_win2.upgrade() else {
                        return;
                    };
                    match res {
                        Ok(Ok(_ver)) => {
                            win.imp().subsonic_status_label.set_label("");
                            cb(cfg);
                        }
                        Ok(Err(e)) => {
                            win.imp().subsonic_status_label.set_label(&format!("{e}"));
                        }
                        Err(e) => {
                            win.imp().subsonic_status_label.set_label(&format!("{e}"));
                        }
                    }
                });
            });
    }

    fn sync_subsonic_fields(&self) {
        let (url, user, pass) = subsonic_settings();
        let imp = self.imp();
        if !url.is_empty() && imp.subsonic_url_entry.text().is_empty() {
            imp.subsonic_url_entry.set_text(&url);
        }
        if !user.is_empty() && imp.subsonic_user_entry.text().is_empty() {
            imp.subsonic_user_entry.set_text(&user);
        }
        if !pass.is_empty() && imp.subsonic_pass_entry.text().is_empty() {
            imp.subsonic_pass_entry.set_text(&pass);
        }
    }

    fn show_auth_error(&self, shown: bool) {
        let widget = self.imp();
        widget
            .status_stack
            .set_visible_child_name(if shown { "error" } else { "notice" });
    }

    fn show_login_error(&self) {
        self.imp()
            .status_stack
            .set_visible_child_name("login_error");
    }

    fn reset_status(&self) {
        self.imp().status_stack.set_visible_child_name("notice");
    }
}

pub struct Login {
    parent: gtk::Window,
    login_window: LoginWindow,
    model: Rc<LoginModel>,
}

impl Login {
    pub fn new(parent: gtk::Window, model: LoginModel) -> Self {
        let model = Rc::new(model);

        let login_window = LoginWindow::new();

        login_window.connect_close(clone!(
            #[weak]
            parent,
            move || {
                if let Some(app) = parent.application() {
                    app.quit();
                }
            }
        ));

        login_window.connect_login_oauth_spotify(clone!(
            #[weak]
            model,
            move || {
                model.login_with_spotify();
            }
        ));

        login_window.connect_local_folder(clone!(
            #[weak]
            model,
            move |folders| {
                model.login_with_local_folders(folders);
            }
        ));

        login_window.connect_subsonic(clone!(
            #[weak]
            model,
            move |cfg| {
                model.login_with_subsonic(cfg);
            }
        ));

        Self {
            parent,
            login_window,
            model,
        }
    }

    fn window(&self) -> &libadwaita::Window {
        self.login_window.upcast_ref::<libadwaita::Window>()
    }

    fn show_self(&self) {
        self.login_window.sync_custom_client_id();
        self.login_window.sync_subsonic_fields();
        self.window().set_transient_for(Some(&self.parent));
        self.window().set_modal(true);
        self.window().set_visible(true);
    }

    fn hide(&self) {
        self.login_window.reset_status();
        self.window().set_visible(false);
    }

    fn reveal_not_premium(&self) {
        self.show_self();
        self.login_window.show_auth_error(true);
    }

    fn reveal_error(&self) {
        self.show_self();
        self.login_window.show_login_error();
    }

    fn open_login_url(&self, url: Url) {
        if open::that(url.as_str()).is_err() {
            warn!("Could not open login page");
        }
    }
}

impl EventListener for Login {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::LoginEvent(LoginEvent::LoginCompleted) => {
                self.hide();
            }
            AppEvent::LoginEvent(LoginEvent::NotPremium) => {
                self.reveal_not_premium();
            }
            AppEvent::LoginEvent(LoginEvent::LoginFailed) => {
                self.reveal_error();
            }
            AppEvent::LoginEvent(LoginEvent::LoginStarted(LoginStartedEvent::OpenUrl(url))) => {
                self.open_login_url(url.clone());
            }
            AppEvent::Started => {
                self.model.try_autologin();
            }
            AppEvent::LoginEvent(LoginEvent::LogoutCompleted) => {
                self.show_self();
            }
            _ => {}
        }
    }
}
