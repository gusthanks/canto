// The "Now Playing" view of the side panel, in place of the old page: the
// cover, the track with links to its album and artists, a like button, a
// lyrics preview, the next track and the Genius credits.

use gettextrs::gettext;
use gio::prelude::SettingsExt;
use glib::clone;
use gtk::prelude::*;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::app::components::utils::Clock;
use crate::app::components::{dispatch_api_call, labels, EventListener};
use crate::app::models::{Track, TrackExt};
use crate::app::state::PlaybackEvent;
use crate::app::{load, AppAction, AppEvent, AppModel, BrowserAction, BrowserEvent, Dispatcher};
use canto_api::models::is_resource_url;
use canto_api::{GeniusTrackInfo, Lyrics};

mod about;
mod info_page;
mod stats;

pub use about::AboutPanel;
use canto_config::settings::{
    GENIUS_ACCESS_TOKEN, LYRICS_GENIUS_FALLBACK, LYRICS_OFFSET_MS, SETTINGS,
};
pub use stats::StatsPanel;

/// Lines shown in the lyrics preview card.
const PREVIEW_LINES: usize = 4;
/// Pixels decoded for the big cover.
const COVER_PX: i32 = 640;
/// Pixels decoded for the next track's cover.
const NEXT_COVER_PX: i32 = 96;

pub const HEART_ICON: &str = "canto-heart-symbolic";
pub const HEART_FILLED_ICON: &str = "canto-heart-filled-symbolic";

/// Markup for a track title that links to its album (plain without one).
pub fn title_markup(title: &str, has_album: bool) -> String {
    let title = glib::markup_escape_text(title);
    if has_album {
        let tip = glib::markup_escape_text(&gettext("Go to Album"));
        format!("<a href=\"album\" title=\"{tip}\">{title}</a>")
    } else {
        title.to_string()
    }
}

/// Markup for the artists of a track, each linking to its page ("artist:ID").
pub fn artists_markup(track: &Track) -> String {
    track
        .artists
        .iter()
        .map(|artist| {
            let name = glib::markup_escape_text(&artist.name);
            if artist.rri.id.is_empty() {
                name.to_string()
            } else {
                format!("<a href=\"artist:{}\">{name}</a>", artist.rri.id)
            }
        })
        .collect::<Vec<_>>()
        .join(", ")
}

/// What a link in a title or artist label points to.
pub enum TrackLink {
    Album,
    Artist(String),
}

pub fn parse_track_link(uri: &str) -> Option<TrackLink> {
    if uri == "album" {
        Some(TrackLink::Album)
    } else {
        uri.strip_prefix("artist:")
            .filter(|id| !id.is_empty())
            .map(|id| TrackLink::Artist(id.to_string()))
    }
}

/// Opens what a title/artist link points to, for the current track.
pub fn follow_track_link(app_model: &AppModel, dispatcher: &Dispatcher, uri: &str) {
    match parse_track_link(uri) {
        Some(TrackLink::Album) => {
            let album = app_model
                .get_state()
                .playback
                .current_song()
                .and_then(|song| song.album)
                .map(|album| album.rri.id)
                .filter(|id| !id.is_empty());
            if let Some(id) = album {
                dispatcher.dispatch(AppAction::ViewAlbum(id));
            }
        }
        Some(TrackLink::Artist(id)) => dispatcher.dispatch(AppAction::ViewArtist(id)),
        None => {}
    }
}

/// Whether the current track is in the user's saved tracks.
pub fn current_track_liked(app_model: &AppModel) -> Option<bool> {
    let state = app_model.get_state();
    let song = state.playback.current_song()?;
    Some(
        state
            .browser
            .home_state()
            .is_some_and(|home| home.is_track_liked(&song.rri.id)),
    )
}

/// Saves the current track, or removes it from the saved tracks.
pub fn toggle_current_track_like(app_model: &AppModel, dispatcher: &Dispatcher) {
    let Some(song) = app_model.get_state().playback.current_song() else {
        return;
    };
    let liked = current_track_liked(app_model).unwrap_or(false);
    let id = song.rri.id.clone();
    let api = app_model.api();
    if liked {
        dispatch_api_call(dispatcher, move || async move {
            api.remove_tracks(vec![id.clone()]).await?;
            Ok(BrowserAction::RemoveSavedTracks(vec![id]).into())
        });
    } else {
        dispatch_api_call(dispatcher, move || async move {
            api.save_tracks(vec![id]).await?;
            Ok(BrowserAction::SaveTracks(vec![song]).into())
        });
    }
}

/// Shows the like state on a heart button (hidden when nothing plays).
pub fn show_like_state(button: &gtk::Button, liked: Option<bool>) {
    button.set_visible(liked.is_some());
    let liked = liked.unwrap_or(false);
    button.set_icon_name(if liked { HEART_FILLED_ICON } else { HEART_ICON });
    button.set_tooltip_text(Some(&if liked {
        labels::UNLIKE.clone()
    } else {
        labels::LIKE.clone()
    }));
    if liked {
        button.add_css_class("heart--liked");
    } else {
        button.remove_css_class("heart--liked");
    }
}

/// Which lines the lyrics preview shows, and which of them is being sung.
/// Empty lines (instrumental breaks) are skipped.
pub fn preview_window(lyrics: &Lyrics, position_ms: u64) -> (Vec<String>, Option<usize>) {
    match lyrics {
        Lyrics::Synced(lines) => {
            let active = lyrics.active_line_index(position_ms);
            let start = active.unwrap_or(0);
            let shown: Vec<String> = lines
                .iter()
                .skip(start)
                .map(|line| line.text.trim().to_string())
                .filter(|text| !text.is_empty())
                .take(PREVIEW_LINES)
                .collect();
            let current = active
                .filter(|&i| !lines[i].text.trim().is_empty())
                .map(|_| 0);
            (shown, current)
        }
        Lyrics::Plain(lines) => (
            lines
                .iter()
                .map(|line| line.trim().to_string())
                .filter(|text| !text.is_empty())
                .take(PREVIEW_LINES)
                .collect(),
            None,
        ),
    }
}

fn adjusted_position(pos_ms: u64, offset_ms: i32) -> u64 {
    (pos_ms as i128 + i128::from(offset_ms)).clamp(0, i128::from(u64::MAX)) as u64
}

fn heading(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("heading");
    label.set_halign(gtk::Align::Start);
    label
}

/// The start of a long bio: whole paragraphs up to about 280 characters
/// (at least the first). `None` when the bio is short enough to show whole.
fn folded_bio(markup: &str) -> Option<String> {
    const FOLD_AT: usize = 280;
    let paragraphs: Vec<&str> = markup.split("\n\n").collect();
    if paragraphs.len() < 2 {
        return None;
    }
    let mut kept = 0;
    let mut length = 0;
    for paragraph in &paragraphs {
        if kept > 0 && length + paragraph.len() > FOLD_AT {
            break;
        }
        length += paragraph.len();
        kept += 1;
    }
    (kept < paragraphs.len()).then(|| paragraphs[..kept].join("\n\n"))
}

fn wrapping_label(classes: &[&str]) -> gtk::Label {
    let label = gtk::Label::new(None);
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    for class in classes {
        label.add_css_class(class);
    }
    label
}

fn card(classes: &[&str]) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    card.add_css_class("now-panel__card");
    for class in classes {
        card.add_css_class(class);
    }
    card
}

/// Numbers side by side, each with what it counts under it ("12 / this
/// song"). `None` when none of the numbers is known.
fn number_columns(columns: Vec<(Option<u64>, String)>) -> Option<gtk::Box> {
    let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    body.set_homogeneous(true);
    let mut any = false;
    for (count, what) in columns {
        let Some(count) = count else { continue };
        any = true;
        let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let number = gtk::Label::new(Some(&format_count(count)));
        number.set_halign(gtk::Align::Start);
        number.add_css_class("now-panel__stat-number");
        let label = wrapping_label(&["dim-label", "caption"]);
        label.set_text(&what);
        column.append(&number);
        column.append(&label);
        body.append(&column);
    }
    any.then_some(body)
}

/// Makes a whole card clickable, running `action` (a window action).
fn make_card_activate(card: &gtk::Box, action: &'static str, tooltip: &str) {
    card.set_cursor_from_name(Some("pointer"));
    card.set_tooltip_text(Some(tooltip));
    card.add_css_class("now-panel__card--link");
    let gesture = gtk::GestureClick::new();
    gesture.set_button(gtk::gdk::BUTTON_PRIMARY);
    gesture.connect_released(move |gesture, _, _, _| {
        if let Some(widget) = gesture.widget() {
            let _ = widget.activate_action(action, None);
        }
    });
    card.add_controller(gesture);
}

mod square_imp {
    use gtk::prelude::*;
    use gtk::subclass::prelude::*;
    use libadwaita::prelude::*;
    use libadwaita::subclass::prelude::*;

    #[derive(Debug, Default)]
    pub struct SquareBin;

    #[glib::object_subclass]
    impl ObjectSubclass for SquareBin {
        const NAME: &'static str = "CantoSquareBin";
        type Type = super::SquareBin;
        type ParentType = libadwaita::Bin;
    }

    impl ObjectImpl for SquareBin {
        fn constructed(&self) {
            self.parent_constructed();
            // AdwBin measures through a layout manager, which would bypass
            // the measure below
            self.obj().set_layout_manager(None::<gtk::LayoutManager>);
        }
    }
    impl BinImpl for SquareBin {}

    impl WidgetImpl for SquareBin {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                // As wide as it's given, from nothing
                gtk::Orientation::Horizontal => (0, 0, -1, -1),
                // As tall as it's wide
                _ => {
                    let side = for_size.max(0);
                    (side, side, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, height: i32, baseline: i32) {
            if let Some(child) = self.obj().child() {
                child.allocate(width, height, baseline, None);
            }
        }
    }
}

glib::wrapper! {
    /// A box that's always square: as tall as it is wide (for the cover).
    pub struct SquareBin(ObjectSubclass<square_imp::SquareBin>)
        @extends gtk::Widget, libadwaita::Bin,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for SquareBin {
    fn default() -> Self {
        glib::Object::new()
    }
}

struct Widgets {
    stack: gtk::Stack,
    scroller: gtk::ScrolledWindow,
    cover: gtk::Picture,
    title: gtk::Label,
    artists: gtk::Label,
    like: gtk::Button,
    lyrics_card: gtk::Box,
    lyrics_lines: Vec<gtk::Label>,
    next_card: gtk::Box,
    next_cover: gtk::Image,
    next_title: gtk::Label,
    next_artist: gtk::Label,
    credits_body: gtk::Box,
    stats_card: gtk::Box,
    stats_body: gtk::Box,
}

impl Widgets {
    fn build() -> Self {
        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.add_css_class("now-panel");

        let empty = libadwaita::StatusPage::new();
        empty.set_icon_name(Some("audio-x-generic-symbolic"));
        // translators: Side panel view of the current track, when nothing plays.
        empty.set_title(&gettext("Nothing Playing"));
        empty.set_description(Some(&gettext("Play something and it shows up here.")));
        empty.add_css_class("compact");
        stack.add_named(&empty, Some("empty"));

        let content = gtk::Box::new(gtk::Orientation::Vertical, 16);
        content.set_margin_top(4);
        content.set_margin_bottom(20);
        content.set_margin_start(16);
        content.set_margin_end(16);

        let cover = gtk::Picture::new();
        cover.set_content_fit(gtk::ContentFit::Cover);
        cover.set_can_shrink(true);
        cover.set_overflow(gtk::Overflow::Hidden);
        cover.add_css_class("now-panel__cover");
        let cover_box = SquareBin::default();
        cover_box.set_child(Some(&cover));
        content.append(&cover_box);

        let info = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let title = wrapping_label(&["title-2", "now-panel__title", "track-links"]);
        let artists = wrapping_label(&["now-panel__artists", "track-links", "dim-label"]);
        text.append(&title);
        text.append(&artists);
        info.append(&text);
        let like = gtk::Button::from_icon_name(HEART_ICON);
        like.set_valign(gtk::Align::Center);
        like.add_css_class("flat");
        like.add_css_class("circular");
        like.add_css_class("heart-button");
        info.append(&like);
        content.append(&info);

        let lyrics_card = card(&["now-panel__lyrics"]);
        lyrics_card.append(&heading(&gettext("Lyrics")));
        let lyrics_lines: Vec<gtk::Label> = (0..PREVIEW_LINES)
            .map(|_| {
                let line = wrapping_label(&["now-panel__lyric-line"]);
                lyrics_card.append(&line);
                line
            })
            .collect();
        // translators: Tooltip of the lyrics preview card in the Now Playing panel.
        make_card_activate(&lyrics_card, "win.show-lyrics", &gettext("Show Lyrics"));
        lyrics_card.set_visible(false);
        content.append(&lyrics_card);

        let next_card = card(&["now-panel__next"]);
        // translators: Heading of the card showing the track that plays next.
        next_card.append(&heading(&gettext("Next in Queue")));
        let next_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let next_cover = gtk::Image::new();
        next_cover.set_pixel_size(48);
        next_cover.set_overflow(gtk::Overflow::Hidden);
        next_cover.add_css_class("now-panel__next-cover");
        next_row.append(&next_cover);
        let next_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        next_text.set_valign(gtk::Align::Center);
        next_text.set_hexpand(true);
        let next_title = gtk::Label::new(None);
        next_title.set_halign(gtk::Align::Start);
        next_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        next_title.add_css_class("heading");
        let next_artist = gtk::Label::new(None);
        next_artist.set_halign(gtk::Align::Start);
        next_artist.set_ellipsize(gtk::pango::EllipsizeMode::End);
        next_artist.add_css_class("dim-label");
        next_artist.add_css_class("caption");
        next_text.append(&next_title);
        next_text.append(&next_artist);
        next_row.append(&next_text);
        next_card.append(&next_row);
        // translators: Tooltip of the next track card in the Now Playing panel.
        make_card_activate(&next_card, "win.show-queue", &gettext("Show Queue"));
        next_card.set_visible(false);
        content.append(&next_card);

        // translators: Heading of the card with how many times the user played the track, its album and artist (from Last.fm).
        let stats_card = card(&["now-panel__stats"]);
        stats_card.append(&heading(&gettext("Your Plays")));
        let stats_body = gtk::Box::new(gtk::Orientation::Vertical, 0);
        stats_card.append(&stats_body);
        stats_card.set_visible(false);
        // A click opens the Stats tab, with everything Last.fm knows
        // translators: Tooltip of the "Your Plays" card in the Now Playing panel: it opens the Stats tab.
        make_card_activate(&stats_card, "win.show-stats", &gettext("Show Song Stats"));
        content.append(&stats_card);

        let credits_card = card(&["now-panel__credits"]);
        credits_card.append(&heading(&gettext("Credits")));
        let credits_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
        credits_card.append(&credits_body);
        make_card_activate(
            &credits_card,
            "win.show-about",
            // A click opens the About tab, with the full credits and the story
            // translators: Tooltip of the Credits card in the Now Playing panel: it opens the About tab.
            &gettext("Show About This Song"),
        );
        content.append(&credits_card);

        let clamp = libadwaita::Clamp::new();
        clamp.set_maximum_size(560);
        clamp.set_child(Some(&content));
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&clamp));
        stack.add_named(&scroller, Some("content"));

        Self {
            stack,
            scroller,
            cover,
            title,
            artists,
            like,
            lyrics_card,
            lyrics_lines,
            next_card,
            next_cover,
            next_title,
            next_artist,
            credits_body,
            stats_card,
            stats_body,
        }
    }

    /// The play counts, as three columns: song, album, artist.
    fn show_playcounts(&self, counts: &canto_api::scrobbler::Playcounts) {
        while let Some(child) = self.stats_body.first_child() {
            self.stats_body.remove(&child);
        }
        let columns = playcount_columns(counts);
        self.stats_card.set_visible(columns.is_some());
        if let Some(columns) = columns {
            self.stats_body.append(&columns);
        }
    }

    fn clear_credits(&self) {
        while let Some(child) = self.credits_body.first_child() {
            self.credits_body.remove(&child);
        }
    }

    fn credits_text(&self, text: &str) {
        self.clear_credits();
        let label = wrapping_label(&["dim-label"]);
        label.set_text(text);
        self.credits_body.append(&label);
    }

    fn credits_row(&self, name: &str, value: &str) {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let name_label = wrapping_label(&["caption", "dim-label"]);
        name_label.set_text(name);
        let value_label = wrapping_label(&["body"]);
        value_label.set_text(value);
        value_label.set_selectable(true);
        row.append(&name_label);
        row.append(&value_label);
        self.credits_body.append(&row);
    }

    fn credits_button(&self, label: &str, on_click: impl Fn(&gtk::Button) + 'static) {
        let button = gtk::Button::with_label(label);
        button.add_css_class("pill");
        button.add_css_class("small");
        button.set_halign(gtk::Align::Start);
        button.set_margin_top(4);
        button.connect_clicked(on_click);
        self.credits_body.append(&button);
    }

    /// A summary of who made the track: the full credits, the song's story
    /// and the artist are in the About tab, which a click on the card opens.
    fn show_credits(&self, track: &Track, info: &GeniusTrackInfo) {
        self.clear_credits();
        let mut any = false;
        let artists: Vec<String> = track.artists.iter().map(|a| a.name.clone()).collect();
        if !artists.is_empty() {
            // translators: Credits row in the Now Playing panel: the track's artists.
            self.credits_row(&gettext("Performed by"), &artists.join(", "));
        }
        for (name, values) in [
            (gettext("Producer(s)"), &info.producers),
            (gettext("Writer(s)"), &info.writers),
        ] {
            if !values.is_empty() {
                self.credits_row(&name, &values.join(", "));
                any = true;
            }
        }
        if !any {
            let label = wrapping_label(&["dim-label", "caption"]);
            label.set_text(&gettext(
                "No detailed credits were found on Genius for this track.",
            ));
            self.credits_body.append(&label);
        }
    }
}

/// The user's plays of the track, its album and its artist, as columns.
fn playcount_columns(counts: &canto_api::scrobbler::Playcounts) -> Option<gtk::Box> {
    number_columns(vec![
        (counts.track, gettext("this song")),
        (counts.album, gettext("this album")),
        (counts.artist, gettext("this artist")),
    ])
}

pub struct NowPanel {
    widgets: Rc<Widgets>,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    settings: gio::Settings,
    clock: Rc<Clock>,
    lyrics: Rc<RefCell<Option<Lyrics>>>,
    /// The track the lyrics and credits were loaded for
    loaded_for: Rc<RefCell<Option<String>>>,
}

impl NowPanel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let panel = Self {
            widgets: Rc::new(Widgets::build()),
            app_model,
            dispatcher,
            settings: gio::Settings::new(SETTINGS),
            clock: Rc::new(Clock::new(400)),
            lyrics: Rc::new(RefCell::new(None)),
            loaded_for: Rc::new(RefCell::new(None)),
        };
        panel.wire();
        panel.update_track();
        panel
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.widgets.stack.upcast_ref()
    }

    fn wire(&self) {
        let w = &self.widgets;
        for label in [&w.title, &w.artists] {
            label.connect_activate_link(clone!(
                #[weak(rename_to = app_model)]
                self.app_model,
                #[strong(rename_to = dispatcher)]
                self.dispatcher,
                #[upgrade_or]
                glib::Propagation::Proceed,
                move |_, uri| {
                    follow_track_link(&app_model, &dispatcher, uri);
                    glib::Propagation::Stop
                }
            ));
        }
        w.like.connect_clicked(clone!(
            #[weak(rename_to = app_model)]
            self.app_model,
            #[strong(rename_to = dispatcher)]
            self.dispatcher,
            move |_| toggle_current_track_like(&app_model, &dispatcher)
        ));

        // Lyrics and credits load only while the panel is on screen.
        let refresh = clone!(
            #[weak(rename_to = widgets)]
            self.widgets,
            #[weak(rename_to = app_model)]
            self.app_model,
            #[weak(rename_to = settings)]
            self.settings,
            #[weak(rename_to = clock)]
            self.clock,
            #[weak(rename_to = lyrics)]
            self.lyrics,
            #[weak(rename_to = loaded_for)]
            self.loaded_for,
            move || {
                load_extras(&widgets, &app_model, &settings, &lyrics, &loaded_for);
                sync_clock(&widgets, &app_model, &settings, &clock, &lyrics);
            }
        );
        let refresh = Rc::new(refresh);
        let on_map = Rc::clone(&refresh);
        w.scroller.connect_map(move |_| on_map());
        // The Genius lyrics switch: the preview follows the lyrics page at
        // once on screen, or the next time the panel shows
        let on_switch = Rc::clone(&refresh);
        self.settings.connect_changed(
            Some(LYRICS_GENIUS_FALLBACK),
            clone!(
                #[weak(rename_to = widgets)]
                self.widgets,
                #[weak(rename_to = loaded_for)]
                self.loaded_for,
                move |_, _| {
                    *loaded_for.borrow_mut() = None;
                    if widgets.scroller.is_mapped() {
                        on_switch();
                    }
                }
            ),
        );
        w.scroller.connect_unmap(clone!(
            #[weak(rename_to = clock)]
            self.clock,
            move |_| clock.stop()
        ));
    }

    fn is_shown(&self) -> bool {
        self.widgets.scroller.is_mapped()
    }

    fn update_track(&self) {
        let w = &self.widgets;
        let song = self.app_model.get_state().playback.current_song();
        let Some(song) = song else {
            self.clock.stop();
            *self.lyrics.borrow_mut() = None;
            *self.loaded_for.borrow_mut() = None;
            w.stack.set_visible_child_name("empty");
            return;
        };
        w.stack.set_visible_child_name("content");
        w.title
            .set_markup(&title_markup(&song.title, song.album.is_some()));
        w.artists.set_markup(&artists_markup(&song));
        self.update_like();
        self.load_cover(&song);
        self.update_next();
        if self.is_shown() {
            load_extras(
                w,
                &self.app_model,
                &self.settings,
                &self.lyrics,
                &self.loaded_for,
            );
            sync_clock(
                w,
                &self.app_model,
                &self.settings,
                &self.clock,
                &self.lyrics,
            );
        }
    }

    fn update_like(&self) {
        show_like_state(&self.widgets.like, current_track_liked(&self.app_model));
    }

    fn load_cover(&self, song: &Track) {
        let picture = self.widgets.cover.clone();
        load_picture(
            &self.app_model,
            &picture,
            song.art.best_for_width(COVER_PX as u32),
            COVER_PX,
        );
    }

    fn update_next(&self) {
        let w = &self.widgets;
        let next = self.app_model.get_state().playback.next_song();
        let Some(next) = next else {
            w.next_card.set_visible(false);
            return;
        };
        w.next_card.set_visible(true);
        w.next_title.set_text(&next.title);
        w.next_artist.set_text(&next.artists_name());
        load_image(
            &self.app_model,
            &w.next_cover,
            next.art.best_for_width(NEXT_COVER_PX as u32),
            NEXT_COVER_PX,
        );
    }
}

fn load_image(app_model: &AppModel, image: &gtk::Image, url: Option<&str>, px: i32) {
    image.set_paintable(None::<&gtk::gdk::Paintable>);
    let Some(url) = url.filter(|url| !is_resource_url(url)).map(str::to_owned) else {
        return;
    };
    let api = app_model.api();
    let weak = image.downgrade();
    let tag = load::transport();
    glib::MainContext::default().spawn_local(async move {
        let texture = api.load_image(&url, px, px, tag).await;
        if let (Some(image), Some(texture)) = (weak.upgrade(), texture) {
            image.set_paintable(Some(&texture));
        }
    });
}

fn load_picture(app_model: &AppModel, picture: &gtk::Picture, url: Option<&str>, px: i32) {
    picture.set_paintable(None::<&gtk::gdk::Paintable>);
    let Some(url) = url.filter(|url| !is_resource_url(url)).map(str::to_owned) else {
        return;
    };
    let api = app_model.api();
    let weak = picture.downgrade();
    let tag = load::transport();
    glib::MainContext::default().spawn_local(async move {
        let texture = api.load_image(&url, px, px, tag).await;
        if let (Some(picture), Some(texture)) = (weak.upgrade(), texture) {
            picture.set_paintable(Some(&texture));
        }
    });
}

/// Loads the lyrics and the credits of the current track, once per track.
fn load_extras(
    w: &Rc<Widgets>,
    app_model: &Rc<AppModel>,
    settings: &gio::Settings,
    lyrics: &Rc<RefCell<Option<Lyrics>>>,
    loaded_for: &Rc<RefCell<Option<String>>>,
) {
    let Some(track) = app_model.get_state().playback.current_song() else {
        return;
    };
    let track_id = track.rri.id.clone();
    if loaded_for.borrow().as_deref() == Some(track_id.as_str()) {
        return;
    }
    *loaded_for.borrow_mut() = Some(track_id.clone());
    *lyrics.borrow_mut() = None;
    w.lyrics_card.set_visible(false);

    let api = app_model.api();
    let key = track
        .rri
        .uri
        .clone()
        .unwrap_or_else(|| track.rri.id.clone());
    let title = track.title.clone();
    let artist = track.artists_name();
    let album = track.album.as_ref().map(|a| a.name.clone());
    let duration_secs = track.duration_ms / 1000;
    let weak_w = Rc::downgrade(w);
    let weak_model = Rc::downgrade(app_model);
    let weak_lyrics = Rc::downgrade(lyrics);
    let weak_loaded = Rc::downgrade(loaded_for);
    let settings_l = settings.clone();
    let lyrics_api = api.clone();
    let lyrics_track_id = track_id.clone();
    // The same look-up as the lyrics page, so both show the same lyrics
    let genius = crate::app::components::genius_fallback(settings);
    let asked_genius = genius.is_some();
    glib::spawn_future_local(async move {
        let fetched = lyrics_api
            .fetch_lyrics(
                &key,
                &title,
                &artist,
                album.as_deref(),
                Some(duration_secs),
                genius.as_deref(),
            )
            .await
            .map(|found| found.lyrics);
        let (Some(w), Some(model), Some(cell), Some(loaded)) = (
            weak_w.upgrade(),
            weak_model.upgrade(),
            weak_lyrics.upgrade(),
            weak_loaded.upgrade(),
        ) else {
            return;
        };
        if loaded.borrow().as_deref() != Some(lyrics_track_id.as_str()) {
            return;
        }
        // The Genius switch flipped meanwhile: its own look-up shows instead
        if settings_l.boolean(LYRICS_GENIUS_FALLBACK) != asked_genius {
            return;
        }
        *cell.borrow_mut() = fetched;
        render_lyrics(&w, &model, &settings_l, &cell);
    });

    load_playcounts(w, settings, &track, &track_id, loaded_for);

    let token = settings.string(GENIUS_ACCESS_TOKEN).trim().to_string();
    if token.is_empty() {
        w.credits_text(&gettext(
            "Add a Genius token to see who produced and wrote this track.",
        ));
        w.credits_button(&gettext("Set Up Genius"), |button| {
            let _ = button.activate_action("win.preferences-integrations", None);
        });
        return;
    }
    // translators: Shown while the credits of the current track load.
    w.credits_text(&gettext("Loading credits…"));
    let weak_w = Rc::downgrade(w);
    let weak_loaded = Rc::downgrade(loaded_for);
    let artist = track.artists_name();
    let title = track.title.clone();
    glib::spawn_future_local(async move {
        let result = api
            .fetch_genius_track_info(&track_id, &artist, &title, &token)
            .await;
        let (Some(w), Some(loaded)) = (weak_w.upgrade(), weak_loaded.upgrade()) else {
            return;
        };
        if loaded.borrow().as_deref() != Some(track_id.as_str()) {
            return;
        }
        match result {
            Ok(info) => w.show_credits(&track, &info),
            Err(canto_api::DomainError::AuthExpired) => {
                // A revoked or mistyped token: say so, and lead to the fix
                w.credits_text(&gettext(
                    "Genius didn't accept your token. Paste a new one in Settings to see the credits again.",
                ));
                w.credits_button(&gettext("Set Up Genius"), |button| {
                    let _ = button.activate_action("win.preferences-integrations", None);
                });
            }
            Err(_) => {
                w.credits_text(&gettext("Couldn't load the credits from Genius."));
                let url = api.genius_search_url(&artist, &title);
                w.credits_button(&gettext("Search on Genius"), move |button| {
                    crate::app::components::open_external_url(button, &url)
                });
            }
        }
    });
}

/// "1.234" style thousands, in the interface's language.
fn format_count(count: u64) -> String {
    let digits = count.to_string();
    let english = glib::language_names()
        .first()
        .is_none_or(|l| l.starts_with("en") || l.as_str() == "C");
    let separator = if english { ',' } else { '.' };
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(separator);
        }
        out.push(c);
    }
    out
}

/// How many times the user played this track, album and artist, from Last.fm.
fn load_playcounts(
    w: &Rc<Widgets>,
    settings: &gio::Settings,
    track: &Track,
    track_id: &str,
    loaded_for: &Rc<RefCell<Option<String>>>,
) {
    w.stats_card.set_visible(false);
    let api_key = settings
        .string(canto_config::settings::LASTFM_API_KEY)
        .to_string();
    let username = settings
        .string(canto_config::settings::LASTFM_USERNAME)
        .to_string();
    if api_key.trim().is_empty() || username.trim().is_empty() {
        return;
    }
    let artist = track
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| track.artists_name());
    let title = track.title.clone();
    let album = track.album.as_ref().map(|a| a.name.clone());
    let track_id = track_id.to_string();
    let weak_w = Rc::downgrade(w);
    let weak_loaded = Rc::downgrade(loaded_for);
    glib::spawn_future_local(async move {
        let result = canto_api::scrobbler::LastFmClient::user_playcounts(
            &api_key,
            &username,
            &artist,
            &title,
            album.as_deref(),
        )
        .await;
        let (Some(w), Some(loaded)) = (weak_w.upgrade(), weak_loaded.upgrade()) else {
            return;
        };
        if loaded.borrow().as_deref() != Some(track_id.as_str()) {
            return;
        }
        match result {
            Ok(counts) => w.show_playcounts(&counts),
            Err(e) => debug!("Last.fm play counts failed: {e}"),
        }
    });
}

fn render_lyrics(
    w: &Widgets,
    app_model: &AppModel,
    settings: &gio::Settings,
    lyrics: &RefCell<Option<Lyrics>>,
) {
    let lyrics = lyrics.borrow();
    let Some(lyrics) = lyrics.as_ref() else {
        w.lyrics_card.set_visible(false);
        return;
    };
    let pos = adjusted_position(
        app_model.get_state().playback.seek_position(),
        settings.int(LYRICS_OFFSET_MS),
    );
    let (lines, current) = preview_window(lyrics, pos);
    if lines.is_empty() {
        w.lyrics_card.set_visible(false);
        return;
    }
    w.lyrics_card.set_visible(true);
    for (i, label) in w.lyrics_lines.iter().enumerate() {
        match lines.get(i) {
            Some(text) => {
                label.set_text(text);
                label.set_visible(true);
            }
            None => label.set_visible(false),
        }
        if current == Some(i) {
            label.add_css_class("now-panel__lyric-line--active");
        } else {
            label.remove_css_class("now-panel__lyric-line--active");
        }
    }
}

/// Keeps the lyrics preview moving while synced lyrics play on screen.
fn sync_clock(
    w: &Rc<Widgets>,
    app_model: &Rc<AppModel>,
    settings: &gio::Settings,
    clock: &Rc<Clock>,
    lyrics: &Rc<RefCell<Option<Lyrics>>>,
) {
    render_lyrics(w, app_model, settings, lyrics);
    let playing = app_model.get_state().playback.is_playing();
    if !(playing && w.scroller.is_mapped()) {
        clock.stop();
        return;
    }
    let weak_w = Rc::downgrade(w);
    let weak_model = Rc::downgrade(app_model);
    let weak_lyrics = Rc::downgrade(lyrics);
    let settings = settings.clone();
    clock.start(move || {
        let (Some(w), Some(model), Some(lyrics)) = (
            weak_w.upgrade(),
            weak_model.upgrade(),
            weak_lyrics.upgrade(),
        ) else {
            return;
        };
        if lyrics.borrow().as_ref().is_some_and(Lyrics::is_synced) {
            render_lyrics(&w, &model, &settings, &lyrics);
        }
    });
}

impl Drop for NowPanel {
    fn drop(&mut self) {
        self.clock.stop();
    }
}

impl EventListener for NowPanel {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::PlaybackStopped,
            ) => self.update_track(),
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaylistChanged
                | PlaybackEvent::ShuffleChanged(_)
                | PlaybackEvent::ShuffleSynced(_)
                | PlaybackEvent::RepeatModeChanged(_)
                | PlaybackEvent::RepeatSynced(_),
            ) => self.update_next(),
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlayStateSynced(_)
                | PlaybackEvent::TrackSeeked(_)
                | PlaybackEvent::SeekSynced(_),
            ) => sync_clock(
                &self.widgets,
                &self.app_model,
                &self.settings,
                &self.clock,
                &self.lyrics,
            ),
            AppEvent::BrowserEvent(BrowserEvent::SavedTracksUpdated) => self.update_like(),
            _ => {}
        }
    }
}

/// The panel's widgets alone, for the GTK smoke test.
#[cfg(test)]
pub(crate) fn widgets_for_tests() -> gtk::Widget {
    Widgets::build().stack.upcast()
}

#[cfg(test)]
pub(crate) use about::sample_for_tests as about_sample_for_tests;
#[cfg(test)]
pub(crate) use stats::sample_for_tests as stats_sample_for_tests;

/// The panel filled with a sample track, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests(cover: &gtk::gdk::Texture) -> gtk::Widget {
    let w = Widgets::build();
    w.stack.set_visible_child_name("content");
    w.cover.set_paintable(Some(cover));
    w.title.set_markup(&title_markup("TODOS ESSES MANOS", true));
    w.artists.set_markup(
        "<a href=\"artist:1\">Niink</a>, <a href=\"artist:2\">Jotapê</a>, <a href=\"artist:3\">Supernova Ent</a>",
    );
    show_like_state(&w.like, Some(true));
    let lines = [
        "Todos esses manos querem ser",
        "Mas ninguém quer pagar o preço",
        "Eu vim do nada e tô aqui",
        "Olha onde a gente chegou",
    ];
    w.lyrics_card.set_visible(true);
    for (label, text) in w.lyrics_lines.iter().zip(lines) {
        label.set_text(text);
    }
    w.lyrics_lines[0].add_css_class("now-panel__lyric-line--active");
    w.next_card.set_visible(true);
    w.next_cover.set_paintable(Some(cover));
    w.next_title.set_text("MARIA EU PEQUEI");
    w.next_artist.set_text("Niink, Supernova Ent");
    w.credits_text("Add a Genius token to see who produced and wrote this track.");
    w.credits_button("Set Up Genius", |_| {});
    w.stack.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;
    use canto_api::LyricLine;

    fn synced(lines: &[(u64, &str)]) -> Lyrics {
        Lyrics::Synced(
            lines
                .iter()
                .map(|(t, text)| LyricLine {
                    start_time_ms: *t,
                    text: text.to_string(),
                    ..Default::default()
                })
                .collect(),
        )
    }

    #[test]
    fn preview_starts_at_the_line_being_sung_and_skips_breaks() {
        let lyrics = synced(&[(0, "one"), (1000, ""), (2000, "two"), (3000, "three")]);
        let (lines, current) = preview_window(&lyrics, 2500);
        assert_eq!(lines, vec!["two", "three"]);
        assert_eq!(current, Some(0));
        let (lines, current) = preview_window(&lyrics, 1500);
        assert_eq!(lines, vec!["two", "three"]);
        assert_eq!(current, None, "an instrumental break highlights nothing");
    }

    #[test]
    fn plain_lyrics_preview_the_first_lines() {
        let lyrics = Lyrics::Plain(vec!["a".into(), " ".into(), "b".into()]);
        assert_eq!(
            preview_window(&lyrics, 0),
            (vec!["a".into(), "b".into()], None)
        );
    }

    #[test]
    fn links_point_to_album_and_artists() {
        assert!(matches!(parse_track_link("album"), Some(TrackLink::Album)));
        assert!(matches!(
            parse_track_link("artist:42"),
            Some(TrackLink::Artist(id)) if id == "42"
        ));
        assert!(parse_track_link("artist:").is_none());
        assert!(parse_track_link("https://example.com").is_none());
        assert_eq!(
            title_markup("Rock & Roll", true),
            format!(
                "<a href=\"album\" title=\"{}\">Rock &amp; Roll</a>",
                gettext("Go to Album")
            )
        );
        assert_eq!(title_markup("<x>", false), "&lt;x&gt;");
    }
}
