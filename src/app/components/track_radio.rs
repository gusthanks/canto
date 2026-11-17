//! Track Radio action and Smart Queue Continuation listener.

use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

use gettextrs::gettext;
use canto_api::DEFAULT_RADIO_TARGET_COUNT;
use canto_config::settings::{SETTINGS, SMART_QUEUE_CONTINUATION};

use crate::app::components::sleep_timer::SleepTimerController;
use crate::app::components::EventListener;
use crate::app::load;
use crate::app::models::Track;
use crate::app::state::{load_context, start_actions, Device, PlaybackAction, PlaybackEvent};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher, SongsSource};
use gio::prelude::*;

pub fn start_track_radio(app_model: &Rc<AppModel>, dispatcher: &Dispatcher, seed: Track) {
    let api = app_model.api();
    let exclude_ids: HashSet<String> = {
        let state = app_model.get_state();
        state.playback.upcoming_ids().into_iter().collect()
    };

    let dispatcher = dispatcher.clone();
    let app_model_clone = Rc::clone(app_model);

    glib::MainContext::default().spawn_local(async move {
        match api
            .fetch_track_radio(&seed, &exclude_ids, DEFAULT_RADIO_TARGET_COUNT, load::visible())
            .await
        {
            Ok(tracks) if tracks.is_empty() => {
                dispatcher.dispatch(AppAction::ShowNotification(gettext(
                    "Could not find similar tracks for this song",
                )));
            }
            Ok(tracks) => {
                let is_connect = matches!(
                    app_model_clone.get_state().playback.current_device(),
                    Device::Connect(_)
                );
                if is_connect {
                    // Connect devices require loading an explicit track list context.
                    let mut all = Vec::with_capacity(tracks.len() + 1);
                    all.push(seed.clone());
                    all.extend(tracks);
                    let label = format!("{}: {}", gettext("Radio"), seed.title);
                    let source = SongsSource::TrackRadio(seed.rri.id.clone());
                    let load = PlaybackAction::LoadContextSongs(source.clone(), all);
                    for action in start_actions(false, load_context(source, Some(label), load)) {
                        dispatcher.dispatch(action);
                    }
                    dispatcher.dispatch(AppAction::ShowNotification(gettext("Started Track Radio")));
                    return;
                }

                let current_id = app_model_clone.get_state().playback.current_song_id();
                let is_playing = app_model_clone.get_state().playback.is_playing();
                if current_id.as_deref() == Some(seed.rri.id.as_str()) && is_playing {
                    // Keep the active track playing seamlessly and populate upcoming radio tracks.
                    dispatcher.dispatch(PlaybackAction::ClearQueued.into());
                    dispatcher.dispatch(AppAction::QueueTracks { tracks });
                } else {
                    let mut all = Vec::with_capacity(tracks.len() + 1);
                    all.push(seed.clone());
                    all.extend(tracks);
                    let label = format!("{}: {}", gettext("Radio"), seed.title);
                    let source = SongsSource::TrackRadio(seed.rri.id.clone());
                    let load = PlaybackAction::LoadContextSongs(source.clone(), all);
                    for action in start_actions(false, load_context(source, Some(label), load)) {
                        dispatcher.dispatch(action);
                    }
                }
                dispatcher.dispatch(AppAction::ShowNotification(gettext("Started Track Radio")));
            }
            Err(e) => {
                warn!("track_radio: failed to build radio for {}: {e}", seed.rri.id);
                dispatcher.dispatch(AppAction::ShowNotification(gettext(
                    "Could not start Track Radio right now",
                )));
            }
        }
    });
}

pub struct SmartQueueController {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    settings: gio::Settings,
    sleep_timer: Rc<SleepTimerController>,
    generation: Rc<Cell<u64>>,
}

impl SmartQueueController {
    pub fn new(
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
        sleep_timer: Rc<SleepTimerController>,
    ) -> Self {
        Self {
            app_model,
            dispatcher,
            settings: gio::Settings::new(SETTINGS),
            sleep_timer,
            generation: Rc::new(Cell::new(0)),
        }
    }

    fn trigger_continuation(&self, seed: Track) {
        let gen_num = self.generation.get().wrapping_add(1);
        self.generation.set(gen_num);

        let api = self.app_model.api();
        let app_model = Rc::clone(&self.app_model);
        let dispatcher = self.dispatcher.clone();
        let generation = Rc::clone(&self.generation);

        glib::MainContext::default().spawn_local(async move {
            let Ok(tracks) = api
                .fetch_track_radio(&seed, &HashSet::new(), DEFAULT_RADIO_TARGET_COUNT, load::visible())
                .await
            else {
                return;
            };
            if tracks.is_empty() || generation.get() != gen_num {
                return;
            }
            // Do not hijack playback if user manually started something while fetching.
            let state = app_model.get_state();
            if state.playback.is_playing()
                || state.playback.current_song_id().is_some()
                || !matches!(state.playback.current_device(), Device::Local)
            {
                return;
            }
            drop(state);

            let label = format!("{}: {}", gettext("Radio"), seed.title);
            let source = SongsSource::TrackRadio(seed.rri.id.clone());
            let load = PlaybackAction::LoadContextSongs(source.clone(), tracks);
            for action in start_actions(false, load_context(source, Some(label), load)) {
                dispatcher.dispatch(action);
            }
            dispatcher.dispatch(AppAction::ShowNotification(gettext(
                "Playing similar tracks",
            )));
        });
    }
}

impl EventListener for SmartQueueController {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) => {
                // Cancel any pending auto-continuation fetch if another track started.
                let _ = self.sleep_timer.take_continuation_suppression();
                self.generation.set(self.generation.get().wrapping_add(1));
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                if self.sleep_timer.take_continuation_suppression() {
                    return;
                }
                if !self.settings.boolean(SMART_QUEUE_CONTINUATION) {
                    return;
                }
                let seed = {
                    let state = self.app_model.get_state();
                    if !matches!(state.playback.current_device(), Device::Local)
                        || state.playback.repeat_mode() != canto_api::RepeatMode::Off
                    {
                        None
                    } else {
                        state.playback.last_finished_track().cloned()
                    }
                };
                if let Some(seed) = seed {
                    self.trigger_continuation(seed);
                }
            }
            _ => {}
        }
    }
}
