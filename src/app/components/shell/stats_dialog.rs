//! "Your Stats": the user's most played artists, tracks and albums from
//! Last.fm, for the last week, month, year or all time.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use canto_api::scrobbler::{LastFmClient, ListeningStats, StatsPeriod, TopItem};
use gettextrs::gettext;
use gtk::prelude::*;
use gtk::{gio, glib};
use libadwaita::prelude::*;

use crate::app::models::Track;
use crate::app::state::{load_context, start_actions, BrowserAction, PlaybackAction, Route};
use crate::app::{AppAction, AppModel, Dispatcher, SongsSource};

/// How many of each list it shows.
const TOP: usize = 10;
const COVER_PX: i32 = 44;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Artist,
    Track,
    Album,
}

struct Stats {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    dialog: libadwaita::Dialog,
    body: gtk::Box,
    stack: gtk::Stack,
    status: libadwaita::StatusPage,
    period: Cell<StatsPeriod>,
    /// Bumped on each load, so a slow answer for another period is dropped
    generation: Cell<u64>,
}

/// Opens the stats over `window`. Without a Last.fm account it says how to
/// link one.
pub fn open(
    window: &libadwaita::ApplicationWindow,
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
) {
    let dialog = libadwaita::Dialog::builder()
        // translators: Title of the window with the user's Last.fm listening stats.
        .title(gettext("Your Stats"))
        .content_width(560)
        .content_height(680)
        .build();
    dialog.add_css_class("stats-dialog");

    let toolbar = libadwaita::ToolbarView::new();
    let header = libadwaita::HeaderBar::new();
    toolbar.add_top_bar(&header);

    let periods = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    periods.set_halign(gtk::Align::Center);
    periods.set_margin_top(4);
    periods.set_margin_bottom(8);
    periods.add_css_class("stats-periods");
    toolbar.add_top_bar(&periods);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 18);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_top(8);
    body.set_margin_bottom(24);
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&body)
        .vexpand(true)
        .build();

    let spinner = libadwaita::Spinner::new();
    spinner.set_size_request(32, 32);
    spinner.set_halign(gtk::Align::Center);
    spinner.set_valign(gtk::Align::Center);

    let status = libadwaita::StatusPage::builder()
        .icon_name("canto-lastfm-symbolic")
        .build();

    let stack = gtk::Stack::new();
    stack.add_named(&spinner, Some("loading"));
    stack.add_named(&scroller, Some("stats"));
    stack.add_named(&status, Some("status"));
    toolbar.set_content(Some(&stack));
    dialog.set_child(Some(&toolbar));

    let stats = Rc::new(Stats {
        app_model,
        dispatcher,
        dialog: dialog.clone(),
        body,
        stack,
        status,
        period: Cell::new(StatsPeriod::Month),
        generation: Cell::new(0),
    });

    let mut group: Option<gtk::ToggleButton> = None;
    for period in StatsPeriod::ALL {
        let button = gtk::ToggleButton::with_label(&period_label(period));
        button.add_css_class("pill");
        button.add_css_class("stats-period");
        if let Some(first) = &group {
            button.set_group(Some(first));
        } else {
            group = Some(button.clone());
        }
        button.set_active(period == stats.period.get());
        let weak = Rc::downgrade(&stats);
        button.connect_toggled(move |button| {
            if let (true, Some(stats)) = (button.is_active(), weak.upgrade()) {
                stats.period.set(period);
                stats.load();
            }
        });
        periods.append(&button);
    }

    stats.load();
    // The dialog keeps the stats alive while open
    dialog.connect_closed(move |_| {
        let _ = &stats;
    });
    dialog.present(Some(window));
}

/// "1.234" style thousands, in the interface's language.
fn format_thousands(count: u64) -> String {
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

fn period_label(period: StatsPeriod) -> String {
    match period {
        // translators: Stats period: the last 7 days.
        StatsPeriod::Week => gettext("Last 7 Days"),
        // translators: Stats period: the last 30 days.
        StatsPeriod::Month => gettext("Last Month"),
        // translators: Stats period: the last 12 months.
        StatsPeriod::Year => gettext("Last Year"),
        // translators: Stats period: since the account was created.
        StatsPeriod::Overall => gettext("All Time"),
    }
}

impl Stats {
    fn load(self: &Rc<Self>) {
        let settings = gio::Settings::new(canto_config::settings::SETTINGS);
        let api_key = settings
            .string(canto_config::settings::LASTFM_API_KEY)
            .to_string();
        let username = settings
            .string(canto_config::settings::LASTFM_USERNAME)
            .to_string();
        if api_key.trim().is_empty() || username.trim().is_empty() {
            self.status
                .set_title(&gettext("Link Last.fm to See Your Stats"));
            self.status.set_description(Some(&gettext(
                "Add your Last.fm username and API key in Preferences → Integrations.",
            )));
            self.stack.set_visible_child_name("status");
            return;
        }
        let generation = self.generation.get().wrapping_add(1);
        self.generation.set(generation);
        self.stack.set_visible_child_name("loading");
        let period = self.period.get();
        let weak = Rc::downgrade(self);
        glib::spawn_future_local(async move {
            let result = LastFmClient::listening_stats(&api_key, &username, period, TOP).await;
            let Some(stats) = weak.upgrade() else {
                return;
            };
            if stats.generation.get() != generation {
                return;
            }
            match result {
                Ok(listening) => stats.show(&listening),
                Err(err) => {
                    log::warn!("Last.fm stats failed: {err}");
                    stats.status.set_title(&gettext("Couldn't Load Your Stats"));
                    stats.status.set_description(Some(&gettext(
                        "Last.fm didn't answer. Try again in a moment.",
                    )));
                    stats.stack.set_visible_child_name("status");
                }
            }
        });
    }

    fn show(self: &Rc<Self>, listening: &ListeningStats) {
        while let Some(child) = self.body.first_child() {
            self.body.remove(&child);
        }
        let nothing = listening.artists.is_empty()
            && listening.tracks.is_empty()
            && listening.albums.is_empty();
        if nothing {
            self.status
                .set_title(&gettext("Nothing Played in This Period"));
            self.status.set_description(None);
            self.stack.set_visible_child_name("status");
            return;
        }

        // Totals: how many different artists, tracks and albums
        let totals = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        totals.set_homogeneous(true);
        totals.add_css_class("stats-totals");
        for (count, label) in [
            // translators: Under a number in the stats: how many different artists.
            (listening.artist_count, gettext("artists")),
            // translators: Under a number in the stats: how many different tracks.
            (listening.track_count, gettext("tracks")),
            // translators: Under a number in the stats: how many different albums.
            (listening.album_count, gettext("albums")),
        ] {
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
            cell.add_css_class("stats-total");
            let number = gtk::Label::new(Some(&format_thousands(count.unwrap_or(0))));
            number.add_css_class("stats-total__number");
            let caption = gtk::Label::new(Some(&label));
            caption.add_css_class("dim-label");
            caption.add_css_class("caption");
            cell.append(&number);
            cell.append(&caption);
            totals.append(&cell);
        }
        self.body.append(&totals);

        // translators: Heading of a list in the stats.
        self.section(&gettext("Top Artists"), &listening.artists, Kind::Artist);
        // translators: Heading of a list in the stats.
        self.section(&gettext("Top Tracks"), &listening.tracks, Kind::Track);
        // translators: Heading of a list in the stats.
        self.section(&gettext("Top Albums"), &listening.albums, Kind::Album);
        self.stack.set_visible_child_name("stats");
    }

    fn section(self: &Rc<Self>, title: &str, items: &[TopItem], kind: Kind) {
        if items.is_empty() {
            return;
        }
        let heading = gtk::Label::new(Some(title));
        heading.set_xalign(0.0);
        heading.add_css_class("title-4");
        self.body.append(&heading);

        let list = gtk::ListBox::new();
        list.set_selection_mode(gtk::SelectionMode::None);
        list.add_css_class("boxed-list");
        list.add_css_class("stats-list");
        let most = items.iter().map(|i| i.plays).max().unwrap_or(1).max(1);
        for (rank, item) in items.iter().enumerate() {
            list.append(&self.row(rank + 1, item, kind, most));
        }
        self.body.append(&list);
    }

    fn row(self: &Rc<Self>, rank: usize, item: &TopItem, kind: Kind, most: u64) -> gtk::ListBoxRow {
        let row = gtk::ListBoxRow::new();
        row.set_activatable(true);
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        hbox.set_margin_top(6);
        hbox.set_margin_bottom(6);
        hbox.set_margin_start(10);
        hbox.set_margin_end(12);

        let number = gtk::Label::new(Some(&rank.to_string()));
        number.set_width_chars(2);
        number.set_xalign(1.0);
        number.add_css_class("dim-label");
        number.add_css_class("numeric");
        hbox.append(&number);

        let icon = match kind {
            Kind::Artist => "avatar-default-symbolic",
            Kind::Track => "audio-x-generic-symbolic",
            Kind::Album => "media-optical-cd-audio-symbolic",
        };
        let cover = gtk::Image::from_icon_name(icon);
        cover.set_size_request(COVER_PX, COVER_PX);
        cover.set_overflow(gtk::Overflow::Hidden);
        cover.add_css_class("stats-cover");
        if kind == Kind::Artist {
            cover.add_css_class("stats-cover--round");
        }
        hbox.append(&cover);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        text.set_valign(gtk::Align::Center);
        let name = gtk::Label::new(Some(&item.name));
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&name);
        if let Some(artist) = item.artist.as_deref().filter(|_| kind != Kind::Artist) {
            let by = gtk::Label::new(Some(artist));
            by.set_xalign(0.0);
            by.set_ellipsize(gtk::pango::EllipsizeMode::End);
            by.add_css_class("dim-label");
            by.add_css_class("caption");
            text.append(&by);
        }
        // A thin bar: how this compares with the most played
        let bar = gtk::LevelBar::for_interval(0.0, most as f64);
        bar.set_value(item.plays as f64);
        bar.add_css_class("stats-bar");
        bar.set_margin_top(3);
        text.append(&bar);
        hbox.append(&text);

        // translators: How many times the user played something, e.g. "109 plays".
        let plays = gtk::Label::new(Some(
            &gettext("{} plays").replace("{}", &format_thousands(item.plays)),
        ));
        plays.add_css_class("dim-label");
        plays.add_css_class("caption");
        plays.add_css_class("numeric");
        hbox.append(&plays);
        row.set_child(Some(&hbox));

        // Last.fm only has album pictures; the rest come from Spotify
        let found: Rc<RefCell<Option<Found>>> = Default::default();
        if let Some(url) = item.image.clone() {
            self.load_picture(&cover, url);
        }
        self.find_on_spotify(item, kind, &cover, &found);

        let click = gtk::GestureClick::new();
        let weak = Rc::downgrade(self);
        let item = item.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(stats) = weak.upgrade() {
                stats.open_item(&item, kind, &found);
            }
        });
        row.add_controller(click);
        row
    }

    fn load_picture(&self, cover: &gtk::Image, url: String) {
        let api = self.app_model.api();
        let weak = cover.downgrade();
        let px = COVER_PX * cover.scale_factor().max(2);
        let tag = canto_api::Load::new(
            canto_api::LoadPriority::Visible,
            canto_api::BACKGROUND_EPOCH,
        );
        glib::spawn_future_local(async move {
            let texture = api.load_image(&url, px, px, tag).await;
            if let (Some(cover), Some(texture)) = (weak.upgrade(), texture) {
                cover.set_paintable(Some(&texture));
            }
        });
    }

    /// Looks the item up on Spotify: its picture (when Last.fm has none) and
    /// what a click opens.
    fn find_on_spotify(
        self: &Rc<Self>,
        item: &TopItem,
        kind: Kind,
        cover: &gtk::Image,
        found: &Rc<RefCell<Option<Found>>>,
    ) {
        let api = self.app_model.api();
        let query = spotify_query(item, kind);
        let needs_picture = item.image.is_none();
        let weak_self = Rc::downgrade(self);
        let weak_cover = cover.downgrade();
        let found = Rc::clone(found);
        let tag = canto_api::Load::new(
            canto_api::LoadPriority::Visible,
            canto_api::BACKGROUND_EPOCH,
        );
        glib::spawn_future_local(async move {
            let Ok(results) = api.search(&query, 0, 5, tag).await else {
                return;
            };
            let hit = match kind {
                Kind::Artist => results.artists.items.first().map(|a| {
                    (
                        Found::Artist(a.rri.id.clone()),
                        a.art.best_for_width(96).map(str::to_owned),
                    )
                }),
                Kind::Album => results.albums.items.first().map(|a| {
                    (
                        Found::Album(a.rri.id.clone()),
                        a.art.best_for_width(96).map(str::to_owned),
                    )
                }),
                Kind::Track => results.tracks.items.first().map(|t| {
                    (
                        Found::Track(Box::new(t.clone())),
                        t.art.best_for_width(96).map(str::to_owned),
                    )
                }),
            };
            let Some((target, picture)) = hit else {
                return;
            };
            *found.borrow_mut() = Some(target);
            if let (true, Some(url), Some(stats), Some(cover)) = (
                needs_picture,
                picture,
                weak_self.upgrade(),
                weak_cover.upgrade(),
            ) {
                stats.load_picture(&cover, url);
            }
        });
    }

    /// Opens the artist or album page, or plays the track.
    fn open_item(&self, item: &TopItem, kind: Kind, found: &Rc<RefCell<Option<Found>>>) {
        let Some(target) = found.borrow().clone() else {
            // Not found on Spotify (yet): search for it
            self.dispatcher
                .dispatch(BrowserAction::Navigate(Route::Search).into());
            self.dispatcher
                .dispatch(BrowserAction::Search(spotify_query(item, kind)).into());
            self.dialog.close();
            return;
        };
        match target {
            Found::Artist(id) => self.dispatcher.dispatch(AppAction::ViewArtist(id)),
            Found::Album(id) => self.dispatcher.dispatch(AppAction::ViewAlbum(id)),
            Found::Track(track) => {
                let track = *track;
                let source = SongsSource::Search(track.title.clone());
                let title = Some(track.title.clone());
                let load = PlaybackAction::LoadContextSongs(source.clone(), vec![track]);
                for action in start_actions(false, load_context(source, title, load)) {
                    self.dispatcher.dispatch(action);
                }
            }
        }
        self.dialog.close();
    }
}

#[derive(Clone)]
enum Found {
    Artist(String),
    Album(String),
    Track(Box<Track>),
}

fn spotify_query(item: &TopItem, kind: Kind) -> String {
    match (kind, item.artist.as_deref()) {
        (Kind::Artist, _) | (_, None) => item.name.clone(),
        (_, Some(artist)) => format!("{} {}", item.name, artist),
    }
}
