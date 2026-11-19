//! Liking a song in Canto loves it on Last.fm; unliking unloves it.
//!
//! Looked at before the state takes each action (unliking only carries the
//! song's id, so its title and artist come from what the state still holds).
//! Nothing happens unless Last.fm is connected and the switch is on.

use std::rc::Rc;
use std::sync::Arc;

use canto_api::scrobbler::LastFmClient;
use canto_api::ApiService;
use canto_config::settings::{
    LASTFM_API_KEY, LASTFM_API_SECRET, LASTFM_LOVE_ON_LIKE, LASTFM_SESSION_KEY,
    LASTFM_SESSION_REJECTED, SETTINGS,
};
use gio::prelude::*;

use crate::app::models::Track;
use crate::app::state::{AppState, BrowserAction};
use crate::app::{load, AppAction, AppModel};

/// Songs liked (`love`) or unliked by an action.
#[derive(Debug, PartialEq)]
enum LikeChange {
    Liked(Vec<Track>),
    Unliked(Vec<String>),
}

/// What `action` likes or unlikes, if anything. The selection's songs are
/// asked for only when the action uses them.
fn like_change(action: &AppAction, selection: impl FnOnce() -> Vec<Track>) -> Option<LikeChange> {
    match action {
        AppAction::BrowserAction(BrowserAction::SaveTracks(tracks)) => {
            Some(LikeChange::Liked(tracks.clone()))
        }
        AppAction::BrowserAction(BrowserAction::RemoveSavedTracks(ids)) => {
            Some(LikeChange::Unliked(ids.clone()))
        }
        AppAction::SaveSelection => Some(LikeChange::Liked(selection())),
        AppAction::UnsaveSelection => Some(LikeChange::Unliked(
            selection().into_iter().map(|t| t.rri.id).collect(),
        )),
        _ => None,
    }
}

/// The artist and title Last.fm knows a song by: its main artist, as the
/// Stats tab asks.
fn love_target(track: &Track) -> Option<(String, String)> {
    let artist = track.artists.first()?.name.trim().to_string();
    let title = track.title.trim().to_string();
    (!artist.is_empty() && !title.is_empty()).then_some((artist, title))
}

/// A song the state still knows, by id: playing, selected or among the
/// liked songs loaded so far.
fn find_track(state: &AppState, id: &str) -> Option<Track> {
    if let Some(track) = state.playback.current_song().filter(|t| t.rri.id == id) {
        return Some(track);
    }
    if let Some(track) = state.selection.peek_selection().find(|t| t.rri.id == id) {
        return Some(track.clone());
    }
    state
        .browser
        .home_state()
        .and_then(|home| home.saved_tracks.get(id))
        .map(|song| song.into_description())
}

pub struct LastFmLove {
    app_model: Rc<AppModel>,
    settings: gio::Settings,
}

impl LastFmLove {
    pub fn new(app_model: Rc<AppModel>) -> Self {
        Self {
            app_model,
            settings: gio::Settings::new(SETTINGS),
        }
    }

    /// Sends love or unlove for what `action` likes or unlikes. Call it
    /// before the state takes the action.
    pub fn before(&self, action: &AppAction) {
        let state = self.app_model.get_state();
        let Some(change) = like_change(action, || {
            state.selection.peek_selection().cloned().collect()
        }) else {
            return;
        };
        let s = &self.settings;
        let (api_key, api_secret, session_key) = (
            s.string(LASTFM_API_KEY).trim().to_string(),
            s.string(LASTFM_API_SECRET).trim().to_string(),
            s.string(LASTFM_SESSION_KEY).trim().to_string(),
        );
        let connected = !api_key.is_empty()
            && !api_secret.is_empty()
            && !session_key.is_empty()
            && !s.boolean(LASTFM_SESSION_REJECTED);
        if !connected || !s.boolean(LASTFM_LOVE_ON_LIKE) {
            return;
        }
        let credentials = Credentials {
            api_key,
            api_secret,
            session_key,
        };
        let api = self.app_model.api();
        match change {
            LikeChange::Liked(tracks) => {
                let targets = tracks.iter().filter_map(love_target).collect();
                tokio::spawn(send(api, credentials, targets, true));
            }
            LikeChange::Unliked(ids) => {
                // Songs the state no longer holds are looked up first
                let (known, unknown): (Vec<_>, Vec<_>) = ids
                    .into_iter()
                    .map(|id| (find_track(&state, &id), id))
                    .partition(|(track, _)| track.is_some());
                let targets: Vec<_> = known
                    .iter()
                    .filter_map(|(track, _)| love_target(track.as_ref()?))
                    .collect();
                let unknown: Vec<String> = unknown.into_iter().map(|(_, id)| id).collect();
                let tag = load::visible();
                tokio::spawn(async move {
                    let mut targets = targets;
                    for id in unknown {
                        match api.get_track(&id, tag).await {
                            Ok(track) => targets.extend(love_target(&track)),
                            Err(e) => debug!("Last.fm unlove: song not found: {e}"),
                        }
                    }
                    send(api, credentials, targets, false).await;
                });
            }
        }
    }
}

struct Credentials {
    api_key: String,
    api_secret: String,
    session_key: String,
}

async fn send(
    api: Arc<ApiService>,
    credentials: Credentials,
    targets: Vec<(String, String)>,
    love: bool,
) {
    let what = if love { "love" } else { "unlove" };
    for (artist, title) in targets {
        if let Err(e) = LastFmClient::set_loved(
            api.http_client(),
            &credentials.api_key,
            &credentials.api_secret,
            &credentials.session_key,
            &artist,
            &title,
            love,
        )
        .await
        {
            debug!("Last.fm {what} failed: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::models::{make_track, ArtistRef};

    fn track(id: &str, title: &str, artists: &[&str]) -> Track {
        let mut track = make_track(id);
        track.title = title.to_string();
        track.artists = artists
            .iter()
            .map(|name| {
                let mut rri = make_track(name).rri;
                rri.id = name.to_string();
                ArtistRef {
                    rri,
                    name: name.to_string(),
                }
            })
            .collect();
        track
    }

    #[test]
    fn liking_and_unliking_are_told_apart() {
        let song = track("a", "TODOS ESSES MANOS", &["Niink", "Jotapê"]);
        let liked = like_change(
            &AppAction::BrowserAction(BrowserAction::SaveTracks(vec![song.clone()])),
            Vec::new,
        );
        assert_eq!(liked, Some(LikeChange::Liked(vec![song.clone()])));
        let unliked = like_change(
            &AppAction::BrowserAction(BrowserAction::RemoveSavedTracks(vec!["a".to_string()])),
            Vec::new,
        );
        assert_eq!(unliked, Some(LikeChange::Unliked(vec!["a".to_string()])));
        let selected = vec![song.clone()];
        assert_eq!(
            like_change(&AppAction::SaveSelection, || selected.clone()),
            Some(LikeChange::Liked(vec![song.clone()]))
        );
        assert_eq!(
            like_change(&AppAction::UnsaveSelection, || selected.clone()),
            Some(LikeChange::Unliked(vec!["a".to_string()]))
        );
        assert_eq!(
            like_change(&AppAction::QueueSelection, || panic!("selection read")),
            None
        );
    }

    #[test]
    fn last_fm_gets_the_main_artist_and_the_title() {
        let song = track("a", " Tô Aqui ", &["Niink", "Jotapê"]);
        assert_eq!(
            love_target(&song),
            Some(("Niink".to_string(), "Tô Aqui".to_string()))
        );
        assert_eq!(love_target(&track("b", "Song", &[])), None);
        assert_eq!(love_target(&track("c", "  ", &["Niink"])), None);
    }
}
