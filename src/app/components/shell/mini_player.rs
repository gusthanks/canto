// Mini player: a small separate window with the cover, the track, the main
// controls, a thin 2px seekable progress bar along the bottom edge, wheel /
// touchpad volume scroll, and an optional discreet 1-line live synced lyric
// strip below the controls.

use canto_api::models::is_resource_url;
use canto_api::Lyrics;
use canto_config::settings::{LYRICS_OFFSET_MS, MINI_PLAYER_LYRICS, SETTINGS};
use gettextrs::gettext;
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use super::now_panel::{current_track_liked, show_like_state, toggle_current_track_like};
use super::playback::{scroll_steps_from_event, volume_after_scroll};
use crate::app::components::utils::Debouncer;
use crate::app::components::visual_bridge::active_synced_lyric;
use crate::app::components::EventListener;
use crate::app::models::TrackExt;
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{load, AppEvent, AppModel, BrowserEvent, Dispatcher};

pub const SHOW_MINI_PLAYER_ACTION: &str = "mini-player";

/// Side of the cover, in pixels.
const COVER_PX: i32 = 64;

/// Smooth live refresh cadence while the mini player is visible and playing.
const TICK_INTERVAL_MS: u64 = 100;

/// Debounce window for wheel/touchpad volume bursts over the mini player.
const VOLUME_DEBOUNCE_MS: u32 = 80;

/// Computes the normalized progress fraction in `0.0..=1.0` for `progress_ms`
/// within `duration_ms`.
pub(crate) fn progress_fraction(progress_ms: u64, duration_ms: u32) -> f64 {
    if duration_ms == 0 {
        return 0.0;
    }
    (progress_ms as f64 / f64::from(duration_ms)).clamp(0.0, 1.0)
}

/// Maps a click at horizontal offset `x` along a bar of `width` pixels to a
/// seek target in milliseconds, or `None` when the bar has no width or track
/// duration is zero.
pub(crate) fn seek_target_ms(x: f64, width: f64, duration_ms: u32) -> Option<u32> {
    if width <= 0.0 || duration_ms == 0 {
        return None;
    }
    let fraction = (x / width).clamp(0.0, 1.0);
    Some((fraction * f64::from(duration_ms)).round() as u32)
}

struct View {
    window: gtk::Window,
    cover: gtk::Image,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    lyrics_toggle: gtk::ToggleButton,
    like: gtk::Button,
    lyric_label: gtk::Label,
    progress_hit: gtk::Box,
    progress: gtk::ProgressBar,
    /// The cover and the titles: clicking them opens the full app.
    cover_area: gtk::Box,
    text_area: gtk::Box,
}

struct Runtime {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    settings: gio::Settings,
    view: RefCell<Option<View>>,
    timer: RefCell<Option<glib::SourceId>>,
    volume_debouncer: Debouncer,
    local_volume: Cell<f64>,
    lyrics_enabled: Cell<bool>,
    lyrics_offset_ms: Cell<i32>,
    lyrics_track_id: RefCell<Option<String>>,
    lyrics: RefCell<Option<Lyrics>>,
}

pub struct MiniPlayer {
    runtime: Rc<Runtime>,
}

impl MiniPlayer {
    pub fn new(
        main_window: &libadwaita::ApplicationWindow,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let runtime = Runtime::new(app_model, dispatcher);
        let action = gio::SimpleAction::new(SHOW_MINI_PLAYER_ACTION, None);
        let action_runtime = Rc::clone(&runtime);
        let application = main_window.application();
        let main = main_window.downgrade();
        action.connect_activate(move |_, _| {
            action_runtime.present(application.as_ref(), Some(main.clone()));
            // The mini player stands in for the app: the big window goes away
            // until the mini player is clicked or closed
            if let Some(main) = main.upgrade() {
                main.set_visible(false);
            }
        });
        main_window.add_action(&action);
        Self { runtime }
    }
}

impl Drop for MiniPlayer {
    fn drop(&mut self) {
        self.runtime.stop_timer();
        self.runtime.volume_debouncer.stop();
    }
}

impl Runtime {
    fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Rc<Self> {
        let settings = gio::Settings::new(SETTINGS);
        let initial_volume = app_model
            .get_state()
            .playback
            .volume()
            .unwrap_or_else(|| settings.double("volume"))
            .clamp(0.0, 1.0);
        let lyrics_enabled = settings.boolean(MINI_PLAYER_LYRICS);
        let lyrics_offset_ms = settings.int(LYRICS_OFFSET_MS);

        let runtime = Rc::new(Self {
            app_model,
            dispatcher,
            settings: settings.clone(),
            view: RefCell::new(None),
            timer: RefCell::new(None),
            volume_debouncer: Debouncer::new(),
            local_volume: Cell::new(initial_volume),
            lyrics_enabled: Cell::new(lyrics_enabled),
            lyrics_offset_ms: Cell::new(lyrics_offset_ms),
            lyrics_track_id: RefCell::new(None),
            lyrics: RefCell::new(None),
        });

        let weak = Rc::downgrade(&runtime);
        settings.connect_changed(Some(MINI_PLAYER_LYRICS), move |s, key| {
            if let Some(rt) = weak.upgrade() {
                let on = s.boolean(key);
                rt.lyrics_enabled.set(on);
                if let Some(view) = rt.view.borrow().as_ref() {
                    if view.lyrics_toggle.is_active() != on {
                        view.lyrics_toggle.set_active(on);
                    }
                    update_lyrics_toggle_tooltip(&view.lyrics_toggle, on);
                }
                if on {
                    rt.ensure_lyrics_loaded();
                } else {
                    *rt.lyrics_track_id.borrow_mut() = None;
                    *rt.lyrics.borrow_mut() = None;
                }
                rt.with_shown_view(|view| rt.update_progress_and_lyric(view));
            }
        });

        let weak_offset = Rc::downgrade(&runtime);
        settings.connect_changed(Some(LYRICS_OFFSET_MS), move |s, key| {
            if let Some(rt) = weak_offset.upgrade() {
                rt.lyrics_offset_ms.set(s.int(key));
                rt.with_shown_view(|view| rt.update_progress_and_lyric(view));
            }
        });

        runtime
    }

    fn present(
        self: &Rc<Self>,
        application: Option<&gtk::Application>,
        main: Option<glib::WeakRef<libadwaita::ApplicationWindow>>,
    ) {
        {
            let mut slot = self.view.borrow_mut();
            if slot.is_none() {
                let view = build_view(self);
                if let Some(app) = application {
                    view.window.set_application(Some(app));
                }
                if let Some(main) = main {
                    link_to_main_window(&view, main);
                }
                *slot = Some(view);
            }
        }
        self.sync_local_volume();
        if let Some(view) = self.view.borrow().as_ref() {
            update_view(self, view);
            view.window.present();
        }
        self.ensure_lyrics_loaded();
        self.sync_timer();
        if let Some(view) = self.view.borrow().as_ref() {
            self.update_progress_and_lyric(view);
        }
    }

    fn is_shown(&self) -> bool {
        self.view
            .borrow()
            .as_ref()
            .is_some_and(|view| view.window.is_visible())
    }

    fn with_shown_view(&self, f: impl FnOnce(&View)) {
        if let Some(view) = self.view.borrow().as_ref() {
            if view.window.is_visible() {
                f(view);
            }
        }
    }

    fn sync_local_volume(&self) {
        let vol = self
            .app_model
            .get_state()
            .playback
            .volume()
            .unwrap_or_else(|| self.settings.double("volume"))
            .clamp(0.0, 1.0);
        self.local_volume.set(vol);
    }

    fn stop_timer(&self) {
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
    }

    fn sync_timer(self: &Rc<Self>) {
        let active = self.is_shown() && self.app_model.get_state().playback.is_playing();
        if !active {
            self.stop_timer();
            return;
        }
        if self.timer.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_local(Duration::from_millis(TICK_INTERVAL_MS), move || {
            let Some(rt) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !rt.is_shown() || !rt.app_model.get_state().playback.is_playing() {
                *rt.timer.borrow_mut() = None;
                return glib::ControlFlow::Break;
            }
            rt.with_shown_view(|view| rt.update_progress_and_lyric(view));
            glib::ControlFlow::Continue
        });
        *self.timer.borrow_mut() = Some(source);
    }

    fn ensure_lyrics_loaded(self: &Rc<Self>) {
        if !self.is_shown() || !self.lyrics_enabled.get() {
            return;
        }
        let Some(track) = self.app_model.get_state().playback.current_song() else {
            *self.lyrics_track_id.borrow_mut() = None;
            *self.lyrics.borrow_mut() = None;
            return;
        };

        let track_id = track.rri.id.clone();
        if self.lyrics_track_id.borrow().as_deref() == Some(track_id.as_str()) {
            return;
        }
        *self.lyrics_track_id.borrow_mut() = Some(track_id.clone());
        *self.lyrics.borrow_mut() = None;

        let key = track
            .rri
            .uri
            .clone()
            .unwrap_or_else(|| track.rri.id.clone());
        let api = self.app_model.api();

        // Check disk cache synchronously first (no Genius fallback: Genius is unsynced).
        if let Some(cached) = api.cached_lyrics(&key, false) {
            if cached.lyrics.is_synced() {
                *self.lyrics.borrow_mut() = Some(cached.lyrics);
            }
            return;
        }

        let title = track.title.clone();
        let artist = track.artists_name();
        let album = track.album.as_ref().map(|a| a.name.clone());
        let duration_secs = track.duration_ms / 1000;
        let weak = Rc::downgrade(self);

        glib::spawn_future_local(async move {
            let fetched = api
                .fetch_lyrics(
                    &key,
                    &title,
                    &artist,
                    album.as_deref(),
                    Some(duration_secs),
                    None,
                )
                .await;
            let Some(rt) = weak.upgrade() else {
                return;
            };
            if !rt.lyrics_enabled.get() {
                return;
            }
            if rt.lyrics_track_id.borrow().as_deref() != Some(track_id.as_str()) {
                return;
            }
            if let Some(found) = fetched {
                if found.lyrics.is_synced() {
                    *rt.lyrics.borrow_mut() = Some(found.lyrics);
                    rt.with_shown_view(|view| rt.update_progress_and_lyric(view));
                }
            }
        });
    }

    fn refresh_cached_lyrics_for(&self, updated_track_id: &str) {
        if !self.is_shown() || !self.lyrics_enabled.get() {
            return;
        }
        let Some(track) = self.app_model.get_state().playback.current_song() else {
            return;
        };
        if track.rri.id != updated_track_id {
            return;
        }
        let key = track
            .rri
            .uri
            .clone()
            .unwrap_or_else(|| track.rri.id.clone());
        let cached = self
            .app_model
            .api()
            .cached_lyrics(&key, false)
            .map(|found| found.lyrics)
            .filter(Lyrics::is_synced);
        *self.lyrics_track_id.borrow_mut() = Some(track.rri.id.clone());
        *self.lyrics.borrow_mut() = cached;
        self.with_shown_view(|view| self.update_progress_and_lyric(view));
    }

    fn update_progress_and_lyric(&self, view: &View) {
        let state = self.app_model.get_state();
        let playback = &state.playback;
        let progress_ms = playback.seek_position();
        let current_song = playback.current_song();
        let duration_ms = current_song.as_ref().map_or(0, |s| s.duration_ms);
        view.progress
            .set_fraction(progress_fraction(progress_ms, duration_ms));
        view.progress_hit.set_cursor_from_name(if duration_ms > 0 {
            Some("pointer")
        } else {
            None
        });

        let lyric = if self.lyrics_enabled.get()
            && current_song.as_ref().is_some_and(|t| {
                self.lyrics_track_id.borrow().as_deref() == Some(t.rri.id.as_str())
            }) {
            active_synced_lyric(
                self.lyrics.borrow().as_ref(),
                progress_ms,
                self.lyrics_offset_ms.get(),
            )
        } else {
            None
        };
        drop(state);

        match lyric {
            Some(line) => {
                if view.lyric_label.text().as_str() != line.as_str() {
                    view.lyric_label.set_text(&line);
                }
                view.lyric_label.set_visible(true);
            }
            None => {
                view.lyric_label.set_visible(false);
                if !view.lyric_label.text().is_empty() {
                    view.lyric_label.set_text("");
                }
            }
        }
    }

    fn handle_seek_click(&self, view: &View, x: f64) {
        let width = f64::from(view.progress_hit.width());
        let duration_ms = self
            .app_model
            .get_state()
            .playback
            .current_song()
            .map_or(0, |s| s.duration_ms);
        let Some(target_ms) = seek_target_ms(x, width, duration_ms) else {
            return;
        };
        view.progress
            .set_fraction(progress_fraction(u64::from(target_ms), duration_ms));
        self.dispatcher
            .dispatch(PlaybackAction::Seek(target_ms).into());
    }

    fn handle_volume_scroll(&self, controller: &gtk::EventControllerScroll, dy: f64) {
        let steps = scroll_steps_from_event(controller, dy);
        let next = volume_after_scroll(self.local_volume.get(), steps);
        self.local_volume.set(next);
        let dispatcher = self.dispatcher.clone();
        self.volume_debouncer.debounce(VOLUME_DEBOUNCE_MS, move || {
            dispatcher.dispatch(PlaybackAction::SetVolume(next).into());
        });
    }
}

/// Clicking the cover or the titles brings the full app back (and closes
/// the mini player); closing the mini player does too, so Canto never ends
/// up with no window.
fn link_to_main_window(view: &View, main: glib::WeakRef<libadwaita::ApplicationWindow>) {
    let show_main = {
        let main = main.clone();
        move || {
            if let Some(main) = main.upgrade() {
                main.set_visible(true);
                main.present();
            }
        }
    };
    for target in [
        view.cover_area.upcast_ref::<gtk::Widget>(),
        view.text_area.upcast_ref(),
    ] {
        let click = gtk::GestureClick::new();
        let mini = view.window.downgrade();
        click.connect_released(move |gesture, _, _, _| {
            gesture.set_state(gtk::EventSequenceState::Claimed);
            if let Some(mini) = mini.upgrade() {
                // `close` shows the main window through the handler below
                mini.close();
            }
        });
        target.add_controller(click);
        target.set_cursor_from_name(Some("pointer"));
    }
    view.window.connect_close_request(move |_| {
        show_main();
        glib::Propagation::Proceed
    });
}

fn icon_button(icon: &str, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::from_icon_name(icon);
    button.set_tooltip_text(Some(tooltip));
    button.set_valign(gtk::Align::Center);
    button.add_css_class("flat");
    button.add_css_class("circular");
    button.set_focus_on_click(false);
    button
}

fn update_lyrics_toggle_tooltip(button: &gtk::ToggleButton, enabled: bool) {
    let tip = if enabled {
        // translators: Tooltip on the Mini Player lyrics toggle when the live lyric line is enabled.
        gettext("Hide Synced Lyrics")
    } else {
        // translators: Tooltip on the Mini Player lyrics toggle when the live lyric line is hidden.
        gettext("Show Synced Lyrics")
    };
    button.set_tooltip_text(Some(&tip));
}

fn build_view(runtime: &Rc<Runtime>) -> View {
    let (view, [previous, next]) = build_layout();
    for (button, action) in [
        (&previous, PlaybackAction::Previous),
        (&view.play, PlaybackAction::TogglePlay),
        (&next, PlaybackAction::Next),
    ] {
        let dispatcher = runtime.dispatcher.clone();
        button.connect_clicked(move |_| dispatcher.dispatch(action.clone().into()));
    }

    let like_model = Rc::clone(&runtime.app_model);
    let like_dispatcher = runtime.dispatcher.clone();
    view.like
        .connect_clicked(move |_| toggle_current_track_like(&like_model, &like_dispatcher));

    // Toggle for the discreet 1-line synced lyric strip.
    let initial_lyrics = runtime.lyrics_enabled.get();
    view.lyrics_toggle.set_active(initial_lyrics);
    update_lyrics_toggle_tooltip(&view.lyrics_toggle, initial_lyrics);
    let weak_rt = Rc::downgrade(runtime);
    view.lyrics_toggle.connect_toggled(move |toggle| {
        let Some(rt) = weak_rt.upgrade() else {
            return;
        };
        let on = toggle.is_active();
        update_lyrics_toggle_tooltip(toggle, on);
        if rt.settings.boolean(MINI_PLAYER_LYRICS) != on {
            let _ = rt.settings.set_boolean(MINI_PLAYER_LYRICS, on);
        }
    });

    // Clickable bottom 2px progress bar to seek.
    let seek_click = gtk::GestureClick::new();
    let weak_rt = Rc::downgrade(runtime);
    seek_click.connect_released(move |gesture, _, x, _| {
        gesture.set_state(gtk::EventSequenceState::Claimed);
        if let Some(rt) = weak_rt.upgrade() {
            if let Some(view) = rt.view.borrow().as_ref() {
                rt.handle_seek_click(view, x);
            }
        }
    });
    view.progress_hit.add_controller(seek_click);

    // Wheel or touchpad volume scroll over the mini player window, reusing
    // the shared natural-scroll direction helper validated on Hyprland.
    let scroll = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::DISCRETE,
    );
    scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
    let weak_rt = Rc::downgrade(runtime);
    scroll.connect_scroll(move |controller, _, dy| {
        if let Some(rt) = weak_rt.upgrade() {
            rt.handle_volume_scroll(controller, dy);
        }
        glib::Propagation::Stop
    });
    view.window.add_controller(scroll);

    // Stop the live progress/lyric timer as soon as the mini player hides.
    let weak_rt = Rc::downgrade(runtime);
    view.window.connect_hide(move |_| {
        if let Some(rt) = weak_rt.upgrade() {
            rt.stop_timer();
        }
    });

    view
}

/// The window and its widgets, plus the previous and next buttons.
fn build_layout() -> (View, [gtk::Button; 2]) {
    let window = gtk::Window::new();
    // A fixed, untranslated title, so window managers can float it by rule
    // (see the README)
    window.set_title(Some("Canto Mini Player"));
    window.set_default_size(360, -1);
    window.set_resizable(false);
    window.set_hide_on_close(true);
    window.add_css_class("mini-player");

    let cover = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    cover.set_pixel_size(COVER_PX);
    let cover_clip = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cover_clip.set_overflow(gtk::Overflow::Hidden);
    cover_clip.set_valign(gtk::Align::Center);
    cover_clip.add_css_class("mini-player__cover");
    cover_clip.append(&cover);

    let title = gtk::Label::new(None);
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("heading");
    let artist = gtk::Label::new(None);
    artist.set_halign(gtk::Align::Start);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    artist.add_css_class("dim-label");
    artist.add_css_class("caption");

    let previous = icon_button("media-skip-backward-symbolic", &gettext("Previous"));
    let play = icon_button("media-playback-start-symbolic", &gettext("Play"));
    play.remove_css_class("flat");
    play.add_css_class("mini-player__play");
    let next = icon_button("media-skip-forward-symbolic", &gettext("Next"));

    let lyrics_toggle = gtk::ToggleButton::new();
    lyrics_toggle.set_icon_name("canto-lyrics-symbolic");
    lyrics_toggle.set_valign(gtk::Align::Center);
    lyrics_toggle.add_css_class("flat");
    lyrics_toggle.add_css_class("circular");
    lyrics_toggle.add_css_class("mini-player__lyrics-toggle");
    lyrics_toggle.set_focus_on_click(false);
    update_lyrics_toggle_tooltip(&lyrics_toggle, true);

    let like = icon_button("canto-heart-symbolic", "");
    like.add_css_class("heart-button");

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.append(&previous);
    controls.append(&play);
    controls.append(&next);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    controls.append(&spacer);
    controls.append(&lyrics_toggle);
    controls.append(&like);

    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
    titles.set_tooltip_text(Some(&gettext("Open Canto")));
    titles.append(&title);
    titles.append(&artist);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);
    text.append(&titles);
    text.append(&controls);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_top(12);
    row.set_margin_bottom(8);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.append(&cover_clip);
    row.append(&text);

    // Optional discreet 1-line live synced lyric strip below the controls.
    let lyric_label = gtk::Label::new(None);
    lyric_label.set_xalign(0.0);
    lyric_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    lyric_label.set_single_line_mode(true);
    lyric_label.set_margin_start(12);
    lyric_label.set_margin_end(12);
    lyric_label.set_margin_bottom(6);
    lyric_label.set_visible(false);
    lyric_label.add_css_class("caption");
    lyric_label.add_css_class("dim-label");
    lyric_label.add_css_class("mini-player__lyric");

    // Thin 2px progress bar flush along the bottom edge, hosted inside a
    // slightly taller invisible hit target so clicking to seek is effortless.
    let progress = gtk::ProgressBar::new();
    progress.set_hexpand(true);
    progress.set_valign(gtk::Align::End);
    progress.set_fraction(0.0);
    progress.add_css_class("mini-player__progress");

    let progress_hit = gtk::Box::new(gtk::Orientation::Vertical, 0);
    progress_hit.set_hexpand(true);
    progress_hit.set_valign(gtk::Align::End);
    progress_hit.set_height_request(6);
    progress_hit.add_css_class("mini-player__progress-hit");
    // translators: Tooltip on the Mini Player's bottom progress bar.
    progress_hit.set_tooltip_text(Some(&gettext("Click to seek")));
    let hit_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    hit_spacer.set_vexpand(true);
    progress_hit.append(&hit_spacer);
    progress_hit.append(&progress);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_width_request(360);
    root.append(&row);
    root.append(&lyric_label);
    root.append(&progress_hit);
    window.set_child(Some(&root));

    let keys = gtk::EventControllerKey::new();
    let weak = window.downgrade();
    keys.connect_key_pressed(move |_, key, _, _| {
        if key == gtk::gdk::Key::Escape {
            if let Some(window) = weak.upgrade() {
                window.close();
            }
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });
    window.add_controller(keys);

    cover_clip.set_tooltip_text(Some(&gettext("Open Canto")));
    let view = View {
        window,
        cover,
        title,
        artist,
        play,
        lyrics_toggle,
        like,
        lyric_label,
        progress_hit,
        progress,
        cover_area: cover_clip,
        text_area: titles,
    };
    (view, [previous, next])
}

fn update_play(view: &View, app_model: &AppModel) {
    let playing = app_model.get_state().playback.is_playing();
    view.play.set_icon_name(if playing {
        "media-playback-pause-symbolic"
    } else {
        "media-playback-start-symbolic"
    });
    view.play.set_tooltip_text(Some(&if playing {
        gettext("Pause")
    } else {
        gettext("Play")
    }));
}

fn update_view(runtime: &Rc<Runtime>, view: &View) {
    let app_model = &runtime.app_model;
    update_play(view, app_model);
    show_like_state(&view.like, current_track_liked(app_model));
    let song = app_model.get_state().playback.current_song();
    let Some(song) = song else {
        view.title.set_text(&gettext("Nothing Playing"));
        view.artist.set_text("");
        view.cover.set_icon_name(Some("audio-x-generic-symbolic"));
        view.progress.set_fraction(0.0);
        view.lyric_label.set_visible(false);
        view.lyric_label.set_text("");
        return;
    };
    view.title.set_text(&song.title);
    view.artist.set_text(&song.artists_name());
    let url = song
        .art
        .best_for_width((COVER_PX * 2) as u32)
        .filter(|url| !is_resource_url(url))
        .map(str::to_owned);
    let Some(url) = url else {
        view.cover.set_icon_name(Some("audio-x-generic-symbolic"));
        return;
    };
    let api = app_model.api();
    let weak = view.cover.downgrade();
    let tag = load::transport();
    glib::MainContext::default().spawn_local(async move {
        let texture = api.load_image(&url, COVER_PX * 2, COVER_PX * 2, tag).await;
        if let (Some(cover), Some(texture)) = (weak.upgrade(), texture) {
            cover.set_paintable(Some(&texture));
        }
    });
}

impl EventListener for MiniPlayer {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::LyricsUpdated(track_id) => {
                self.runtime.refresh_cached_lyrics_for(track_id);
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::PlaybackStopped,
            ) => {
                self.runtime
                    .with_shown_view(|view| update_view(&self.runtime, view));
                self.runtime.ensure_lyrics_loaded();
                self.runtime.sync_timer();
                self.runtime
                    .with_shown_view(|view| self.runtime.update_progress_and_lyric(view));
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlayStateSynced(_),
            ) => {
                self.runtime
                    .with_shown_view(|view| update_play(view, &self.runtime.app_model));
                self.runtime.sync_timer();
                self.runtime
                    .with_shown_view(|view| self.runtime.update_progress_and_lyric(view));
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::SeekSynced(_) | PlaybackEvent::TrackSeeked(_),
            ) => {
                self.runtime
                    .with_shown_view(|view| self.runtime.update_progress_and_lyric(view));
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::VolumeSet(vol) | PlaybackEvent::VolumeSynced(vol),
            ) => {
                self.runtime.local_volume.set(vol.clamp(0.0, 1.0));
            }
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => {
                self.runtime.with_shown_view(|view| {
                    show_like_state(&view.like, current_track_liked(&self.runtime.app_model))
                });
            }
            _ => {}
        }
    }
}

/// The mini player with a sample track, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests(cover: &gtk::gdk::Texture) -> gtk::Window {
    let (view, _) = build_layout();
    view.cover.set_paintable(Some(cover));
    view.title.set_text("TODOS ESSES MANOS");
    view.artist.set_text("Niink, Jotapê, Supernova Ent");
    view.play.set_icon_name("media-playback-pause-symbolic");
    view.lyrics_toggle.set_active(true);
    view.lyric_label.set_text("Disco voador passando na linha");
    view.lyric_label.set_visible(true);
    view.progress.set_fraction(0.42);
    show_like_state(&view.like, Some(true));
    view.window
}

#[cfg(test)]
pub(crate) struct TestMiniPlayerParts {
    pub window: gtk::Window,
    pub progress: gtk::ProgressBar,
    pub progress_hit: gtk::Box,
    pub lyric_label: gtk::Label,
    pub lyrics_toggle: gtk::ToggleButton,
}

#[cfg(test)]
impl MiniPlayer {
    pub(crate) fn new_for_tests(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            runtime: Runtime::new(app_model, dispatcher),
        }
    }

    pub(crate) fn present_for_tests(&self) -> TestMiniPlayerParts {
        self.runtime.present(None, None);
        let view = self.runtime.view.borrow();
        let view = view.as_ref().expect("view created on present");
        TestMiniPlayerParts {
            window: view.window.clone(),
            progress: view.progress.clone(),
            progress_hit: view.progress_hit.clone(),
            lyric_label: view.lyric_label.clone(),
            lyrics_toggle: view.lyrics_toggle.clone(),
        }
    }

    pub(crate) fn simulate_seek_click_for_tests(&self, x: f64) {
        if let Some(view) = self.runtime.view.borrow().as_ref() {
            self.runtime.handle_seek_click(view, x);
        }
    }

    pub(crate) fn set_lyrics_for_tests(&self, track_id: &str, lyrics: Option<Lyrics>) {
        *self.runtime.lyrics_track_id.borrow_mut() = Some(track_id.to_string());
        *self.runtime.lyrics.borrow_mut() = lyrics;
        self.runtime
            .with_shown_view(|view| self.runtime.update_progress_and_lyric(view));
    }

    pub(crate) fn local_volume_for_tests(&self) -> f64 {
        self.runtime.local_volume.get()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_fraction_clamps_and_handles_zero_duration() {
        assert_eq!(progress_fraction(0, 0), 0.0);
        assert_eq!(progress_fraction(5_000, 0), 0.0);
        assert_eq!(progress_fraction(0, 100_000), 0.0);
        assert!((progress_fraction(25_000, 100_000) - 0.25).abs() < 1e-9);
        assert_eq!(progress_fraction(150_000, 100_000), 1.0);
    }

    #[test]
    fn seek_target_ms_maps_clicks_and_clamps_edges() {
        assert_eq!(seek_target_ms(10.0, 0.0, 200_000), None);
        assert_eq!(seek_target_ms(10.0, 360.0, 0), None);
        assert_eq!(seek_target_ms(-20.0, 360.0, 200_000), Some(0));
        assert_eq!(seek_target_ms(0.0, 360.0, 200_000), Some(0));
        assert_eq!(seek_target_ms(180.0, 360.0, 200_000), Some(100_000));
        assert_eq!(seek_target_ms(500.0, 360.0, 200_000), Some(200_000));
    }
}
