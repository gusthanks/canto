// Full-screen lyrics: the synced lyrics big over a blurred, darkened cover,
// in their own full-screen window (Escape or the close button leave it).

use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::components::{EventListener, LyricsPage};
use crate::app::models::TrackExt;
use crate::app::state::PlaybackEvent;
use crate::app::{load, AppEvent, AppModel, Dispatcher};
use riff_api::models::is_resource_url;

pub const SHOW_FULLSCREEN_LYRICS_ACTION: &str = "fullscreen-lyrics";

/// The cover is decoded this small and scaled up: a cheap blur.
const BLUR_PX: i32 = 24;

struct View {
    window: gtk::Window,
    backdrop: gtk::Picture,
    title: gtk::Label,
    artist: gtk::Label,
}

pub struct FullscreenLyrics {
    lyrics: LyricsPage,
    app_model: Rc<AppModel>,
    view: Rc<RefCell<Option<View>>>,
}

impl FullscreenLyrics {
    pub fn new(
        main_window: &libadwaita::ApplicationWindow,
        app_model: Rc<AppModel>,
        dispatcher: Dispatcher,
    ) -> Self {
        let lyrics = LyricsPage::new(app_model.clone(), dispatcher);
        lyrics.widget().add_css_class("lyrics--fullscreen");
        lyrics.widget().set_fullscreen_button_visible(false);
        let this = Self {
            lyrics,
            app_model,
            view: Rc::new(RefCell::new(None)),
        };
        let action = gio::SimpleAction::new(SHOW_FULLSCREEN_LYRICS_ACTION, None);
        let view = Rc::clone(&this.view);
        let lyrics_widget: gtk::Widget = this.lyrics.widget().clone().upcast();
        let app_model = Rc::clone(&this.app_model);
        let parent = main_window.clone();
        action.connect_activate(move |_, _| {
            let mut slot = view.borrow_mut();
            let view = slot.get_or_insert_with(|| build_view(Some(parent.upcast_ref()), &lyrics_widget));
            update_view(view, &app_model);
            view.window.present();
            view.window.fullscreen();
        });
        main_window.add_action(&action);
        this
    }

    fn refresh(&self) {
        if let Some(view) = self.view.borrow().as_ref() {
            if view.window.is_visible() {
                update_view(view, &self.app_model);
            }
        }
    }
}

fn build_view(parent: Option<&gtk::Window>, lyrics: &gtk::Widget) -> View {
    let window = gtk::Window::new();
    window.set_transient_for(parent);
    window.set_decorated(false);
    window.set_hide_on_close(true);
    // translators: Title of the full-screen lyrics window.
    window.set_title(Some(&gettext("Lyrics")));
    window.add_css_class("lyrics-fullscreen-window");

    let backdrop = gtk::Picture::new();
    backdrop.set_content_fit(gtk::ContentFit::Cover);
    backdrop.set_can_shrink(true);
    backdrop.add_css_class("lyrics-fullscreen__backdrop");

    let shade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    shade.add_css_class("lyrics-fullscreen__shade");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_margin_top(28);
    content.set_margin_bottom(28);
    content.set_margin_start(40);
    content.set_margin_end(40);

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title = gtk::Label::new(None);
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.add_css_class("title-2");
    let artist = gtk::Label::new(None);
    artist.set_halign(gtk::Align::Start);
    artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
    artist.add_css_class("dim-label");
    text.append(&title);
    text.append(&artist);
    top.append(&text);
    let close = gtk::Button::from_icon_name("view-restore-symbolic");
    close.set_valign(gtk::Align::Center);
    close.add_css_class("circular");
    close.add_css_class("osd");
    // translators: Tooltip of the button leaving full-screen lyrics.
    close.set_tooltip_text(Some(&gettext("Exit Full Screen (Esc)")));
    let weak = window.downgrade();
    close.connect_clicked(move |_| {
        if let Some(window) = weak.upgrade() {
            window.close();
        }
    });
    top.append(&close);
    content.append(&top);

    let clamp = libadwaita::Clamp::new();
    clamp.set_maximum_size(900);
    clamp.set_vexpand(true);
    clamp.set_child(Some(lyrics));
    content.append(&clamp);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&backdrop));
    overlay.add_overlay(&shade);
    overlay.add_overlay(&content);
    window.set_child(Some(&overlay));

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

    View {
        window,
        backdrop,
        title,
        artist,
    }
}

fn update_view(view: &View, app_model: &AppModel) {
    let song = app_model.get_state().playback.current_song();
    let Some(song) = song else {
        view.title.set_text(&gettext("No Track Playing"));
        view.artist.set_text("");
        view.backdrop.set_paintable(None::<&gtk::gdk::Paintable>);
        return;
    };
    view.title.set_text(&song.title);
    view.artist.set_text(&song.artists_name());
    let Some(url) = song
        .art
        .best_for_width(64)
        .filter(|url| !is_resource_url(url))
        .map(str::to_owned)
    else {
        view.backdrop.set_paintable(None::<&gtk::gdk::Paintable>);
        return;
    };
    let api = app_model.api();
    let weak = view.backdrop.downgrade();
    let tag = load::transport();
    glib::MainContext::default().spawn_local(async move {
        let texture = api.load_image(&url, BLUR_PX, BLUR_PX, tag).await;
        if let (Some(backdrop), Some(texture)) = (weak.upgrade(), texture) {
            backdrop.set_paintable(Some(&texture));
        }
    });
}

impl EventListener for FullscreenLyrics {
    fn on_event(&mut self, event: &AppEvent) {
        self.lyrics.on_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_) | PlaybackEvent::TrackSynced | PlaybackEvent::PlaybackStopped,
        ) = event
        {
            self.refresh();
        }
    }
}

/// The full-screen view with sample lyrics, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests(cover: &gtk::gdk::Texture) -> gtk::Window {
    use crate::app::components::LyricsPageWidget;
    use riff_api::{LyricLine, Lyrics};

    let lyrics_widget = LyricsPageWidget::default();
    lyrics_widget.add_css_class("lyrics--fullscreen");
    lyrics_widget.set_fullscreen_button_visible(false);
    let lines = [
        "Todos esses manos querem ser",
        "Mas ninguém quer pagar o preço",
        "Eu vim do nada e tô aqui",
        "Olha onde a gente chegou",
        "Ninguém acreditou em nós",
        "Hoje eles cantam junto",
    ];
    let lyrics = Lyrics::Synced(
        lines
            .iter()
            .enumerate()
            .map(|(i, text)| LyricLine {
                start_time_ms: i as u64 * 4000,
                text: text.to_string(),
            })
            .collect(),
    );
    lyrics_widget.set_lyrics(&lyrics, |_| {});
    // Halfway through the active line
    lyrics_widget.set_position(Some((2, 0.5)));
    let view = build_view(None, lyrics_widget.upcast_ref());
    view.title.set_text("TODOS ESSES MANOS");
    view.artist.set_text("Niink, Jotapê, Supernova Ent");
    view.backdrop.set_paintable(Some(cover));
    view.window
}
