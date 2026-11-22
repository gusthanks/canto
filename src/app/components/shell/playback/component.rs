use gio::prelude::SettingsExtManual;
use glib::object::ObjectExt;
use gtk::prelude::WidgetExt;
use std::ops::Deref;
use std::rc::Rc;

use crate::app::components::{
    DeviceSelector, DeviceSelectorModel, EventListener, SleepTimerController, SleepTimerMode,
};
use crate::app::models::*;
use crate::app::state::{
    Device as PlaybackDevice, LoginEvent, PlaybackAction, PlaybackEvent, SelectionEvent,
};
use crate::app::{AppEvent, AppModel, AppState, BrowserEvent, Dispatcher};
use canto_config::feature_flags::FeatureFlag;
use canto_config::settings::SETTINGS;

use super::playback_widget::PlaybackWidget;
use crate::app::components::now_panel::{
    artists_markup, current_track_liked, follow_track_link, title_markup, toggle_current_track_like,
};

pub struct PlaybackModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
}

impl PlaybackModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
        }
    }

    fn state(&self) -> impl Deref<Target = AppState> + '_ {
        self.app_model.get_state()
    }

    fn api_service(&self) -> std::sync::Arc<canto_api::ApiService> {
        self.app_model.api()
    }

    fn follow_link(&self, uri: &str) {
        follow_track_link(&self.app_model, &self.dispatcher, uri);
    }

    fn toggle_like(&self) {
        toggle_current_track_like(&self.app_model, &self.dispatcher);
    }

    fn liked(&self) -> Option<bool> {
        current_track_liked(&self.app_model)
    }

    fn is_playing(&self) -> bool {
        self.state().playback.is_playing()
    }

    fn is_shuffled(&self) -> bool {
        self.state().playback.is_shuffled()
    }

    fn current_song(&self) -> Option<Track> {
        self.app_model.get_state().playback.current_song()
    }

    fn play_next_song(&self) {
        self.dispatcher.dispatch(PlaybackAction::Next.into());
    }

    fn play_prev_song(&self) {
        self.dispatcher.dispatch(PlaybackAction::Previous.into());
    }

    fn toggle_playback(&self) {
        self.dispatcher.dispatch(PlaybackAction::TogglePlay.into());
    }

    fn toggle_shuffle(&self) {
        self.dispatcher
            .dispatch(PlaybackAction::ToggleShuffle.into());
    }

    fn toggle_repeat(&self) {
        self.dispatcher
            .dispatch(PlaybackAction::ToggleRepeat.into());
    }

    fn seek_to(&self, position: u32) {
        self.dispatcher
            .dispatch(PlaybackAction::Seek(position).into());
    }

    fn set_volume(&self, value: f64) {
        self.dispatcher
            .dispatch(PlaybackAction::SetVolume(value).into())
    }

    fn current_device(&self) -> PlaybackDevice {
        self.state().playback.current_device().clone()
    }
}

/// Whether the built-in equalizer affects the current playback route.
/// It applies only to local librespot playback, never to Soloist, the hidden
/// official Spotify app, or remote Spotify Connect devices.
pub(crate) fn equalizer_available(device: &PlaybackDevice, needs_official_client: bool) -> bool {
    matches!(device, PlaybackDevice::Local) && !needs_official_client
}

pub struct PlaybackControl {
    model: Rc<PlaybackModel>,
    widget: PlaybackWidget,
    device_selector: DeviceSelector,
    official_client_detected: bool,
}

impl PlaybackControl {
    pub fn new(
        model: PlaybackModel,
        widget: PlaybackWidget,
        sleep_timer: Rc<SleepTimerController>,
    ) -> Self {
        let model = Rc::new(model);

        widget.connect_play_pause(clone!(
            #[weak]
            model,
            move || model.toggle_playback()
        ));
        widget.connect_next(clone!(
            #[weak]
            model,
            move || model.play_next_song()
        ));
        widget.connect_prev(clone!(
            #[weak]
            model,
            move || model.play_prev_song()
        ));
        widget.connect_shuffle(clone!(
            #[weak]
            model,
            move || model.toggle_shuffle()
        ));
        widget.connect_repeat(clone!(
            #[weak]
            model,
            move || model.toggle_repeat()
        ));
        widget.connect_seek(clone!(
            #[weak]
            model,
            move |position| model.seek_to(position)
        ));
        widget.connect_cover_clicked(clone!(
            #[weak]
            widget,
            move || {
                let _ = widget.activate_action("win.show-now-playing", None);
            }
        ));
        widget.connect_link_activated(clone!(
            #[weak]
            model,
            move |uri| model.follow_link(uri)
        ));
        widget.connect_like_clicked(clone!(
            #[weak]
            model,
            move || model.toggle_like()
        ));
        widget.connect_volume_changed(clone!(
            #[weak]
            model,
            move |value| model.set_volume(value)
        ));

        let ds_widget = widget.device_selector();
        let ds_model = DeviceSelectorModel::new(model.app_model.clone(), model.dispatcher.clone());
        let device_selector = DeviceSelector::new(ds_widget.clone(), ds_model);
        gio::Settings::new(SETTINGS)
            .bind(FeatureFlag::DeviceSelector.key(), &ds_widget, "visible")
            .get()
            .build();

        let timer_for_clicks = Rc::clone(&sleep_timer);
        widget.connect_sleep_timer(move |mode| match mode {
            SleepTimerMode::Off => timer_for_clicks.set_off(),
            SleepTimerMode::EndOfTrack => timer_for_clicks.set_end_of_track(),
            SleepTimerMode::Minutes(m) => timer_for_clicks.set_minutes(m),
        });

        let weak_widget = widget.downgrade();
        sleep_timer.subscribe(move |snap| {
            if let Some(w) = weak_widget.upgrade() {
                w.set_sleep_timer_snapshot(snap);
            }
        });

        let control = Self {
            model,
            widget,
            device_selector,
            official_client_detected: crate::connect::account_needs_official_client(),
        };
        control.update_equalizer_availability();
        control
    }

    fn update_equalizer_availability(&self) {
        let needs_official =
            self.official_client_detected || crate::connect::account_needs_official_client();
        let available = equalizer_available(&self.model.current_device(), needs_official);
        self.widget.set_equalizer_available(available);
    }

    fn update_repeat(&self, mode: &RepeatMode) {
        self.widget.set_repeat_mode(*mode);
    }

    fn update_shuffled(&self) {
        self.widget.set_shuffled(self.model.is_shuffled());
    }

    fn update_playing(&self) {
        let is_playing = self.model.is_playing();
        self.widget.set_playing(is_playing);
    }

    fn update_current_info(&self) {
        if let Some(song) = self.model.current_song() {
            self.widget.set_track(
                &title_markup(&song.title, &song.rri.id, song.album.is_some()),
                &artists_markup(&song),
            );
            self.widget.set_explicit(song.is_explicit());
            self.widget.set_liked(self.model.liked());
            self.widget.set_song_duration(Some(song.duration_ms as f64));
            if let Some(url) = song.art.best_for_width(120) {
                self.widget
                    .set_artwork_from_url(url.to_owned(), self.model.api_service());
            }
        } else {
            self.widget.reset_info();
        }
    }

    fn sync_seek(&self, pos: u32) {
        self.widget.set_seek_position(pos as f64);
    }

    fn set_connection_lost(&self, lost: bool) {
        // Do NOT freeze the seekbar during an outage. A dead session does not
        // stop playback: buffered audio keeps streaming from the CDN, so the
        // local clock stays in sync with what the user actually hears. Freezing
        // it here left the seekbar behind, and since no Playing event fires when
        // playback was never interrupted, it never resynced once the session
        // came back. If playback really was interrupted, the reload after
        // reconnect emits a SyncSeek that corrects the position. We only make
        // sure the clock is running again on restore, in case playback was
        // paused and resumed during the outage. The "reconnecting" indicator
        // itself is shown as a persistent toast by the Notification component.
        if !lost && self.model.is_playing() {
            self.widget.resume_seek_position();
        }
    }
}

impl EventListener for PlaybackControl {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused)
            | AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed)
            | AppEvent::PlaybackEvent(PlaybackEvent::PlayStateSynced(_)) => {
                self.update_playing();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::RepeatModeChanged(mode))
            | AppEvent::PlaybackEvent(PlaybackEvent::RepeatSynced(mode)) => {
                self.update_repeat(mode);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::ShuffleChanged(_))
            | AppEvent::PlaybackEvent(PlaybackEvent::ShuffleSynced(_)) => {
                self.update_shuffled();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_))
            | AppEvent::PlaybackEvent(PlaybackEvent::TrackSynced) => {
                self.update_playing();
                self.update_current_info();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                self.update_playing();
                self.update_current_info();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::SeekSynced(pos))
            | AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(pos)) => {
                self.sync_seek(*pos);
            }
            AppEvent::SelectionEvent(SelectionEvent::SelectionModeChanged(active)) => {
                self.widget.set_seekbar_visible(!active);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::VolumeSet(value))
            | AppEvent::PlaybackEvent(PlaybackEvent::VolumeSynced(value)) => {
                self.widget.set_volume(*value)
            }
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => {
                self.widget.set_liked(self.model.liked());
            }
            AppEvent::ConnectionLostChanged(lost) => {
                self.set_connection_lost(*lost);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::SwitchedDevice(_))
            | AppEvent::LoginEvent(LoginEvent::LoginCompleted) => {
                self.update_equalizer_availability();
            }
            AppEvent::OfficialClientRequested => {
                self.official_client_detected = true;
                self.update_equalizer_availability();
            }
            AppEvent::LoginEvent(LoginEvent::LogoutCompleted) => {
                self.official_client_detected = crate::connect::account_needs_official_client();
                self.update_equalizer_availability();
            }
            _ => {}
        }
        self.device_selector.on_event(event);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn remote_device(label: &str) -> PlaybackDevice {
        PlaybackDevice::Connect(Device {
            id: "dev-1".to_string(),
            label: label.to_string(),
            kind: DeviceKind::Speaker,
            is_active: true,
            volume_percent: Some(50),
        })
    }

    #[test]
    fn equalizer_is_available_only_for_local_librespot_playback() {
        assert!(equalizer_available(&PlaybackDevice::Local, false));
        assert!(!equalizer_available(&PlaybackDevice::Local, true));
        assert!(!equalizer_available(&remote_device("Canto"), false));
        assert!(!equalizer_available(&remote_device("Canto"), true));
        assert!(!equalizer_available(&remote_device("Living Room"), false));
    }
}
