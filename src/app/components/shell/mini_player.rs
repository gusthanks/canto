// Mini player: a small separate window with the cover, the track and the
// main controls, to keep playback at hand while Canto's window is away.

use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::now_panel::{current_track_liked, show_like_state, toggle_current_track_like};
use crate::app::components::EventListener;
use crate::app::models::TrackExt;
use crate::app::state::{PlaybackAction, PlaybackEvent};
use crate::app::{load, AppEvent, AppModel, BrowserEvent, Dispatcher};
use canto_api::models::is_resource_url;

pub const SHOW_MINI_PLAYER_ACTION: &str = "mini-player";

/// Side of the cover, in pixels.
const COVER_PX: i32 = 64;

struct View {
    window: gtk::Window,
    cover: gtk::Image,
    title: gtk::Label,
    artist: gtk::Label,
    play: gtk::Button,
    like: gtk::Button,
    /// The cover and the titles: clicking them opens the full app.
    cover_area: gtk::Box,
    text_area: gtk::Box,
}

pub struct MiniPlayer {
    app_model: Rc<AppModel>,
    view: Rc<RefCell<Option<View>>>,
}

impl MiniPlayer {
    pub fn new(
        main_window: &libadwaita::ApplicationWindow,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let view: Rc<RefCell<Option<View>>> = Rc::new(RefCell::new(None));
        let action = gio::SimpleAction::new(SHOW_MINI_PLAYER_ACTION, None);
        let action_view = Rc::clone(&view);
        let action_model = Rc::clone(&app_model);
        let application = main_window.application();
        let main = main_window.downgrade();
        action.connect_activate(move |_, _| {
            let mut slot = action_view.borrow_mut();
            let view = slot.get_or_insert_with(|| {
                let view = build_view(&action_model, &dispatcher);
                view.window.set_application(application.as_ref());
                link_to_main_window(&view, main.clone());
                view
            });
            update_view(view, &action_model);
            view.window.present();
            // The mini player stands in for the app: the big window goes away
            // until the mini player is clicked or closed
            if let Some(main) = main.upgrade() {
                main.set_visible(false);
            }
        });
        main_window.add_action(&action);
        Self { app_model, view }
    }

    fn with_shown_view(&self, f: impl FnOnce(&View, &AppModel)) {
        if let Some(view) = self.view.borrow().as_ref() {
            if view.window.is_visible() {
                f(view, &self.app_model);
            }
        }
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

fn build_view(app_model: &Rc<AppModel>, dispatcher: &Dispatcher) -> View {
    let (view, [previous, next]) = build_layout();
    for (button, action) in [
        (&previous, PlaybackAction::Previous),
        (&view.play, PlaybackAction::TogglePlay),
        (&next, PlaybackAction::Next),
    ] {
        let dispatcher = dispatcher.clone();
        button.connect_clicked(move |_| dispatcher.dispatch(action.clone().into()));
    }
    let like_model = Rc::clone(app_model);
    let like_dispatcher = dispatcher.clone();
    view.like
        .connect_clicked(move |_| toggle_current_track_like(&like_model, &like_dispatcher));
    view
}

/// The window and its widgets, plus the previous and next buttons.
fn build_layout() -> (View, [gtk::Button; 2]) {
    let window = gtk::Window::new();
    // A fixed, untranslated title, so window managers can float it by rule
    // (see the README)
    window.set_title(Some("Canto Mini Player"));
    window.set_default_size(360, 96);
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
    let like = icon_button("canto-heart-symbolic", "");
    like.add_css_class("heart-button");

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    controls.append(&previous);
    controls.append(&play);
    controls.append(&next);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    controls.append(&spacer);
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
    row.set_margin_bottom(12);
    row.set_margin_start(12);
    row.set_margin_end(12);
    row.append(&cover_clip);
    row.append(&text);
    window.set_child(Some(&row));

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
        like,
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

fn update_view(view: &View, app_model: &AppModel) {
    update_play(view, app_model);
    show_like_state(&view.like, current_track_liked(app_model));
    let song = app_model.get_state().playback.current_song();
    let Some(song) = song else {
        view.title.set_text(&gettext("Nothing Playing"));
        view.artist.set_text("");
        view.cover.set_icon_name(Some("audio-x-generic-symbolic"));
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
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::PlaybackStopped,
            ) => self.with_shown_view(update_view),
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlayStateSynced(_),
            ) => self.with_shown_view(update_play),
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => {
                self.with_shown_view(|view, app_model| {
                    show_like_state(&view.like, current_track_liked(app_model))
                })
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
    show_like_state(&view.like, Some(true));
    view.window
}
