use super::LyricsPageWidget;
use crate::app::components::utils::Clock;
use crate::app::components::{Component, EventListener};
use crate::app::models::TrackExt;
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{AppEvent, AppModel, Dispatcher};
use gettextrs::gettext;
use gio::prelude::SettingsExt;
use glib::clone;
use gtk::prelude::*;
use riff_api::Lyrics;
use riff_config::settings::{
    APP_LANGUAGE, GOOGLE_TRANSLATE_API_KEY, LYRICS_OFFSET_MS, LYRICS_TRANSLATION,
    LYRICS_TRANSLATION_LANGUAGE, SETTINGS,
};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

pub struct LyricsPage {
    widget: LyricsPageWidget,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    lyrics: Rc<RefCell<Option<Lyrics>>>,
    current_track_id: Rc<RefCell<Option<String>>>,
    pending_fetch: Rc<Cell<bool>>,
    offset_ms: Rc<Cell<i32>>,
    settings: gio::Settings,
    clock: Rc<Clock>,
}

fn adjusted_lyric_position(pos_ms: u64, offset_ms: i32) -> u64 {
    let adjusted = pos_ms as i128 + i128::from(offset_ms);
    adjusted.clamp(0, i128::from(u64::MAX)) as u64
}

fn adjusted_seek_target(seek_ms: u32, offset_ms: i32) -> u32 {
    (i64::from(seek_ms) - i64::from(offset_ms)).clamp(0, i64::from(u32::MAX)) as u32
}

impl LyricsPage {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let widget = LyricsPageWidget::default();
        let settings = gio::Settings::new(SETTINGS);
        let offset_ms = Rc::new(Cell::new(settings.int(LYRICS_OFFSET_MS)));
        widget.set_offset_ms(offset_ms.get());
        // Often enough for the word-by-word highlight
        let clock = Rc::new(Clock::new(150));
        let page = Self {
            widget,
            app_model,
            dispatcher,
            lyrics: Rc::new(RefCell::new(None)),
            current_track_id: Rc::new(RefCell::new(None)),
            pending_fetch: Rc::new(Cell::new(false)),
            offset_ms,
            settings,
            clock,
        };
        page.setup_controls();
        page.setup_visibility_hooks();
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
        self.reload_for_current_track(false);
        if self.widget.is_mapped() && self.app_model.get_state().playback.is_playing() {
            self.start_clock();
        }
    }

    fn setup_controls(&self) {
        self.widget
            .set_translate_active(self.settings.boolean(LYRICS_TRANSLATION));
        let settings_translate = self.settings.clone();
        self.widget.connect_translate_toggled(move |active| {
            if settings_translate.boolean(LYRICS_TRANSLATION) != active {
                let _ = settings_translate.set_boolean(LYRICS_TRANSLATION, active);
            }
        });
        // Picking a language turns the translation on, in that language
        self.widget.connect_language_picked(clone!(
            #[strong(rename_to = settings)]
            self.settings,
            move |code| {
                let _ = settings.set_string(LYRICS_TRANSLATION_LANGUAGE, code);
                let _ = settings.set_boolean(LYRICS_TRANSLATION, true);
            }
        ));
        self.settings.connect_changed(
            Some(LYRICS_TRANSLATION_LANGUAGE),
            clone!(
                #[weak(rename_to = widget)]
                self.widget,
                #[weak(rename_to = app_model)]
                self.app_model,
                #[weak(rename_to = current_track_id)]
                self.current_track_id,
                move |settings, _| {
                    let track = current_track_id.borrow().clone();
                    if let Some(track_id) = track.filter(|_| settings.boolean(LYRICS_TRANSLATION)) {
                        widget.set_translations(None);
                        translate_lyrics(&widget, &app_model, &track_id);
                    }
                }
            ),
        );
        self.settings.connect_changed(
            Some(LYRICS_TRANSLATION),
            clone!(
                #[weak(rename_to = widget)]
                self.widget,
                #[weak(rename_to = app_model)]
                self.app_model,
                #[weak(rename_to = current_track_id)]
                self.current_track_id,
                move |settings, _| {
                    let active = settings.boolean(LYRICS_TRANSLATION);
                    widget.set_translate_active(active);
                    let track = current_track_id.borrow().clone();
                    match track {
                        Some(track_id) if active => translate_lyrics(&widget, &app_model, &track_id),
                        _ => widget.set_translations(None),
                    }
                }
            ),
        );

        let settings_minus = self.settings.clone();
        self.widget.connect_offset_minus(move || {
            let next = (settings_minus.int(LYRICS_OFFSET_MS) - 100).clamp(-10_000, 10_000);
            let _ = settings_minus.set_int(LYRICS_OFFSET_MS, next);
        });

        let settings_plus = self.settings.clone();
        self.widget.connect_offset_plus(move || {
            let next = (settings_plus.int(LYRICS_OFFSET_MS) + 100).clamp(-10_000, 10_000);
            let _ = settings_plus.set_int(LYRICS_OFFSET_MS, next);
        });

        let settings_reset = self.settings.clone();
        self.widget.connect_offset_reset(move || {
            let _ = settings_reset.set_int(LYRICS_OFFSET_MS, 0);
        });

        self.settings.connect_changed(
            Some(LYRICS_OFFSET_MS),
            clone!(
                #[weak(rename_to = widget)]
                self.widget,
                #[weak(rename_to = offset_ms)]
                self.offset_ms,
                #[weak(rename_to = app_model)]
                self.app_model,
                #[weak(rename_to = lyrics)]
                self.lyrics,
                move |settings, _| {
                    let val = settings.int(LYRICS_OFFSET_MS);
                    offset_ms.set(val);
                    widget.set_offset_ms(val);
                    if widget.is_mapped() {
                        let pos = app_model.get_state().playback.seek_position();
                        let adjusted = adjusted_lyric_position(pos, val);
                        if let Some(l) = lyrics.borrow().as_ref() {
                            widget.set_position(l.line_progress(adjusted));
                        }
                    }
                }
            ),
        );

        self.widget.connect_reload(clone!(
            #[weak(rename_to = widget)]
            self.widget,
            #[weak(rename_to = app_model)]
            self.app_model,
            #[strong(rename_to = dispatcher)]
            self.dispatcher,
            #[weak(rename_to = lyrics)]
            self.lyrics,
            #[weak(rename_to = current_track_id)]
            self.current_track_id,
            #[weak(rename_to = pending_fetch)]
            self.pending_fetch,
            #[weak(rename_to = offset_ms)]
            self.offset_ms,
            #[weak(rename_to = clock)]
            self.clock,
            move || {
                fetch_lyrics_for_current_track(
                    &widget,
                    &app_model,
                    &dispatcher,
                    &lyrics,
                    &current_track_id,
                    &pending_fetch,
                    &offset_ms,
                    &clock,
                    true,
                );
            }
        ));
    }

    fn setup_visibility_hooks(&self) {
        self.widget.connect_unmap(clone!(
            #[weak(rename_to = clock)]
            self.clock,
            move |_| {
                clock.stop();
            }
        ));

        self.widget.connect_map(clone!(
            #[weak(rename_to = widget)]
            self.widget,
            #[weak(rename_to = app_model)]
            self.app_model,
            #[strong(rename_to = dispatcher)]
            self.dispatcher,
            #[weak(rename_to = lyrics)]
            self.lyrics,
            #[weak(rename_to = current_track_id)]
            self.current_track_id,
            #[weak(rename_to = pending_fetch)]
            self.pending_fetch,
            #[weak(rename_to = offset_ms)]
            self.offset_ms,
            #[weak(rename_to = clock)]
            self.clock,
            move |_| {
                let maybe_track_id = app_model
                    .get_state()
                    .playback
                    .current_song()
                    .map(|t| t.rri.id.clone());
                let needs_fetch = pending_fetch.get()
                    || current_track_id.borrow().as_deref() != maybe_track_id.as_deref();
                if needs_fetch {
                    fetch_lyrics_for_current_track(
                        &widget,
                        &app_model,
                        &dispatcher,
                        &lyrics,
                        &current_track_id,
                        &pending_fetch,
                        &offset_ms,
                        &clock,
                        false,
                    );
                    return;
                }

                let (is_playing, pos) = {
                    let playback = &app_model.get_state().playback;
                    (playback.is_playing(), playback.seek_position())
                };
                let adjusted = adjusted_lyric_position(pos, offset_ms.get());
                let is_synced = if let Some(l) = lyrics.borrow().as_ref() {
                    widget.set_position(l.line_progress(adjusted));
                    l.is_synced()
                } else {
                    false
                };
                if is_playing && is_synced {
                    start_clock_loop(&clock, &widget, &app_model, &lyrics, &offset_ms);
                } else {
                    clock.stop();
                }
            }
        ));
    }

    fn start_clock(&self) {
        if !self.widget.is_mapped() {
            self.clock.stop();
            return;
        }
        let is_synced = self.lyrics.borrow().as_ref().is_some_and(Lyrics::is_synced);
        if is_synced {
            start_clock_loop(
                &self.clock,
                &self.widget,
                &self.app_model,
                &self.lyrics,
                &self.offset_ms,
            );
        } else {
            self.clock.stop();
        }
    }

    fn stop_clock(&self) {
        self.clock.stop();
    }

    fn sync_position(&self, pos: u64) {
        if !self.widget.is_mapped() {
            return;
        }
        let adjusted = adjusted_lyric_position(pos, self.offset_ms.get());
        if let Some(l) = self.lyrics.borrow().as_ref() {
            self.widget.set_position(l.line_progress(adjusted));
        }
    }

    pub fn reload_for_current_track(&self, force_refresh: bool) {
        fetch_lyrics_for_current_track(
            &self.widget,
            &self.app_model,
            &self.dispatcher,
            &self.lyrics,
            &self.current_track_id,
            &self.pending_fetch,
            &self.offset_ms,
            &self.clock,
            force_refresh,
        );
    }
}

fn fetch_lyrics_for_current_track(
    widget: &LyricsPageWidget,
    app_model: &Rc<AppModel>,
    dispatcher: &Dispatcher,
    lyrics_cell: &Rc<RefCell<Option<Lyrics>>>,
    current_track_id: &Rc<RefCell<Option<String>>>,
    pending_fetch: &Rc<Cell<bool>>,
    offset_ms: &Rc<Cell<i32>>,
    clock: &Rc<Clock>,
    force_refresh: bool,
) {
    let maybe_track = app_model.get_state().playback.current_song();
    let Some(track) = maybe_track else {
        clock.stop();
        pending_fetch.set(false);
        *current_track_id.borrow_mut() = None;
        *lyrics_cell.borrow_mut() = None;
        widget.set_empty(&gettext("No Track Playing"), None, false);
        return;
    };

    let track_id = track.rri.id.clone();
    if !force_refresh
        && !pending_fetch.get()
        && current_track_id.borrow().as_deref() == Some(track_id.as_str())
    {
        if widget.is_mapped() {
            let pos = app_model.get_state().playback.seek_position();
            let adjusted = adjusted_lyric_position(pos, offset_ms.get());
            if let Some(l) = lyrics_cell.borrow().as_ref() {
                widget.set_position(l.line_progress(adjusted));
            }
        }
        return;
    }

    if !widget.is_mapped() {
        clock.stop();
        *current_track_id.borrow_mut() = Some(track_id);
        *lyrics_cell.borrow_mut() = None;
        pending_fetch.set(true);
        return;
    }

    pending_fetch.set(false);
    *current_track_id.borrow_mut() = Some(track_id.clone());
    *lyrics_cell.borrow_mut() = None;

    let key = track
        .rri
        .uri
        .clone()
        .unwrap_or_else(|| track.rri.id.clone());
    let api = app_model.api();

    // 1. Check disk cache synchronously unless reloading manually
    if !force_refresh {
        if let Some(cached) = api.lyrics_service().cache().get(&key) {
            apply_lyrics_to_widget(
                widget,
                app_model,
                dispatcher,
                lyrics_cell,
                current_track_id,
                offset_ms,
                clock,
                &track_id,
                Some(cached),
            );
            return;
        }
    }

    // 2. Fetch asynchronously from LRCLIB
    clock.stop();
    widget.set_loading();

    let dispatcher = dispatcher.clone();
    let title = track.title.clone();
    let artist = track.artists_name();
    let album = track.album.as_ref().map(|a| a.name.clone());
    let duration_secs = track.duration_ms / 1000;
    let weak_widget = widget.downgrade();
    let weak_lyrics = Rc::downgrade(lyrics_cell);
    let weak_track_id = Rc::downgrade(current_track_id);
    let weak_offset = Rc::downgrade(offset_ms);
    let weak_model = Rc::downgrade(app_model);
    let weak_clock = Rc::downgrade(clock);

    glib::spawn_future_local(async move {
        let fetched = if force_refresh {
            api.refetch_lyrics(&key, &title, &artist, album.as_deref(), Some(duration_secs))
                .await
        } else {
            api.fetch_lyrics(&key, &title, &artist, album.as_deref(), Some(duration_secs))
                .await
        };

        let (Some(w), Some(lc), Some(ct), Some(off), Some(am), Some(cl)) = (
            weak_widget.upgrade(),
            weak_lyrics.upgrade(),
            weak_track_id.upgrade(),
            weak_offset.upgrade(),
            weak_model.upgrade(),
            weak_clock.upgrade(),
        ) else {
            return;
        };

        apply_lyrics_to_widget(
            &w,
            &am,
            &dispatcher,
            &lc,
            &ct,
            &off,
            &cl,
            &track_id,
            fetched,
        );
    });
}

#[allow(clippy::too_many_arguments)]
fn apply_lyrics_to_widget(
    widget: &LyricsPageWidget,
    app_model: &Rc<AppModel>,
    dispatcher: &Dispatcher,
    lyrics_cell: &Rc<RefCell<Option<Lyrics>>>,
    current_track_id: &Rc<RefCell<Option<String>>>,
    offset_ms: &Rc<Cell<i32>>,
    clock: &Rc<Clock>,
    track_id: &str,
    lyrics_opt: Option<Lyrics>,
) {
    if current_track_id.borrow().as_deref() != Some(track_id) {
        return;
    }

    *lyrics_cell.borrow_mut() = lyrics_opt.clone();

    if let Some(l) = lyrics_opt {
        let disp = dispatcher.clone();
        let off_seek = Rc::clone(offset_ms);
        let is_synced = l.is_synced();
        widget.set_lyrics(&l, move |seek_ms| {
            let target = adjusted_seek_target(seek_ms, off_seek.get());
            disp.dispatch(PlaybackAction::Seek(target).into());
        });
        let pos = app_model.get_state().playback.seek_position();
        let adjusted = adjusted_lyric_position(pos, offset_ms.get());
        widget.set_position(l.line_progress(adjusted));
        if gio::Settings::new(SETTINGS).boolean(LYRICS_TRANSLATION) {
            translate_lyrics(widget, app_model, track_id);
        }
        annotate_lyrics(widget, app_model, track_id);

        if is_synced && widget.is_mapped() && app_model.get_state().playback.is_playing() {
            start_clock_loop(clock, widget, app_model, lyrics_cell, offset_ms);
        } else {
            clock.stop();
        }
    } else {
        clock.stop();
        widget.set_empty(
            &gettext("No Lyrics Available"),
            Some(&gettext("Lyrics couldn't be found for this song.")),
            true,
        );
    }
}

/// Marks the lines Genius explains (with a Genius token).
fn annotate_lyrics(widget: &LyricsPageWidget, app_model: &Rc<AppModel>, track_id: &str) {
    let token = gio::Settings::new(SETTINGS)
        .string(riff_config::settings::GENIUS_ACCESS_TOKEN)
        .trim()
        .to_string();
    let Some(track) = app_model.get_state().playback.current_song() else {
        return;
    };
    if token.is_empty() || track.rri.id != track_id {
        return;
    }
    let lines = widget.line_texts();
    let api = app_model.api();
    let weak_widget = widget.downgrade();
    let track_id = track_id.to_string();
    let artist = track.artists_name();
    let title = track.title.clone();
    glib::spawn_future_local(async move {
        let result = api
            .fetch_genius_line_annotations(&track_id, &artist, &title, &token, &lines)
            .await;
        let Some(widget) = weak_widget.upgrade() else {
            return;
        };
        if widget.line_texts() != lines {
            return;
        }
        match result {
            Ok(notes) => widget.set_annotations(Some(&notes)),
            Err(e) => warn!("Genius annotations failed: {e}"),
        }
    });
}

/// The language lyrics are translated into: the one picked in the lyrics
/// bar, or else the interface's.
fn translation_target() -> String {
    let settings = gio::Settings::new(SETTINGS);
    let picked = settings.string(LYRICS_TRANSLATION_LANGUAGE).to_string();
    if !picked.is_empty() && picked != "auto" {
        return picked;
    }
    let chosen = settings.string(APP_LANGUAGE).to_string();
    let locale = if chosen.is_empty() || chosen == "system" {
        glib::language_names()
            .first()
            .map(|l| l.to_string())
            .unwrap_or_default()
    } else {
        chosen
    };
    riff_api::lyrics::target_for_locale(&locale)
}

/// Fetches (or reads from the cache) a translation of the shown lyrics and
/// puts it under each line, if the same track is still shown.
fn translate_lyrics(widget: &LyricsPageWidget, app_model: &Rc<AppModel>, track_id: &str) {
    let lines = widget.line_texts();
    if lines.iter().all(|l| l.trim().is_empty()) {
        return;
    }
    let target = translation_target();
    let api_key = gio::Settings::new(SETTINGS)
        .string(GOOGLE_TRANSLATE_API_KEY)
        .to_string();
    let api = app_model.api();
    let weak_widget = widget.downgrade();
    let track_id = track_id.to_string();
    glib::spawn_future_local(async move {
        let result = api
            .lyrics_service()
            .translator()
            .translate(&track_id, &lines, &target, Some(api_key.as_str()))
            .await;
        let Some(widget) = weak_widget.upgrade() else {
            return;
        };
        // Lyrics changed (another track) or translation turned off meanwhile
        if widget.line_texts() != lines
            || !gio::Settings::new(SETTINGS).boolean(LYRICS_TRANSLATION)
        {
            return;
        }
        match result {
            Ok(t) if !t.same_language => widget.set_translations(Some(&t.lines)),
            Ok(_) => widget.set_translations(None),
            Err(e) => warn!("lyrics translation failed: {e}"),
        }
    });
}

fn start_clock_loop(
    clock: &Rc<Clock>,
    widget: &LyricsPageWidget,
    app_model: &Rc<AppModel>,
    lyrics: &Rc<RefCell<Option<Lyrics>>>,
    offset_ms: &Rc<Cell<i32>>,
) {
    let weak_model = Rc::downgrade(app_model);
    let weak_widget = widget.downgrade();
    let weak_lyrics = Rc::downgrade(lyrics);
    let weak_offset = Rc::downgrade(offset_ms);

    clock.start(move || {
        let (Some(model), Some(w), Some(l_cell), Some(off)) = (
            weak_model.upgrade(),
            weak_widget.upgrade(),
            weak_lyrics.upgrade(),
            weak_offset.upgrade(),
        ) else {
            return;
        };
        if !w.is_mapped() {
            return;
        }
        let (is_playing, pos) = {
            let playback = &model.get_state().playback;
            (playback.is_playing(), playback.seek_position())
        };
        if !is_playing {
            return;
        }
        let adjusted = adjusted_lyric_position(pos, off.get());
        if let Some(l) = l_cell.borrow().as_ref() {
            w.set_position(l.line_progress(adjusted));
        };
    });
}

impl Component for LyricsPage {
    fn get_root_widget(&self) -> &gtk::Widget {
        self.widget.upcast_ref()
    }
}

impl Drop for LyricsPage {
    fn drop(&mut self) {
        self.clock.stop();
    }
}

impl EventListener for LyricsPage {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_) | PlaybackEvent::TrackSynced,
            ) => {
                self.reload_for_current_track(false);
                if self.widget.is_mapped() && self.app_model.get_state().playback.is_playing() {
                    self.start_clock();
                } else {
                    self.stop_clock();
                }
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed | PlaybackEvent::PlayStateSynced(true),
            ) => {
                if self.widget.is_mapped() {
                    self.start_clock();
                    let pos = self.app_model.get_state().playback.seek_position();
                    self.sync_position(pos);
                }
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackPaused | PlaybackEvent::PlayStateSynced(false),
            ) => {
                self.stop_clock();
            }
            AppEvent::PlaybackEvent(PlaybackEvent::PlaybackStopped) => {
                self.stop_clock();
                self.pending_fetch.set(false);
                *self.current_track_id.borrow_mut() = None;
                *self.lyrics.borrow_mut() = None;
                self.widget
                    .set_empty(&gettext("No Track Playing"), None, false);
            }
            AppEvent::PlaybackEvent(PlaybackEvent::TrackSeeked(pos))
            | AppEvent::PlaybackEvent(PlaybackEvent::SeekSynced(pos)) => {
                self.sync_position(u64::from(*pos));
            }
            _ => {}
        }
    }
}
