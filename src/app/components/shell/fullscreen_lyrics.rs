// Full-screen lyrics: the sharp cover on the left, the synced lyrics big on
// the right, over a solid dark shade of the cover's color, in their own
// full-screen window (Escape or the close button leave it).

use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::components::{EventListener, LyricsPage};
use crate::app::models::TrackExt;
use crate::app::state::PlaybackEvent;
use crate::app::{load, AppEvent, AppModel, Dispatcher};
use canto_api::models::is_resource_url;

pub const SHOW_FULLSCREEN_LYRICS_ACTION: &str = "fullscreen-lyrics";

/// The cover is decoded at this size: sharp on a full-screen display.
const COVER_PX: i32 = 640;
/// Logical size of the cover on screen.
const COVER_SIZE: i32 = 400;
/// Background when there is no cover color.
const FALLBACK_BG: &str = "#121212";

struct View {
    window: gtk::Window,
    cover: gtk::Picture,
    title: gtk::Label,
    artist: gtk::Label,
    background: gtk::CssProvider,
}

/// A solid, dark shade of the cover's color for the whole window.
fn background_css(cover_color: Option<&str>) -> String {
    let color = cover_color
        .and_then(crate::app::theme::valid_hex_color)
        .map(|hex| format!("mix({hex}, #0a0a0a, 0.78)"))
        .unwrap_or_else(|| FALLBACK_BG.to_string());
    format!(".lyrics-fullscreen-window {{ background-color: {color}; }}")
}

fn set_cover(view: &View, texture: Option<&gtk::gdk::Texture>) {
    view.cover.set_paintable(texture);
    let color = texture.and_then(crate::app::theme::extract_cover_accent_from_texture);
    view.background
        .load_from_string(&background_css(color.as_deref()));
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
            let view =
                slot.get_or_insert_with(|| build_view(Some(parent.upcast_ref()), &lyrics_widget));
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

    let background = gtk::CssProvider::new();
    background.load_from_string(&background_css(None));
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &background,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }

    // Left: the sharp cover, with the title and artist under it
    let cover = gtk::Picture::new();
    cover.set_content_fit(gtk::ContentFit::Cover);
    cover.set_can_shrink(true);
    cover.set_size_request(COVER_SIZE, COVER_SIZE);
    // The clip rounds the corners; the shadow sits on the box around it
    let cover_clip = gtk::Box::new(gtk::Orientation::Vertical, 0);
    cover_clip.set_overflow(gtk::Overflow::Hidden);
    cover_clip.add_css_class("lyrics-fullscreen__cover-clip");
    cover_clip.append(&cover);
    let cover_shadow = gtk::Box::new(gtk::Orientation::Vertical, 0);
    cover_shadow.add_css_class("lyrics-fullscreen__cover");
    cover_shadow.set_halign(gtk::Align::Start);
    cover_shadow.append(&cover_clip);

    let title = gtk::Label::new(None);
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_max_width_chars(22);
    title.add_css_class("title-1");
    let artist = gtk::Label::new(None);
    artist.set_halign(gtk::Align::Start);
    artist.set_xalign(0.0);
    artist.set_wrap(true);
    artist.set_max_width_chars(30);
    artist.add_css_class("lyrics-fullscreen__artist");

    let side = gtk::Box::new(gtk::Orientation::Vertical, 6);
    side.set_valign(gtk::Align::Center);
    side.set_width_request(COVER_SIZE);
    side.append(&cover_shadow);
    title.set_margin_top(22);
    side.append(&title);
    side.append(&artist);

    // Right: the lyrics
    let clamp = libadwaita::Clamp::new();
    clamp.set_maximum_size(820);
    clamp.set_hexpand(true);
    clamp.set_vexpand(true);
    clamp.set_child(Some(lyrics));

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 64);
    row.set_margin_top(48);
    row.set_margin_bottom(32);
    row.set_margin_start(64);
    row.set_margin_end(64);
    row.append(&side);
    row.append(&clamp);

    let close = gtk::Button::from_icon_name("view-restore-symbolic");
    close.set_halign(gtk::Align::End);
    close.set_valign(gtk::Align::Start);
    close.set_margin_top(24);
    close.set_margin_end(24);
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

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&row));
    overlay.add_overlay(&close);
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
        cover,
        title,
        artist,
        background,
    }
}

fn update_view(view: &View, app_model: &AppModel) {
    let song = app_model.get_state().playback.current_song();
    let Some(song) = song else {
        view.title.set_text(&gettext("No Track Playing"));
        view.artist.set_text("");
        set_cover(view, None);
        return;
    };
    view.title.set_text(&song.title);
    view.artist.set_text(&song.artists_name());
    let Some(url) = song
        .art
        .best_for_width(COVER_PX as u32)
        .filter(|url| !is_resource_url(url))
        .map(str::to_owned)
    else {
        set_cover(view, None);
        return;
    };
    let api = app_model.api();
    let cover = view.cover.downgrade();
    let background = view.background.clone();
    let tag = load::transport();
    glib::MainContext::default().spawn_local(async move {
        let texture = api.load_image(&url, COVER_PX, COVER_PX, tag).await;
        if let (Some(cover), Some(texture)) = (cover.upgrade(), texture) {
            cover.set_paintable(Some(&texture));
            let color = crate::app::theme::extract_cover_accent_from_texture(&texture);
            background.load_from_string(&background_css(color.as_deref()));
        }
    });
}

impl EventListener for FullscreenLyrics {
    fn on_event(&mut self, event: &AppEvent) {
        self.lyrics.on_event(event);
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::PlaybackStopped,
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
    use canto_api::{LyricLine, Lyrics};

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
    set_cover(&view, Some(cover));
    view.window
}
