use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::LyricsPageWidget;
use crate::app::components::utils::Clock;
use crate::app::components::{Component, EventListener};
use crate::app::models::TrackExt;
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{AppEvent, AppModel, Dispatcher};
use riff_api::Lyrics;

pub struct LyricsPage {
    widget: LyricsPageWidget,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    lyrics: Rc<RefCell<Option<Lyrics>>>,
    current_track_id: Rc<RefCell<Option<String>>>,
    clock: Clock,
}

impl LyricsPage {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let widget = LyricsPageWidget::default();
        let clock = Clock::new(250);
        let page = Self {
            widget,
            app_model,
            dispatcher,
            lyrics: Rc::new(RefCell::new(None)),
            current_track_id: Rc::new(RefCell::new(None)),
            clock,
        };
        page.init();
        page
    }

    pub fn widget(&self) -> &LyricsPageWidget {
        &self.widget
    }

    #[allow(dead_code)]
    pub fn title() -> String {
        gettext("Lyrics")
    }

    fn init(&self) {
        self.reload_for_current_track();
        if self.app_model.get_state().playback.is_playing() {
            self.start_clock();
        }
    }

    fn start_clock(&self) {
        let app_model = self.app_model.clone();
        let widget = self.widget.clone();
        let lyrics = self.lyrics.clone();

        self.clock.start(move || {
            let (is_playing, pos) = {
                let playback = &app_model.get_state().playback;
                (playback.is_playing(), playback.seek_position())
            };
            if !is_playing {
                return;
            }
            if let Some(l) = lyrics.borrow().as_ref() {
                let active = l.active_line_index(pos);
                widget.set_active_line(active);
            }
        });
    }

    fn stop_clock(&self) {
        self.clock.stop();
    }

    fn sync_position(&self, pos: u64) {
        if let Some(l) = self.lyrics.borrow().as_ref() {
            let active = l.active_line_index(pos);
            self.widget.set_active_line(active);
        }
    }

    pub fn reload_for_current_track(&self) {
        let maybe_track = self.app_model.get_state().playback.current_song();
        let Some(track) = maybe_track else {
            *self.current_track_id.borrow_mut() = None;
            *self.lyrics.borrow_mut() = None;
            self.widget.set_empty(&gettext("No Track Playing"), None);
            return;
        };

        let track_id = track.rri.id.clone();
        if *self.current_track_id.borrow() == Some(track_id.clone()) {
            return;
        }

        *self.current_track_id.borrow_mut() = Some(track_id.clone());
        *self.lyrics.borrow_mut() = None;

        let key = track.rri.uri.clone().unwrap_or_else(|| track.rri.id.clone());
        let api = self.app_model.api();

        // 1. Check disk cache synchronously for 0ms reload
        if let Some(cached) = api.lyrics_service().cache().get(&key) {
            self.apply_lyrics(&track_id, Some(cached));
            return;
        }

        // 2. Fetch asynchronously from LRCLIB
        self.widget.set_loading();

        let dispatcher = self.dispatcher.clone();
        let title = track.title.clone();
        let artist = track.artists_name();
        let album = track.album.as_ref().map(|a| a.name.clone());
        let duration_secs = track.duration_ms / 1000;
        let widget = self.widget.clone();
        let lyrics_cell = self.lyrics.clone();
        let current_track_id = self.current_track_id.clone();
        let app_model = self.app_model.clone();

        glib::spawn_future_local(async move {
            let fetched = api
                .fetch_lyrics(
                    &key,
                    &title,
                    &artist,
                    album.as_deref(),
                    Some(duration_secs),
                )
                .await;

            if *current_track_id.borrow() != Some(track_id.clone()) {
                return;
            }

            *lyrics_cell.borrow_mut() = fetched.clone();

            if let Some(l) = fetched {
                let disp = dispatcher.clone();
                widget.set_lyrics(&l, move |seek_ms| {
                    disp.dispatch(PlaybackAction::Seek(seek_ms).into());
                });
                let pos = app_model.get_state().playback.seek_position();
                widget.set_active_line(l.active_line_index(pos));
            } else {
                widget.set_empty(
                    &gettext("No Lyrics Available"),
                    Some(&gettext("Lyrics couldn't be found for this song.")),
                );
            }
        });
    }

    fn apply_lyrics(&self, track_id: &str, lyrics_opt: Option<Lyrics>) {
        if *self.current_track_id.borrow() != Some(track_id.to_string()) {
            return;
        }

        *self.lyrics.borrow_mut() = lyrics_opt.clone();

        if let Some(l) = lyrics_opt {
            let disp = self.dispatcher.clone();
            self.widget.set_lyrics(&l, move |seek_ms| {
                disp.dispatch(PlaybackAction::Seek(seek_ms).into());
            });
            let pos = self.app_model.get_state().playback.seek_position();
            self.widget.set_active_line(l.active_line_index(pos));
        } else {
            self.widget.set_empty(
                &gettext("No Lyrics Available"),
                Some(&gettext("Lyrics couldn't be found for this song.")),
            );
        }
    }
}

impl Component for LyricsPage {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl EventListener for LyricsPage {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(PlaybackEvent::TrackChanged(_)) => {
                self.reload_for_current_track();
                if self.app_model.get_state().playback.is_playing() {
                    self.start_clock();
                }
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackResumed) => {
                self.start_clock();
                let pos = self.app_model.get_state().playback.seek_position();
                self.sync_position(pos);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackPaused) => {
                self.stop_clock();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                self.stop_clock();
                *self.current_track_id.borrow_mut() = None;
                *self.lyrics.borrow_mut() = None;
                self.widget.set_empty(&gettext("No Track Playing"), None);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(pos))
            | AppEvent::PlaybackEvent(PlaybackEvent::SeekSynced(pos)) => {
                self.sync_position(*pos as u64);
            }
            _ => {}
        }
    }
}
