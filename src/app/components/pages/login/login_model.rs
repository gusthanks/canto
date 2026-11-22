use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use canto_api::{ProviderKind, SubsonicConfig, TokenProvider};
use gettextrs::gettext;

use crate::app::state::{LoginAction, TryLoginAction};
use crate::app::{AppModel, Dispatcher};

pub struct LoginModel {
    dispatcher: Dispatcher,
    app_model: Rc<AppModel>,
    token_provider: Arc<dyn TokenProvider>,
    session_token_provider: Arc<dyn TokenProvider>,
}

impl LoginModel {
    pub fn new(
        dispatcher: Dispatcher,
        app_model: Rc<AppModel>,
        token_provider: Arc<dyn TokenProvider>,
        session_token_provider: Arc<dyn TokenProvider>,
    ) -> Self {
        Self {
            dispatcher,
            app_model,
            token_provider,
            session_token_provider,
        }
    }

    pub fn try_autologin(&self) {
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::Restore).into());
    }

    pub fn login_with_spotify(&self) {
        canto_config::settings::set_music_source("spotify");
        if self.app_model.api().provider_kind() != ProviderKind::Spotify {
            self.app_model
                .api()
                .set_provider(canto_api::spotify_provider(
                    Arc::clone(&self.token_provider),
                    Arc::clone(&self.session_token_provider),
                ));
        }
        self.dispatcher
            .dispatch(LoginAction::TryLogin(TryLoginAction::InitLogin).into())
    }

    pub fn login_with_local_folders(&self, folders: Vec<PathBuf>) {
        canto_config::settings::set_local_music_folders(&folders);
        canto_config::settings::set_music_source("local");
        self.app_model
            .api()
            .set_provider(canto_api::local_provider(folders));
        self.dispatcher
            .dispatch(LoginAction::SetLoginSuccess(gettext("Local Library")).into());
    }

    pub fn login_with_subsonic(&self, config: SubsonicConfig) {
        canto_config::settings::set_subsonic_settings(
            &config.server_url,
            &config.username,
            &config.password,
        );
        canto_config::settings::set_music_source("subsonic");
        let label = config.username.clone();
        self.app_model
            .api()
            .set_provider(canto_api::subsonic_provider(config));
        self.dispatcher
            .dispatch(LoginAction::SetLoginSuccess(label).into());
    }
}
