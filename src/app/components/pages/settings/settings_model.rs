use crate::app::state::{LoginAction, PlaybackAction, SettingsAction, TryLoginAction};
use crate::app::{AppAction, AppModel, Dispatcher};
use crate::settings::CantoSettings;
use canto_api::{SubsonicConfig, TokenProvider};
use gettextrs::gettext;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

pub struct SettingsModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    token_provider: Arc<dyn TokenProvider>,
    session_token_provider: Arc<dyn TokenProvider>,
}

impl SettingsModel {
    pub fn new(
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        token_provider: Arc<dyn TokenProvider>,
        session_token_provider: Arc<dyn TokenProvider>,
    ) -> Self {
        Self {
            app_model,
            dispatcher,
            token_provider,
            session_token_provider,
        }
    }

    pub fn switch_music_source(&self, source: &str) {
        self.stop_player();
        canto_config::settings::set_music_source(source);
        match source {
            "local" => {
                let folders = canto_config::settings::local_music_folders();
                if folders.is_empty() {
                    self.notify(gettext(
                        "Choose a local music folder to browse your library",
                    ));
                    return;
                }
                self.app_model
                    .api()
                    .set_provider(canto_api::local_provider(folders));
                self.dispatcher
                    .dispatch(LoginAction::SetLoginSuccess(gettext("Local Library")).into());
                self.notify(gettext("Switched to Local Music Library"));
            }
            "subsonic" => {
                let (server_url, username, password) = canto_config::settings::subsonic_settings();
                let cfg = SubsonicConfig {
                    server_url,
                    username: username.clone(),
                    password,
                };
                if !cfg.is_valid() {
                    self.notify(gettext(
                        "Enter server URL, username, and password to connect Navidrome / Subsonic",
                    ));
                    return;
                }
                self.app_model
                    .api()
                    .set_provider(canto_api::subsonic_provider(cfg));
                self.dispatcher
                    .dispatch(LoginAction::SetLoginSuccess(username).into());
                self.notify(gettext("Connected to Navidrome / Subsonic"));
            }
            _ => {
                self.app_model
                    .api()
                    .set_provider(canto_api::spotify_provider(
                        Arc::clone(&self.token_provider),
                        Arc::clone(&self.session_token_provider),
                    ));
                self.dispatcher
                    .dispatch(LoginAction::TryLogin(TryLoginAction::Restore).into());
                self.notify(gettext("Switched to Spotify"));
            }
        }
    }

    pub fn set_local_folders(&self, folders: Vec<PathBuf>) {
        canto_config::settings::set_local_music_folders(&folders);
        if canto_config::settings::music_source() == "local" && !folders.is_empty() {
            self.switch_music_source("local");
        }
    }

    pub fn rescan_local_library(&self) {
        let api = self.app_model.api();
        let dispatcher = self.dispatcher.clone();
        glib::spawn_future_local(async move {
            match api.rescan_provider().await {
                Ok(()) => {
                    dispatcher
                        .dispatch(LoginAction::SetLoginSuccess(gettext("Local Library")).into());
                    dispatcher.dispatch(AppAction::ShowNotification(gettext(
                        "Local library rescanned",
                    )));
                }
                Err(e) => {
                    dispatcher.dispatch(AppAction::ShowNotification(format!("{e}")));
                }
            }
        });
    }

    pub fn stop_player(&self) {
        self.dispatcher.dispatch(PlaybackAction::Stop.into());
    }

    pub fn set_settings(&self) {
        self.dispatcher
            .dispatch(SettingsAction::ChangeSettings.into());
    }

    pub fn soloist_key_changed(&self) {
        self.dispatcher
            .dispatch(SettingsAction::SoloistKeyChanged.into());
    }

    pub fn show_restart_required(&self) {
        self.dispatcher.dispatch(AppAction::ShowRestartRequired);
    }

    pub fn notify(&self, message: String) {
        self.dispatcher
            .dispatch(AppAction::ShowNotification(message));
    }

    pub fn http_client(&self) -> canto_api::HttpClient {
        self.app_model.api().http_client().clone()
    }

    /// Clear the on-disk and in-memory data caches, plus librespot's
    /// downloaded audio cache, then notify the user.
    pub fn clear_cache(&self) {
        let api = self.app_model.api();
        let dispatcher = self.dispatcher.clone();
        tokio::spawn(async move {
            api.clear_user_cache().await;
            crate::player::clear_audio_cache();
            // Translators: Toast shown after the user clears the cache in settings.
            dispatcher.dispatch(AppAction::ShowNotification(gettext("Cache cleared")));
        });
    }

    pub fn settings(&self) -> CantoSettings {
        let state = self.app_model.get_state();
        state.settings.settings.clone()
    }

    /// Whether the logged-in Spotify account has locked its explicit content
    /// filter (e.g. via a family plan parental control). When locked, the user
    /// cannot disable explicit-track skipping in Canto.
    pub fn explicit_filter_locked(&self) -> bool {
        self.app_model.get_state().playback.explicit_filter_locked()
    }
}
