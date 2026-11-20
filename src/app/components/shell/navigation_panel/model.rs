use gettextrs::gettext;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use tokio::sync::oneshot;

use super::{NavigationPanelDestination, NavigationPanelItem};
use crate::app::components::{
    dispatch_api_call, dispatch_api_call_many, dispatch_api_read, dispatch_api_read_many,
    play_in_album_actions, queue_source_tracks, upload_playlist_cover, PlaylistForm, Saved,
};
use crate::app::models::{CardModel, PlaylistSummary};
use crate::app::playlist_actions::unfollow_and_unpin_playlist;
use crate::app::state::{load_context, start_actions, PlaybackAction, Route};
use crate::app::{AppAction, AppModel, BrowserAction, Dispatcher, PaginationTarget, SongsSource};
use crate::settings;
use canto_config::feature_flags::{is_enabled, FeatureFlag};
use canto_core::play_queue::CONTEXT_PAGE_SIZE;

// Nothing for an empty context
fn start_context(
    empty: bool,
    source: SongsSource,
    name: Option<String>,
    load: PlaybackAction,
    shuffle: bool,
) -> Vec<AppAction> {
    if empty {
        return vec![];
    }
    start_actions(shuffle, load_context(source, name, load))
}

/// How "Your Library" is ordered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibrarySort {
    #[default]
    Recents,
    RecentlyAdded,
    Alphabetical,
    Creator,
}

impl LibrarySort {
    pub const ALL: [LibrarySort; 4] = [
        LibrarySort::Recents,
        LibrarySort::RecentlyAdded,
        LibrarySort::Alphabetical,
        LibrarySort::Creator,
    ];

    pub fn key(self) -> &'static str {
        match self {
            Self::Recents => "recents",
            Self::RecentlyAdded => "recently-added",
            Self::Alphabetical => "alphabetical",
            Self::Creator => "creator",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|s| s.key() == key)
            .unwrap_or_default()
    }

    pub fn label(self) -> String {
        match self {
            // translators: Library order: what was played most recently first.
            Self::Recents => gettext("Recents"),
            // translators: Library order: what was saved most recently first.
            Self::RecentlyAdded => gettext("Recently Added"),
            // translators: Library order: by name.
            Self::Alphabetical => gettext("Alphabetical"),
            // translators: Library order: by who made it (playlist owner, album artist).
            Self::Creator => gettext("Creator"),
        }
    }
}

/// How "Your Library" is shown: rows, or a grid of covers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryView {
    #[default]
    List,
    Grid,
}

impl LibraryView {
    pub const ALL: [LibraryView; 2] = [LibraryView::List, LibraryView::Grid];

    pub fn key(self) -> &'static str {
        match self {
            Self::List => "list",
            Self::Grid => "grid",
        }
    }

    pub fn from_key(key: &str) -> Self {
        Self::ALL
            .iter()
            .copied()
            .find(|v| v.key() == key)
            .unwrap_or_default()
    }

    pub fn label(self) -> String {
        match self {
            // translators: "View As" option of Your Library: one item per row.
            Self::List => gettext("List"),
            // translators: "View As" option of Your Library: covers side by side.
            Self::Grid => gettext("Grid"),
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            Self::List => "view-list-symbolic",
            Self::Grid => "view-grid-symbolic",
        }
    }
}

/// Which kind of item "Your Library" lists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibraryFilter {
    #[default]
    All,
    Playlists,
    Albums,
    Artists,
}

impl LibraryFilter {
    fn shows_playlists(self) -> bool {
        matches!(self, Self::All | Self::Playlists)
    }

    fn shows_albums(self) -> bool {
        matches!(self, Self::All | Self::Albums)
    }

    fn shows_artists(self) -> bool {
        matches!(self, Self::All | Self::Artists)
    }
}

pub struct NavigationPanelModel {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    filter: std::cell::Cell<LibraryFilter>,
    sort: std::cell::Cell<LibrarySort>,
    view: std::cell::Cell<LibraryView>,
    query: std::cell::RefCell<String>,
    /// Library keys of what was played lately, newest first
    recents: std::cell::RefCell<Vec<String>>,
    recents_fetched: std::cell::Cell<Option<std::time::Instant>>,
    /// The covers on show, by URL and size: a row or tile made again shows
    /// its cover at once instead of blinking empty while it loads.
    covers: Rc<RefCell<HashMap<(String, i32), gtk::gdk::Texture>>>,
}

impl NavigationPanelModel {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        let settings = gtk::gio::Settings::new(canto_config::settings::SETTINGS);
        let sort = LibrarySort::from_key(&settings.string(canto_config::settings::LIBRARY_SORT));
        let view = LibraryView::from_key(&settings.string(canto_config::settings::LIBRARY_VIEW));
        Self {
            app_model,
            dispatcher,
            filter: Default::default(),
            sort: std::cell::Cell::new(sort),
            view: std::cell::Cell::new(view),
            query: Default::default(),
            recents: Default::default(),
            recents_fetched: Default::default(),
            covers: Default::default(),
        }
    }

    pub fn sort(&self) -> LibrarySort {
        self.sort.get()
    }

    pub fn set_sort(&self, sort: LibrarySort) {
        self.sort.set(sort);
        let _ = gtk::gio::Settings::new(canto_config::settings::SETTINGS)
            .set_string(canto_config::settings::LIBRARY_SORT, sort.key());
    }

    pub fn view(&self) -> LibraryView {
        self.view.get()
    }

    pub fn set_view(&self, view: LibraryView) {
        self.view.set(view);
        let _ = gtk::gio::Settings::new(canto_config::settings::SETTINGS)
            .set_string(canto_config::settings::LIBRARY_VIEW, view.key());
    }

    pub fn set_query(&self, query: &str) {
        *self.query.borrow_mut() = query.trim().to_lowercase();
    }

    /// Puts what plays now at the top of the recents. Whether it moved.
    pub fn bump_playing_to_recents(&self) -> bool {
        let key = match self.app_model_state_source() {
            Some(SongsSource::Playlist(id)) => format!("playlist:{id}"),
            Some(SongsSource::Album(id)) => format!("album:{id}"),
            Some(SongsSource::Artist(id)) => format!("artist:{id}"),
            Some(SongsSource::SavedTracks) => "liked".to_string(),
            _ => return false,
        };
        let mut recents = self.recents.borrow_mut();
        if recents.first() == Some(&key) {
            return false;
        }
        recents.retain(|k| k != &key);
        recents.insert(0, key);
        true
    }

    /// Asks Spotify what was played lately (at most every few minutes) and
    /// re-sorts the library when it answers.
    pub fn refresh_recents(self: &Rc<Self>, list_store: &gtk::gio::ListStore) {
        let stale = self
            .recents_fetched
            .get()
            .is_none_or(|at| at.elapsed() > std::time::Duration::from_secs(180));
        if !stale {
            return;
        }
        self.recents_fetched.set(Some(std::time::Instant::now()));
        let api = self.app_model.api();
        let weak_model = Rc::downgrade(self);
        let weak_store = list_store.downgrade();
        glib::spawn_future_local(async move {
            let fetched = match api.get_recent_contexts().await {
                Ok(fetched) => {
                    log::info!("recently played: {} items", fetched.len());
                    fetched
                }
                Err(err) => {
                    log::debug!("recently played failed: {err:?}");
                    // Not logged in yet, most likely: try again on the next chance
                    if let Some(model) = weak_model.upgrade() {
                        model.recents_fetched.set(None);
                    }
                    return;
                }
            };
            let (Some(model), Some(store)) = (weak_model.upgrade(), weak_store.upgrade()) else {
                return;
            };
            {
                // What Canto saw play first, then Spotify's history
                let mut recents = model.recents.borrow_mut();
                let mut merged: Vec<String> = Vec::new();
                if let Some(first) = recents.first() {
                    merged.push(first.clone());
                }
                for key in fetched.into_iter().chain(recents.iter().cloned()) {
                    if !merged.contains(&key) {
                        merged.push(key);
                    }
                }
                *recents = merged;
            }
            if model.sort() == LibrarySort::Recents {
                model.apply_navigation_panel_items(&store);
            }
        });
    }

    /// Sorts and searches the library rows (the pinned ones stay on top).
    fn arrange(&self, items: Vec<NavigationPanelItem>) -> Vec<NavigationPanelItem> {
        let query = self.query.borrow().clone();
        let mut items: Vec<NavigationPanelItem> = if query.is_empty() {
            items
        } else {
            items
                .into_iter()
                .filter(|item| {
                    item.title().to_lowercase().contains(&query)
                        || item
                            .property::<String>("subtitle")
                            .to_lowercase()
                            .contains(&query)
                })
                .collect()
        };
        let recents = self.recents.borrow();
        let creator = |item: &NavigationPanelItem| {
            let subtitle = item.property::<String>("subtitle");
            subtitle
                .split_once(" • ")
                .map(|(_, who)| who.to_lowercase())
                .unwrap_or_else(|| item.title().to_lowercase())
        };
        match self.sort.get() {
            LibrarySort::Recents => items.sort_by_key(|item| {
                item.library_key()
                    .and_then(|key| recents.iter().position(|k| *k == key))
                    .unwrap_or(usize::MAX)
            }),
            LibrarySort::RecentlyAdded => {}
            LibrarySort::Alphabetical => items.sort_by_key(|item| item.title().to_lowercase()),
            LibrarySort::Creator => {
                items.sort_by_key(|item| (creator(item), item.title().to_lowercase()))
            }
        }
        items
    }

    /// Where the music plays from, while it plays.
    pub fn app_model_state_source(&self) -> Option<SongsSource> {
        let state = self.app_model.get_state();
        state
            .playback
            .is_playing()
            .then(|| state.playback.current_source().cloned())
            .flatten()
    }

    /// Whether `item` is what the music plays from.
    pub fn is_playing_from(&self, item: &NavigationPanelItem) -> bool {
        match (self.app_model_state_source(), item.destination()) {
            (Some(SongsSource::Playlist(id)), Some(NavigationPanelDestination::Playlist(p))) => {
                id == p.id
            }
            (
                Some(SongsSource::Album(id)),
                Some(NavigationPanelDestination::Album { id: a, .. }),
            ) => id == a,
            (
                Some(SongsSource::Artist(id)),
                Some(NavigationPanelDestination::Artist { id: a, .. }),
            ) => id == a,
            (Some(SongsSource::SavedTracks), Some(NavigationPanelDestination::SavedTracks)) => true,
            _ => false,
        }
    }

    pub fn filter(&self) -> LibraryFilter {
        self.filter.get()
    }

    pub fn set_filter(&self, filter: LibraryFilter) {
        self.filter.set(filter);
    }

    fn get_playlists(&self) -> Vec<NavigationPanelDestination> {
        self.app_model
            .get_state()
            .browser
            .home_state()
            .expect("expected HomeState to be available")
            .playlists
            .iter()
            .map(Self::map_to_destination)
            .collect()
    }

    pub fn load_more_playlists(&self) -> Option<()> {
        let api = self.app_model.api();
        let state = self.app_model.get_state();
        let home = state.browser.home_state()?;
        let batch_size = home.next_playlists_page.batch_size;
        let offset = home.next_playlists_page.next_offset?;
        drop(state);

        self.app_model
            .update_state(BrowserAction::ConsumeNextPage(PaginationTarget::SavedPlaylists).into());

        dispatch_api_read(&self.dispatcher, move |tag| async move {
            api.get_saved_playlists(offset, batch_size, tag)
                .await
                .map(|page| BrowserAction::AppendPlaylistsContent(page.items).into())
        });

        Some(())
    }

    /// The cover of a playlist, album, artist or track the sidebar shows.
    fn cover_for(&self, kind: settings::PinnedKind, id: &str) -> Option<String> {
        let state = self.app_model.get_state();
        let home = state.browser.home_state()?;
        let card = |cards: &crate::app::ListStore<CardModel>| {
            cards.iter().find(|c| c.id() == id).and_then(|c| c.image())
        };
        match kind {
            settings::PinnedKind::Playlist => card(&home.playlists),
            settings::PinnedKind::Album => card(&home.albums),
            settings::PinnedKind::Artist => card(&home.artists),
            settings::PinnedKind::Track => home
                .saved_tracks
                .get(id)
                .and_then(|sm| sm.description().art.best_for_width(64).map(str::to_owned)),
        }
    }

    /// The kind line under a pinned item's title.
    fn with_kind_subtitle(&self, item: NavigationPanelItem) -> NavigationPanelItem {
        match item.destination() {
            Some(NavigationPanelDestination::Playlist(_)) => return self.with_playlist_owner(item),
            Some(NavigationPanelDestination::Album { .. }) => {
                let artist = {
                    let state = self.app_model.get_state();
                    state.browser.home_state().and_then(|home| {
                        home.albums
                            .iter()
                            .find(|c| c.id() == item.data())
                            .map(|c| c.subtitle())
                    })
                };
                item.set_subtitle(with_detail(&gettext("Album"), &artist.unwrap_or_default()));
            }
            Some(NavigationPanelDestination::Artist { .. }) => item.set_subtitle(gettext("Artist")),
            Some(NavigationPanelDestination::Track { .. }) => item.set_subtitle(gettext("Song")),
            _ => {}
        }
        item
    }

    /// "Playlist • owner" under a playlist's title.
    fn with_playlist_owner(&self, item: NavigationPanelItem) -> NavigationPanelItem {
        let owner = {
            let state = self.app_model.get_state();
            state.browser.home_state().and_then(|home| {
                home.playlists
                    .iter()
                    .find(|c| c.id() == item.data())
                    .map(|c| c.subtitle())
            })
        };
        item.set_subtitle(with_detail(
            &gettext("Playlist"),
            &owner.unwrap_or_default(),
        ));
        item
    }

    fn with_cover(&self, item: NavigationPanelItem) -> NavigationPanelItem {
        let kind = match item.destination() {
            Some(NavigationPanelDestination::Playlist(_)) => settings::PinnedKind::Playlist,
            Some(NavigationPanelDestination::Album { .. }) => settings::PinnedKind::Album,
            Some(NavigationPanelDestination::Artist { .. }) => settings::PinnedKind::Artist,
            Some(NavigationPanelDestination::Track { .. }) => settings::PinnedKind::Track,
            _ => return item,
        };
        if let Some(url) = self.cover_for(kind, &item.data()) {
            item.set_image(url);
        }
        item
    }

    /// Shows the cover at `url` in `image`, `px` pixels wide: right away
    /// when it was shown before or is in memory, else once it loads.
    pub(super) fn load_cover(&self, image: &gtk::Image, url: &str, px: i32) {
        if canto_api::models::is_resource_url(url) {
            return;
        }
        if show_known_cover(&self.covers, image, url, px) {
            return;
        }
        let api = self.app_model.api();
        if let Some(texture) = api.cached_texture(url, px, px) {
            image.set_paintable(Some(&texture));
            self.covers
                .borrow_mut()
                .insert((url.to_owned(), px), texture);
            return;
        }
        let covers = Rc::downgrade(&self.covers);
        let url = url.to_owned();
        let weak = image.downgrade();
        // Behind whatever page is loading
        let tag = canto_api::Load::new(
            canto_api::LoadPriority::Visible,
            canto_api::BACKGROUND_EPOCH,
        );
        glib::MainContext::default().spawn_local(async move {
            let Some(texture) = api.load_image(&url, px, px, tag).await else {
                return;
            };
            if let Some(image) = weak.upgrade() {
                image.set_paintable(Some(&texture));
            }
            if let Some(covers) = covers.upgrade() {
                covers.borrow_mut().insert((url, px), texture);
            }
        });
    }

    fn map_to_destination(a: CardModel) -> NavigationPanelDestination {
        let title = Some(a.title())
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| gettext("Unnamed Playlist"));
        let id = a.id();
        NavigationPanelDestination::Playlist(PlaylistSummary { id, title })
    }

    pub(super) fn create_new_playlist(&self, new: PlaylistForm) -> oneshot::Receiver<()> {
        let (created, created_rx) = Saved::channel();
        let user_id = self.app_model.get_state().logged_user.user.clone().unwrap();
        let api = self.app_model.api();
        let dispatcher = self.dispatcher.clone();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            let playlist = api
                .create_playlist(&user_id, &new.name, new.description.as_deref(), new.public)
                .await?;
            created.tell();
            let id = playlist.rri.id.clone();
            if let Some(cover) = new.cover {
                upload_playlist_cover(&dispatcher, api, id.clone(), cover);
            }
            Ok(vec![
                AppAction::CreatePlaylist(playlist),
                AppAction::ViewPlaylist(id),
            ])
        });
        created_rx
    }

    pub(super) fn is_playlist_owned(&self, id: &str) -> bool {
        self.app_model
            .get_state()
            .logged_user
            .owned_playlist_ids
            .contains(id)
    }

    pub(super) fn logged_user_id(&self) -> Option<String> {
        self.app_model.get_state().logged_user.user.clone()
    }

    pub(super) fn unfollow_playlist(&self, id: String) {
        let user_id = self.logged_user_id();
        let api = self.app_model.api();
        dispatch_api_call(&self.dispatcher, move || async move {
            unfollow_and_unpin_playlist(&api, id, user_id).await
        })
    }

    fn playlist_title(&self, id: &str) -> Option<String> {
        self.get_playlists()
            .into_iter()
            .find_map(|dest| match dest {
                NavigationPanelDestination::Playlist(summary) if summary.id == id => {
                    Some(summary.title)
                }
                _ => None,
            })
    }

    /// Plays what a library row opens, from its first track (double click).
    pub(super) fn play_destination(&self, dest: NavigationPanelDestination) {
        match dest {
            NavigationPanelDestination::Playlist(summary) => self.play_playlist(summary.id),
            NavigationPanelDestination::Album { id, .. } => self.play_album(id, false),
            NavigationPanelDestination::Artist { id, .. } => self.play_artist(id, false),
            NavigationPanelDestination::Track { id, .. } => self.play_track(id),
            NavigationPanelDestination::SavedTracks => self.play_liked_songs(),
            _ => {}
        }
    }

    fn play_liked_songs(&self) {
        let api = self.app_model.api();
        let source = SongsSource::SavedTracks;
        let name = Some(gettext("Liked Songs"));
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api.get_saved_tracks(0, CONTEXT_PAGE_SIZE, tag).await?;
            let empty = batch.items.is_empty();
            let load = PlaybackAction::LoadPagedSongs(source.clone(), batch);
            Ok(start_context(empty, source, name, load, false))
        });
    }

    pub(super) fn play_playlist(&self, id: String) {
        self.start_playlist(id, false);
    }

    pub(super) fn shuffle_playlist(&self, id: String) {
        self.start_playlist(id, true);
    }

    fn start_playlist(&self, id: String, shuffle: bool) {
        let api = self.app_model.api();
        let source = SongsSource::Playlist(id.clone());
        let name = self.playlist_title(&id);
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api
                .get_playlist_tracks(&id, 0, CONTEXT_PAGE_SIZE, tag)
                .await?;
            let empty = batch.items.is_empty();
            let load = PlaybackAction::LoadPagedSongs(source.clone(), batch);
            Ok(start_context(empty, source, name, load, shuffle))
        });
    }

    pub(super) fn queue_playlist(&self, id: String) {
        let source = SongsSource::Playlist(id);
        queue_source_tracks(&self.app_model, &self.dispatcher, source);
    }

    /// Play a pinned album from its first track, or shuffled.
    pub(super) fn play_album(&self, id: String, shuffle: bool) {
        let api = self.app_model.api();
        let source = SongsSource::Album(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let batch = api.get_album_tracks(&id, 0, CONTEXT_PAGE_SIZE, tag).await?;
            let empty = batch.items.is_empty();
            let name = batch
                .items
                .iter()
                .find_map(|t| Some(t.album.as_ref()?.name.clone()));
            let load = PlaybackAction::LoadPagedSongs(source.clone(), batch);
            Ok(start_context(empty, source, name, load, shuffle))
        });
    }

    /// Play a pinned artist's top tracks, or shuffle them.
    pub(super) fn play_artist(&self, id: String, shuffle: bool) {
        let api = self.app_model.api();
        let source = SongsSource::Artist(id.clone());
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let tracks = api.get_artist_top_tracks(&id, tag).await?;
            let empty = tracks.is_empty();
            let name = tracks
                .iter()
                .flat_map(|t| t.artists.iter())
                .find(|a| a.rri.id == id)
                .map(|a| a.name.clone());
            let load = PlaybackAction::LoadContextSongs(source.clone(), tracks);
            Ok(start_context(empty, source, name, load, shuffle))
        });
    }

    /// Whether a pinned album, artist or track is known to be in the user's
    /// library. Only the loaded pages of the library are checked.
    pub(super) fn is_in_library(&self, kind: settings::PinnedKind, id: &str) -> bool {
        let state = self.app_model.get_state();
        let Some(home) = state.browser.home_state() else {
            return false;
        };
        match kind {
            settings::PinnedKind::Playlist => home.playlists.iter().any(|c| c.id() == id),
            settings::PinnedKind::Album => home.albums.iter().any(|c| c.id() == id),
            settings::PinnedKind::Artist => home.artists.iter().any(|c| c.id() == id),
            settings::PinnedKind::Track => home.is_track_liked(id),
        }
    }

    /// Remove a pinned album, artist or track from the user's library, and
    /// unpin it as it can no longer be pinned from its detail page.
    pub(super) fn remove_from_library(&self, kind: settings::PinnedKind, id: String) {
        if kind == settings::PinnedKind::Playlist {
            self.unfollow_playlist(id);
            return;
        }
        let user_id = self.logged_user_id();
        let api = self.app_model.api();
        dispatch_api_call_many(&self.dispatcher, move || async move {
            let removed: AppAction = match kind {
                settings::PinnedKind::Album => {
                    api.remove_albums(&id).await?;
                    BrowserAction::UnsaveAlbum(id.clone()).into()
                }
                settings::PinnedKind::Artist => {
                    api.unfollow_artists(&id).await?;
                    BrowserAction::UnfollowArtist(id.clone()).into()
                }
                _ => {
                    api.remove_tracks(vec![id.clone()]).await?;
                    BrowserAction::RemoveSavedTracks(vec![id.clone()]).into()
                }
            };
            let mut actions = vec![removed];
            if user_id.is_some_and(|user_id| settings::unpin_object(&user_id, kind, &id)) {
                actions.push(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
            }
            Ok(actions)
        });
    }

    pub(super) fn navigate(&self, dest: NavigationPanelDestination) {
        let actions = match dest {
            NavigationPanelDestination::Library
            | NavigationPanelDestination::SavedTracks
            | NavigationPanelDestination::SavedPlaylists
            | NavigationPanelDestination::SavedArtists => {
                let page = dest.home_page().expect("home destination");
                vec![BrowserAction::Navigate(Route::Home(page)).into()]
            }
            NavigationPanelDestination::Playlist(PlaylistSummary { id, .. }) => {
                vec![AppAction::ViewPlaylist(id)]
            }
            NavigationPanelDestination::Album { id, .. } => vec![AppAction::ViewAlbum(id)],
            NavigationPanelDestination::Artist { id, .. } => vec![AppAction::ViewArtist(id)],
            NavigationPanelDestination::Track { id, .. } => {
                self.view_track_album(id);
                return;
            }
        };
        self.dispatcher.dispatch_many(actions);
    }

    /// Open the album page of a track, as there is no track detail page.
    fn view_track_album(&self, id: String) {
        let api = self.app_model.api();
        dispatch_api_read(&self.dispatcher, move |tag| async move {
            let track = api.get_track(&id, tag).await?;
            Ok(AppAction::ViewAlbum(
                track.album.map(|a| a.rri.id).unwrap_or_default(),
            ))
        });
    }

    /// Play a single track in the context of its album.
    ///
    /// `PlaybackAction::Load` only plays songs already in the queue, so the
    /// album is queued first.
    pub(super) fn play_track(&self, id: String) {
        let api = self.app_model.api();
        dispatch_api_read_many(&self.dispatcher, move |tag| async move {
            let track = api.get_track(&id, tag).await?;
            play_in_album_actions(&api, track, tag).await
        });
    }

    pub(super) fn toggle_pin_playlist(&self, id: &str) {
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        let changed = if settings::is_object_pinned(&user_id, id, settings::PinnedKind::Playlist) {
            settings::unpin_object(&user_id, settings::PinnedKind::Playlist, id)
        } else if !self.is_in_library(settings::PinnedKind::Playlist, id) {
            // Only saved playlists can be pinned.
            return;
        } else {
            let title = self.saved_playlist_title(id);
            settings::pin_object(&user_id, settings::PinnedKind::Playlist, id, title)
        };
        if changed {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }

    pub(super) fn unpin(&self, kind: settings::PinnedKind, id: &str) {
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        if settings::unpin_object(&user_id, kind, id) {
            self.dispatcher
                .dispatch(BrowserAction::NotifyPinnedPlaylistsUpdated.into());
        }
    }

    fn saved_playlist_title(&self, id: &str) -> Option<String> {
        self.app_model
            .get_state()
            .browser
            .home_state()?
            .playlists
            .iter()
            .find(|c| c.id() == id)
            .map(|c| c.title())
    }

    fn prune_stale_pins(&self) {
        if !is_enabled(FeatureFlag::PinnedObjects) {
            return;
        }
        let Some(user_id) = self.logged_user_id() else {
            return;
        };
        // Only playlist pins can be pruned reliably: the saved-playlists list
        // is the authoritative set of playlists that still exist. Albums,
        // artists and tracks are pinned from detail pages that may be opened
        // before the corresponding home lists are loaded, so pruning against
        // those lists could drop valid pins.
        //
        // Even for playlists, prune only once every page has been loaded,
        // otherwise pins past the loaded pages (or all pins, before the first
        // page arrives) would be dropped.
        let fully_loaded = self
            .app_model
            .get_state()
            .browser
            .home_state()
            .is_some_and(|home| home.next_playlists_page.next_offset.is_none());
        if !fully_loaded {
            return;
        }
        let saved_ids: Vec<String> = self
            .get_playlists()
            .into_iter()
            .filter_map(|destination| {
                if let NavigationPanelDestination::Playlist(summary) = destination {
                    Some(summary.id)
                } else {
                    None
                }
            })
            .collect();
        let _ =
            settings::prune_pinned_objects(&user_id, settings::PinnedKind::Playlist, &saved_ids);
    }

    /// Fills "Your Library": Saved Tracks first, then pinned items, then
    /// playlists, albums and artists, as the filter allows.
    pub fn apply_navigation_panel_items(&self, list_store: &gio::ListStore) {
        self.prune_stale_pins();
        let filter = self.filter.get();
        let (pinned, playlists) = self.build_navigation_panel_items();
        let mut items: Vec<NavigationPanelItem> = Vec::new();
        if filter.shows_playlists() {
            let saved =
                NavigationPanelItem::from_destination(NavigationPanelDestination::SavedTracks);
            saved.set_subtitle(gettext("Playlist"));
            items.push(saved);
        }
        let pinned_ids: Vec<String> = pinned.iter().map(|item| item.data()).collect();
        if filter == LibraryFilter::All {
            items.extend(pinned);
        }
        if filter.shows_playlists() {
            items.extend(playlists);
        }
        // Pinned ones are already at the top
        let not_pinned = |item: &NavigationPanelItem| {
            filter != LibraryFilter::All || !pinned_ids.contains(&item.data())
        };
        if filter.shows_albums() {
            items.extend(self.album_items().into_iter().filter(not_pinned));
        }
        if filter.shows_artists() {
            items.extend(self.artist_items().into_iter().filter(not_pinned));
        }
        let items = self.arrange_keeping_pins(items, &pinned_ids);
        // Covers no longer listed are let go
        let shown: HashSet<String> = items.iter().map(|item| item.image()).collect();
        self.covers
            .borrow_mut()
            .retain(|(url, _), _| shown.contains(url));
        update_store(list_store, &items);
    }

    /// The pinned rows first as they are, then the rest sorted and searched.
    fn arrange_keeping_pins(
        &self,
        items: Vec<NavigationPanelItem>,
        pinned_ids: &[String],
    ) -> Vec<NavigationPanelItem> {
        let searching = !self.query.borrow().is_empty();
        let (top, rest): (Vec<_>, Vec<_>) = items.into_iter().partition(|item| {
            !searching
                && self.filter.get() == LibraryFilter::All
                && pinned_ids.contains(&item.data())
        });
        let mut out = top;
        out.extend(self.arrange(rest));
        out
    }

    fn album_items(&self) -> Vec<NavigationPanelItem> {
        let cards: Vec<CardModel> = {
            let state = self.app_model.get_state();
            let Some(home) = state.browser.home_state() else {
                return vec![];
            };
            home.albums.iter().collect()
        };
        cards
            .into_iter()
            .map(|card| {
                let item =
                    NavigationPanelItem::from_destination(NavigationPanelDestination::Album {
                        id: card.id(),
                        title: card.title(),
                    });
                item.set_subtitle(with_detail(&gettext("Album"), &card.subtitle()));
                if let Some(url) = card.image() {
                    item.set_image(url);
                }
                item
            })
            .collect()
    }

    fn artist_items(&self) -> Vec<NavigationPanelItem> {
        let cards: Vec<CardModel> = {
            let state = self.app_model.get_state();
            let Some(home) = state.browser.home_state() else {
                return vec![];
            };
            home.artists.iter().collect()
        };
        cards
            .into_iter()
            .map(|card| {
                let item =
                    NavigationPanelItem::from_destination(NavigationPanelDestination::Artist {
                        id: card.id(),
                        title: card.title(),
                    });
                item.set_subtitle(gettext("Artist"));
                if let Some(url) = card.image() {
                    item.set_image(url);
                }
                item
            })
            .collect()
    }

    fn pinned_title_for(&self, object: &settings::PinnedObject) -> String {
        let fallback = match object.kind {
            settings::PinnedKind::Playlist => gettext("Pinned Playlist"),
            settings::PinnedKind::Album => gettext("Pinned Album"),
            settings::PinnedKind::Artist => gettext("Pinned Artist"),
            settings::PinnedKind::Track => gettext("Pinned Track"),
        };
        let fallback = object.title.clone().unwrap_or(fallback);
        let state = self.app_model.get_state();
        let Some(home) = state.browser.home_state() else {
            return fallback;
        };
        match object.kind {
            settings::PinnedKind::Playlist => home
                .playlists
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Album => home
                .albums
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Artist => home
                .artists
                .iter()
                .find(|c| c.id() == object.id)
                .map(|c| c.title())
                .unwrap_or(fallback),
            settings::PinnedKind::Track => home
                .saved_tracks
                .get(&object.id)
                .map(|sm| sm.description().title.clone())
                .unwrap_or(fallback),
        }
    }

    fn to_pinned_item(&self, object: settings::PinnedObject) -> NavigationPanelItem {
        let title = self.pinned_title_for(&object);
        NavigationPanelItem::from_destination(match object.kind {
            settings::PinnedKind::Playlist => {
                NavigationPanelDestination::Playlist(PlaylistSummary {
                    id: object.id,
                    title,
                })
            }
            settings::PinnedKind::Album => NavigationPanelDestination::Album {
                id: object.id,
                title,
            },
            settings::PinnedKind::Artist => NavigationPanelDestination::Artist {
                id: object.id,
                title,
            },
            settings::PinnedKind::Track => NavigationPanelDestination::Track {
                id: object.id,
                title,
            },
        })
    }

    /// Build the pinned rows (with their section header) and the playlist rows.
    fn build_navigation_panel_items(&self) -> (Vec<NavigationPanelItem>, Vec<NavigationPanelItem>) {
        let mut pinned_items = Vec::new();
        let mut items = Vec::new();
        let pinned_enabled = is_enabled(FeatureFlag::PinnedObjects);
        let playlists = self.get_playlists();
        let pinned: Vec<settings::PinnedObject> = self
            .logged_user_id()
            .map(|user_id| settings::get_pinned_objects(&user_id))
            .unwrap_or_default();
        let pinned_playlist_ids: Vec<String> = pinned
            .iter()
            .filter(|o| o.kind == settings::PinnedKind::Playlist)
            .map(|o| o.id.clone())
            .collect();

        if pinned_enabled {
            // First in the list, marked with a pin (no section header)
            for object in pinned {
                let item = self.with_cover(self.to_pinned_item(object));
                item.set_pinned(true);
                pinned_items.push(self.with_kind_subtitle(item));
            }

            let mut unpinned = Vec::new();
            for p in playlists {
                if let NavigationPanelDestination::Playlist(ref summary) = p {
                    if pinned_playlist_ids.contains(&summary.id) {
                        continue;
                    }
                }
                unpinned.push(p);
            }

            items.extend(
                unpinned
                    .into_iter()
                    .map(NavigationPanelItem::from_destination)
                    .map(|item| self.with_playlist_owner(self.with_cover(item))),
            );
        } else {
            items.extend(
                playlists
                    .into_iter()
                    .map(NavigationPanelItem::from_destination)
                    .map(|item| self.with_playlist_owner(self.with_cover(item))),
            );
        }

        (pinned_items, items)
    }
}

/// "Kind • detail", or just the kind without a detail.
fn with_detail(kind: &str, detail: &str) -> String {
    if detail.trim().is_empty() {
        kind.to_string()
    } else {
        format!("{kind} • {detail}")
    }
}

/// Shows the cover already on show elsewhere (or before), if any.
fn show_known_cover(
    covers: &RefCell<HashMap<(String, i32), gtk::gdk::Texture>>,
    image: &gtk::Image,
    url: &str,
    px: i32,
) -> bool {
    let covers = covers.borrow();
    let Some(texture) = covers.get(&(url.to_owned(), px)) else {
        return false;
    };
    image.set_paintable(Some(texture));
    true
}

/// What a library row shows: two rows that show the same need no new widget.
fn row_look(item: &NavigationPanelItem) -> (String, String, String, String, String, bool, bool) {
    (
        item.id(),
        item.data(),
        item.title(),
        item.subtitle(),
        item.image(),
        item.pinned(),
        item.navigatable(),
    )
}

/// The least to change to turn one list of rows into another.
#[derive(Debug, PartialEq, Eq)]
enum ListChange {
    Same,
    /// One row only moved (what plays now went to the top of Recents).
    Move { from: usize, to: usize },
    /// The rows `at..at + removed` give way to `added` new ones.
    Replace {
        at: usize,
        removed: usize,
        added: usize,
    },
}

/// Compares `old` and `new` from both ends: the rows they share there stay.
fn list_change<T: PartialEq>(old: &[T], new: &[T]) -> ListChange {
    let start = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    if start == old.len() && start == new.len() {
        return ListChange::Same;
    }
    let end = old[start..]
        .iter()
        .rev()
        .zip(new[start..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let old_mid = &old[start..old.len() - end];
    let new_mid = &new[start..new.len() - end];
    let n = old_mid.len();
    if n >= 2 && n == new_mid.len() {
        if new_mid[0] == old_mid[n - 1] && new_mid[1..] == old_mid[..n - 1] {
            return ListChange::Move {
                from: start + n - 1,
                to: start,
            };
        }
        if new_mid[n - 1] == old_mid[0] && new_mid[..n - 1] == old_mid[1..] {
            return ListChange::Move {
                from: start,
                to: start + n - 1,
            };
        }
    }
    ListChange::Replace {
        at: start,
        removed: n,
        added: new_mid.len(),
    }
}

/// Makes `list_store` show `items`, changing only the rows that differ: the
/// others keep their widgets, covers and place. Replacing everything made
/// every cover of the library blink on each play (20/11).
fn update_store(list_store: &gtk::gio::ListStore, items: &[NavigationPanelItem]) {
    let old: Vec<NavigationPanelItem> = (0..list_store.n_items())
        .filter_map(|i| list_store.item(i).and_downcast())
        .collect();
    let old_looks: Vec<_> = old.iter().map(row_look).collect();
    let new_looks: Vec<_> = items.iter().map(row_look).collect();
    match list_change(&old_looks, &new_looks) {
        ListChange::Same => {}
        ListChange::Move { from, to } => {
            let item = old[from].clone();
            list_store.remove(from as u32);
            list_store.insert(to as u32, &item);
        }
        ListChange::Replace { at, removed, added } => {
            list_store.splice(at as u32, removed as u32, &items[at..at + added]);
        }
    }
}

/// `update_store`, for the GTK checks.
#[cfg(test)]
pub(crate) fn update_store_for_tests(
    list_store: &gtk::gio::ListStore,
    items: &[NavigationPanelItem],
) {
    update_store(list_store, items);
}

#[cfg(test)]
mod list_change_tests {
    use super::{list_change, ListChange};

    #[test]
    fn the_same_rows_change_nothing() {
        assert_eq!(list_change(&[1, 2, 3], &[1, 2, 3]), ListChange::Same);
        assert_eq!(list_change::<i32>(&[], &[]), ListChange::Same);
    }

    #[test]
    fn a_row_moved_up_is_one_move() {
        // What plays now goes to the top of Recents (Saved Tracks stays first)
        assert_eq!(
            list_change(&[0, 1, 2, 3, 4, 5], &[0, 4, 1, 2, 3, 5]),
            ListChange::Move { from: 4, to: 1 }
        );
        assert_eq!(
            list_change(&[1, 2, 3], &[3, 1, 2]),
            ListChange::Move { from: 2, to: 0 }
        );
    }

    #[test]
    fn a_row_moved_down_is_one_move() {
        assert_eq!(
            list_change(&[1, 2, 3, 4], &[2, 3, 1, 4]),
            ListChange::Move { from: 0, to: 2 }
        );
    }

    #[test]
    fn two_rows_swapped_are_one_move() {
        assert_eq!(
            list_change(&[1, 2, 3, 4], &[1, 3, 2, 4]),
            ListChange::Move { from: 2, to: 1 }
        );
    }

    #[test]
    fn added_and_removed_rows_touch_only_their_place() {
        assert_eq!(
            list_change(&[1, 2, 3], &[1, 9, 2, 3]),
            ListChange::Replace {
                at: 1,
                removed: 0,
                added: 1
            }
        );
        assert_eq!(
            list_change(&[1, 2, 3, 4], &[1, 4]),
            ListChange::Replace {
                at: 1,
                removed: 2,
                added: 0
            }
        );
        assert_eq!(
            list_change(&[1, 2, 3], &[]),
            ListChange::Replace {
                at: 0,
                removed: 3,
                added: 0
            }
        );
        assert_eq!(
            list_change(&[], &[1, 2]),
            ListChange::Replace {
                at: 0,
                removed: 0,
                added: 2
            }
        );
    }

    #[test]
    fn a_reshuffle_replaces_only_the_rows_in_between() {
        assert_eq!(
            list_change(&[1, 2, 3, 4, 5], &[1, 4, 3, 2, 5]),
            ListChange::Replace {
                at: 1,
                removed: 3,
                added: 3
            }
        );
    }

    #[test]
    fn repeated_rows_do_not_overlap_the_ends() {
        assert_eq!(
            list_change(&[1, 1], &[1]),
            ListChange::Replace {
                at: 1,
                removed: 1,
                added: 0
            }
        );
        assert_eq!(
            list_change(&[1], &[1, 1]),
            ListChange::Replace {
                at: 1,
                removed: 0,
                added: 1
            }
        );
    }
}

#[cfg(test)]
mod view_tests {
    use super::LibraryView;

    #[test]
    fn library_view_keys_round_trip() {
        for view in LibraryView::ALL {
            assert_eq!(LibraryView::from_key(view.key()), view);
        }
        // Anything unknown falls back to the list
        assert_eq!(LibraryView::from_key("mosaic"), LibraryView::List);
        assert_eq!(LibraryView::default(), LibraryView::List);
    }
}
