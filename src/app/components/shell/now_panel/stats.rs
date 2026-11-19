//! The side panel's Stats tab: what Last.fm knows about the playing song,
//! its album and artist, and the user's own plays. Read-only.

use gettextrs::{gettext, ngettext};
use gio::prelude::SettingsExt;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use super::info_page::{
    action_row, caption, chips, flow, folding_text, link_button, pill_button, setup_button,
    InfoPage,
};
use super::{card, format_count, heading, number_columns, playcount_columns, HEART_FILLED_ICON};
use crate::app::components::EventListener;
use crate::app::components::{dispatch_api_read_many, play_in_album_actions};
use crate::app::models::SearchType;
use crate::app::models::TrackExt;
use crate::app::state::{BrowserAction, PlaybackEvent, Route};
use crate::app::{AppEvent, AppModel, Dispatcher};
use canto_api::scrobbler::{LastFmClient, TrackInsights};
use canto_config::settings::{LASTFM_API_KEY, LASTFM_USERNAME, SETTINGS};

const ICON: &str = "canto-stats-symbolic";

pub struct StatsPanel {
    page: Rc<InfoPage>,
    app_model: Rc<AppModel>,
    settings: gio::Settings,
    on_search: OnPick,
    /// The track shown (or loading)
    loaded_for: Rc<RefCell<Option<String>>>,
}

impl StatsPanel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let api_model = Rc::clone(&app_model);
        let on_search: OnPick = Rc::new(move |pick: Pick| open_pick(&api_model, &dispatcher, pick));
        let panel = Self {
            page: Rc::new(InfoPage::new("stats-panel")),
            app_model,
            settings: gio::Settings::new(SETTINGS),
            on_search,
            loaded_for: Rc::new(RefCell::new(None)),
        };
        // Loads only while the tab is on screen
        let page = Rc::downgrade(&panel.page);
        let model = Rc::downgrade(&panel.app_model);
        let loaded_for = Rc::downgrade(&panel.loaded_for);
        let settings = panel.settings.clone();
        let on_search = Rc::downgrade(&panel.on_search);
        panel.page.stack.connect_map(move |_| {
            if let (Some(page), Some(model), Some(loaded_for), Some(on_search)) = (
                page.upgrade(),
                model.upgrade(),
                loaded_for.upgrade(),
                on_search.upgrade(),
            ) {
                load(&page, &model, &settings, &loaded_for, &on_search);
            }
        });
        panel.update();
        panel
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.page.widget()
    }

    fn update(&self) {
        if self.app_model.get_state().playback.current_song().is_none() {
            *self.loaded_for.borrow_mut() = None;
            self.page.show_nothing_playing();
        } else if self.page.is_shown() {
            load(
                &self.page,
                &self.app_model,
                &self.settings,
                &self.loaded_for,
                &self.on_search,
            );
        }
    }
}

impl EventListener for StatsPanel {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::TrackSynced
            | PlaybackEvent::PlaybackStopped,
        ) = event
        {
            self.update();
        }
    }
}

/// Loads the playing song from Last.fm, once per song (answers are kept a
/// few minutes, shared with the Now Playing panel's Your Plays card).
fn load(
    page: &Rc<InfoPage>,
    app_model: &Rc<AppModel>,
    settings: &gio::Settings,
    loaded_for: &Rc<RefCell<Option<String>>>,
    on_search: &OnPick,
) {
    let Some(track) = app_model.get_state().playback.current_song() else {
        page.show_nothing_playing();
        return;
    };
    let track_id = track.rri.id.clone();
    if loaded_for.borrow().as_deref() == Some(track_id.as_str()) {
        return;
    }
    let api_key = settings.string(LASTFM_API_KEY).trim().to_string();
    let username = settings.string(LASTFM_USERNAME).trim().to_string();
    if api_key.is_empty() {
        show_not_set_up(page);
        return;
    }
    *loaded_for.borrow_mut() = Some(track_id.clone());
    page.show_loading();

    // The main artist, as the Your Plays card asks
    let artist = track
        .artists
        .first()
        .map(|a| a.name.clone())
        .unwrap_or_else(|| track.artists_name());
    let title = track.title.clone();
    let album = track.album.as_ref().map(|a| a.name.clone());
    let weak_page = Rc::downgrade(page);
    let weak_loaded = Rc::downgrade(loaded_for);
    let on_search = Rc::clone(on_search);
    glib::spawn_future_local(async move {
        let result =
            LastFmClient::track_insights(&api_key, &username, &artist, &title, album.as_deref())
                .await;
        let (Some(page), Some(loaded)) = (weak_page.upgrade(), weak_loaded.upgrade()) else {
            return;
        };
        if loaded.borrow().as_deref() != Some(track_id.as_str()) {
            return;
        }
        match result {
            Ok(insights) if !insights.is_empty() => {
                render(&page, &artist, &insights, !username.is_empty(), &on_search)
            }
            Ok(_) | Err(canto_api::DomainError::ClientError { .. }) => page.show_status(
                ICON,
                // translators: Title in the Stats tab when Last.fm doesn't know the playing song.
                &gettext("Not on Last.fm"),
                &gettext("Last.fm doesn't know this song yet."),
                None,
            ),
            Err(canto_api::DomainError::AuthExpired | canto_api::DomainError::NoToken) => {
                *loaded.borrow_mut() = None;
                show_not_set_up(&page);
            }
            Err(e) => {
                debug!("Last.fm insights failed: {e}");
                // Tried again the next time the tab opens
                *loaded.borrow_mut() = None;
                page.show_status(
                    ICON,
                    // translators: Title in the Stats tab when Last.fm can't be reached.
                    &gettext("Couldn't Reach Last.fm"),
                    &gettext("Last.fm didn't answer. Try again in a moment."),
                    None,
                );
            }
        }
    });
}

/// Something picked in the tab: a song plays at once (in its album, like a
/// click on a song anywhere in Canto), an artist opens the artist's page.
#[derive(Clone, Debug, PartialEq)]
enum Pick {
    Song {
        name: String,
        artist: Option<String>,
    },
    Artist(String),
}

type OnPick = Rc<dyn Fn(Pick)>;

/// Spotify's search query for a song: by field when the artist is known.
fn song_query(name: &str, artist: Option<&str>) -> String {
    let clean = |s: &str| s.replace('"', " ");
    match artist.filter(|a| !a.trim().is_empty()) {
        Some(by) => format!("track:\"{}\" artist:\"{}\"", clean(name), clean(by)),
        None => name.to_string(),
    }
}

/// The best of Spotify's answers: the same name (any case) first, else the
/// first one.
fn best_match<'a, T>(items: &'a [T], name: &str, name_of: impl Fn(&T) -> &str) -> Option<&'a T> {
    let wanted = name.trim().to_lowercase();
    items
        .iter()
        .find(|item| name_of(item).trim().to_lowercase() == wanted)
        .or_else(|| items.first())
}

fn open_pick(app_model: &AppModel, dispatcher: &Dispatcher, pick: Pick) {
    let api = app_model.api();
    match pick {
        Pick::Song { name, artist } => {
            let fallback = match &artist {
                Some(by) => format!("{name} {by}"),
                None => name.clone(),
            };
            dispatch_api_read_many(dispatcher, move |tag| async move {
                let mut found = api
                    .search_scoped(
                        &song_query(&name, artist.as_deref()),
                        SearchType::Tracks.into(),
                        0,
                        5,
                        tag,
                    )
                    .await?
                    .tracks
                    .items;
                if found.is_empty() && artist.is_some() {
                    found = api
                        .search_scoped(&fallback, SearchType::Tracks.into(), 0, 5, tag)
                        .await?
                        .tracks
                        .items;
                }
                let Some(track) = best_match(&found, &name, |t| t.title.as_str()).cloned() else {
                    // Not on Spotify: show the search instead
                    return Ok(vec![
                        BrowserAction::Navigate(Route::Search).into(),
                        BrowserAction::Search(fallback).into(),
                    ]);
                };
                play_in_album_actions(&api, track, tag).await
            });
        }
        Pick::Artist(name) => {
            dispatch_api_read_many(dispatcher, move |tag| async move {
                let found = api
                    .search_scoped(&name, SearchType::Artists.into(), 0, 5, tag)
                    .await?
                    .artists
                    .items;
                Ok(match best_match(&found, &name, |a| a.name.as_str()) {
                    Some(artist) => {
                        vec![BrowserAction::Navigate(Route::Artist(artist.rri.id.clone())).into()]
                    }
                    None => vec![
                        BrowserAction::Navigate(Route::Search).into(),
                        BrowserAction::Search(name).into(),
                    ],
                })
            });
        }
    }
}

fn show_not_set_up(page: &InfoPage) {
    page.show_status(
        ICON,
        &gettext("Link Last.fm to See Your Stats"),
        &gettext("Add your Last.fm username and API key in Preferences → Integrations."),
        Some(setup_button(&gettext("Set Up Last.fm"))),
    );
}

/// "122,741 scrobbles since 2019".
fn profile_line(scrobbles: Option<u64>, registered: Option<i64>) -> Option<String> {
    let scrobbles = scrobbles?;
    let count = format_count(scrobbles);
    let year = registered
        .and_then(|t| glib::DateTime::from_unix_utc(t).ok())
        .map(|d| d.year());
    let n = scrobbles.min(u64::from(u32::MAX)) as u32;
    Some(match year {
        Some(year) => ngettext(
            // translators: Stats tab, the user's Last.fm profile. {count} is a number, {year} like 2019.
            "{count} scrobble since {year}",
            "{count} scrobbles since {year}",
            n,
        )
        .replace("{count}", &count)
        .replace("{year}", &year.to_string()),
        None => ngettext("{count} scrobble", "{count} scrobbles", n).replace("{count}", &count),
    })
}

fn render(
    page: &InfoPage,
    artist: &str,
    insights: &TrackInsights,
    has_user: bool,
    on_search: &OnPick,
) {
    page.clear();
    let content = &page.content;

    // The user's own numbers
    if has_user {
        let yours = card(&["stats-panel__yours"]);
        yours.append(&heading(&gettext("Your Plays")));
        if let Some(columns) = playcount_columns(&insights.playcounts) {
            yours.append(&columns);
        }
        if insights.loved == Some(true) {
            let loved = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let heart = gtk::Image::from_icon_name(HEART_FILLED_ICON);
            heart.add_css_class("stats-panel__loved");
            loved.append(&heart);
            // translators: Stats tab: the user marked the playing song as loved on Last.fm.
            loved.append(&gtk::Label::new(Some(&gettext(
                "You loved this song on Last.fm",
            ))));
            yours.append(&loved);
        }
        if let Some(line) = profile_line(insights.scrobbles, insights.registered) {
            yours.append(&caption(&line));
        }
        // translators: Stats tab button: opens the user's top 10 artists, albums and songs.
        yours.append(&pill_button(&gettext("See Your Top 10"), |button| {
            let _ = button.activate_action("win.listening-stats", None);
        }));
        content.append(&yours);
    }

    // Everyone's numbers
    let song_numbers = number_columns(vec![
        // translators: Under a number in the Stats tab: people who played the song on Last.fm.
        (insights.track_listeners, gettext("listeners")),
        // translators: Under a number in the Stats tab: times the song was played on Last.fm.
        (insights.track_plays, gettext("plays")),
    ]);
    let artist_numbers = number_columns(vec![
        (insights.artist_listeners, gettext("listeners")),
        (insights.artist_plays, gettext("plays")),
    ]);
    if song_numbers.is_some() || artist_numbers.is_some() {
        let world = card(&["stats-panel__world"]);
        // translators: Heading of the Stats tab's card with everyone's plays on Last.fm.
        world.append(&heading(&gettext("On Last.fm")));
        // translators: Stats tab: over the playing song's numbers.
        for (what, numbers) in [
            (gettext("This Song"), song_numbers),
            (artist.to_string(), artist_numbers),
        ] {
            if let Some(numbers) = numbers {
                world.append(&caption(&what));
                world.append(&numbers);
            }
        }
        if let Some(url) = &insights.track_url {
            world.append(&link_button(&gettext("Open on Last.fm"), url.clone()));
        }
        content.append(&world);
    }

    if !insights.tags.is_empty() {
        let tags = card(&["stats-panel__tags"]);
        // translators: Heading of the Stats tab's genre-like tags from Last.fm.
        tags.append(&heading(&gettext("Tags")));
        tags.append(&chips(&insights.tags));
        content.append(&tags);
    }

    // The artist's and the album's story
    if insights.artist_bio.is_some() || insights.album_wiki.is_some() {
        let about = card(&["stats-panel__bio"]);
        about.append(&heading(artist));
        if let Some(bio) = &insights.artist_bio {
            about.append(&folding_text(glib::markup_escape_text(bio).to_string()));
        }
        if let Some(wiki) = &insights.album_wiki {
            // translators: Stats tab: over the album's description from Last.fm.
            about.append(&caption(&gettext("About the Album")));
            about.append(&folding_text(glib::markup_escape_text(wiki).to_string()));
        }
        // translators: Stats tab: where the artist's bio comes from.
        about.append(&caption(&gettext("From Last.fm")));
        if let Some(url) = &insights.artist_url {
            about.append(&link_button(&gettext("Read More on Last.fm"), url.clone()));
        }
        content.append(&about);
    }

    if !insights.top_tracks.is_empty() {
        let top = card(&["stats-panel__top"]);
        // translators: Heading in the Stats tab: the artist's most played songs on Last.fm.
        top.append(&heading(&gettext("Most Played Worldwide")));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (i, item) in insights.top_tracks.iter().enumerate() {
            let plays = (item.plays > 0).then(|| format_count(item.plays));
            let pick = Pick::Song {
                name: item.name.clone(),
                artist: Some(artist.to_string()),
            };
            let on_pick = Rc::clone(on_search);
            list.append(&action_row(
                Some(i + 1),
                &item.name,
                None,
                plays.as_deref(),
                &gettext("Play"),
                move || on_pick(pick.clone()),
            ));
        }
        top.append(&list);
        content.append(&top);
    }

    if !insights.similar_tracks.is_empty() {
        let similar = card(&["stats-panel__similar-songs"]);
        // translators: Heading in the Stats tab: songs like the playing one, from Last.fm.
        similar.append(&heading(&gettext("Similar Songs")));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for item in &insights.similar_tracks {
            let pick = Pick::Song {
                name: item.name.clone(),
                artist: item.artist.clone(),
            };
            let on_pick = Rc::clone(on_search);
            list.append(&action_row(
                None,
                &item.name,
                item.artist.as_deref(),
                None,
                &gettext("Play"),
                move || on_pick(pick.clone()),
            ));
        }
        similar.append(&list);
        content.append(&similar);
    }

    if !insights.similar_artists.is_empty() {
        let similar = card(&["stats-panel__similar-artists"]);
        // translators: Heading in the Stats tab: artists like the playing one, from Last.fm.
        similar.append(&heading(&gettext("Similar Artists")));
        similar.append(&flow(
            insights
                .similar_artists
                .iter()
                .map(|name| {
                    let on_search = Rc::clone(on_search);
                    let pick = Pick::Artist(name.clone());
                    let button = pill_button(name, move |_| on_search(pick.clone()));
                    // translators: Tooltip of a similar artist in the Stats tab: opens the artist's page.
                    button.set_tooltip_text(Some(&gettext("Open Artist")));
                    button.upcast()
                })
                .collect(),
        ));
        content.append(&similar);
    }
}

/// The tab filled with sample numbers, to look at its rendering.
#[cfg(test)]
pub(crate) fn sample_for_tests() -> gtk::Widget {
    use canto_api::scrobbler::{Playcounts, TopItem};
    let page = InfoPage::new("stats-panel");
    let item = |name: &str, artist: Option<&str>, plays: u64| TopItem {
        name: name.into(),
        artist: artist.map(Into::into),
        plays,
        image: None,
    };
    let insights = TrackInsights {
        playcounts: Playcounts {
            track: Some(12),
            album: Some(1832),
            artist: Some(2529),
        },
        loved: Some(true),
        track_listeners: Some(1_254_340),
        track_plays: Some(16_748_210),
        artist_listeners: Some(4_472_285),
        artist_plays: Some(815_125_124),
        tags: ["rnb", "soul", "alternative rnb", "2016", "neo-soul"]
            .map(String::from)
            .to_vec(),
        artist_bio: Some("Christopher Edwin Breaux, professionally known as Frank Ocean, is an American singer, songwriter and record producer from New Orleans.".into()),
        album_wiki: None,
        similar_artists: ["Daniel Caesar", "SZA", "Steve Lacy", "Brent Faiyaz"]
            .map(String::from)
            .to_vec(),
        similar_tracks: vec![
            item("Self Control", Some("Frank Ocean"), 0),
            item("Get You", Some("Daniel Caesar"), 0),
        ],
        top_tracks: vec![
            item("Pink + White", None, 62_021_220),
            item("Nights", None, 58_311_002),
            item("Ivy", None, 51_002_984),
        ],
        artist_url: Some("https://www.last.fm/music/Frank+Ocean".into()),
        track_url: Some("https://www.last.fm/music/Frank+Ocean/_/Pink+++White".into()),
        scrobbles: Some(122_741),
        registered: Some(1_548_544_192),
    };
    let on_search: OnPick = Rc::new(|_| {});
    render(&page, "Frank Ocean", &insights, true, &on_search);
    page.stack.clone().upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn song_query_asks_by_field_when_the_artist_is_known() {
        assert_eq!(
            song_query("Ivy", Some("Frank Ocean")),
            "track:\"Ivy\" artist:\"Frank Ocean\""
        );
        assert_eq!(
            song_query("Say \"Hi\"", Some("A")),
            "track:\"Say  Hi \" artist:\"A\""
        );
        assert_eq!(song_query("Ivy", None), "Ivy");
        assert_eq!(song_query("Ivy", Some(" ")), "Ivy");
    }

    #[test]
    fn best_match_prefers_the_same_name() {
        let names = ["Ivy (Live)", "ivy", "Ivy Remix"];
        assert_eq!(best_match(&names, "Ivy", |n: &&str| *n), Some(&"ivy"));
        assert_eq!(
            best_match(&names, "Nights", |n: &&str| *n),
            Some(&"Ivy (Live)")
        );
        let none: [&str; 0] = [];
        assert_eq!(best_match(&none, "Ivy", |n: &&str| *n), None);
    }

    #[test]
    fn profile_line_has_the_year_when_known() {
        let line = profile_line(Some(5), Some(1_548_544_192)).unwrap();
        assert!(line.contains('5') && line.contains("2019"), "{}", line);
        assert!(!profile_line(Some(5), None).unwrap().contains("2019"));
        assert_eq!(profile_line(None, Some(1)), None);
    }
}
