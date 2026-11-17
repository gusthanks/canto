use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

use canto_api::models::{Page, Track};
use canto_api::{ApiService, DomainError, Load};

use crate::app::components::{dispatch_api_read, dispatch_api_read_many, EventListener};
use crate::app::models::{PageRequest, SongsSource};
use crate::app::state::{LoginEvent, PlaybackAction, PlaybackEvent, StagedPlay};
use crate::app::{AppAction, AppEvent, AppModel, Dispatcher};
use canto_core::play_queue::CONTEXT_PAGE_SIZE;

// An unanswered request is retried after this
const RETRY_AFTER: Duration = Duration::from_secs(10);

async fn get_page(
    api: &ApiService,
    source: &SongsSource,
    offset: usize,
    limit: usize,
    load: Load,
) -> Result<Page<Track>, DomainError> {
    match source {
        SongsSource::Playlist(id) => api.get_playlist_tracks(id, offset, limit, load).await,
        SongsSource::Album(id) => api.get_album_tracks(id, offset, limit, load).await,
        SongsSource::SavedTracks => api.get_saved_tracks(offset, limit, load).await,
        SongsSource::Artist(_) | SongsSource::Search(_) | SongsSource::TrackRadio(_) => {
            unreachable!("{:?} isn't paginated", source)
        }
    }
}

pub fn queue_source_tracks(app_model: &AppModel, dispatcher: &Dispatcher, source: SongsSource) {
    let api = app_model.api();
    dispatch_api_read(dispatcher, move |tag| async move {
        let mut tracks = vec![];
        loop {
            let page = get_page(&api, &source, tracks.len(), CONTEXT_PAGE_SIZE, tag).await?;
            let len = page.items.len();
            tracks.extend(page.items);
            if len < CONTEXT_PAGE_SIZE || page.total.is_some_and(|t| tracks.len() >= t) {
                break;
            }
        }
        tracks.retain(|t| t.playable);
        Ok(AppAction::QueueTracks { tracks })
    });
}

pub fn fetch_queue_page(
    app_model: &AppModel,
    dispatcher: &Dispatcher,
    source: SongsSource,
    request: PageRequest,
) {
    let api = app_model.api();
    let PageRequest { offset, batch_size } = request;
    debug!("loading queue page source={source:?} offset={offset} size={batch_size}");
    dispatch_api_read(dispatcher, move |tag| async move {
        let page = get_page(&api, &source, offset, batch_size, tag).await?;
        Ok(PlaybackAction::LoadPagedSongs(source, page).into())
    });
}

// Pages of an album or playlist scanned for the last track (at most)
const LAST_PLAYED_PAGES: usize = 20;

fn source_of_context(uri: &str) -> Option<SongsSource> {
    let mut parts = uri.split(':');
    if parts.next() != Some("spotify") {
        return None;
    }
    match (parts.next()?, parts.next_back()?) {
        ("album", id) => Some(SongsSource::Album(id.to_string())),
        ("playlist", id) => Some(SongsSource::Playlist(id.to_string())),
        // Liked Songs: spotify:user:<name>:collection or spotify:collection
        ("user" | "collection", "collection") => Some(SongsSource::SavedTracks),
        _ => None,
    }
}

/// The page of `source` holding the track, if it is there.
async fn page_with(
    api: &ApiService,
    source: &SongsSource,
    track_id: &str,
    load: Load,
) -> Result<Option<Page<Track>>, DomainError> {
    for n in 0..LAST_PLAYED_PAGES {
        let offset = n * CONTEXT_PAGE_SIZE;
        let page = get_page(api, source, offset, CONTEXT_PAGE_SIZE, load).await?;
        let len = page.items.len();
        if page.items.iter().any(|t| t.rri.id == track_id) {
            return Ok(Some(Page {
                offset: Some(offset),
                ..page
            }));
        }
        if len < CONTEXT_PAGE_SIZE || page.total.is_some_and(|t| offset + len >= t) {
            break;
        }
    }
    Ok(None)
}

/// Shows the account's last track, paused, inside the album or playlist it
/// played from (its album when that is unknown), where it stopped.
pub fn stage_last_played(app_model: &AppModel, dispatcher: &Dispatcher) {
    let api = app_model.api();
    dispatch_api_read_many(dispatcher, move |tag| async move {
        let Some(last) = api.get_last_played(tag).await? else {
            debug!("last played: none, or playing elsewhere");
            return Ok(vec![]);
        };
        let mut found = None;
        if let Some(source) = last.context_uri.as_deref().and_then(source_of_context) {
            match page_with(&api, &source, &last.track_id, tag).await {
                Ok(Some(page)) => found = Some((source, page)),
                Ok(None) => debug!("last played: not in {source:?} (anymore)"),
                // A playlist gone private, say: its album still works
                Err(err) => debug!("last played: {source:?} failed: {err:?}"),
            }
        }
        if found.is_none() {
            let track = api.get_track(&last.track_id, tag).await?;
            if let Some(album) = track.album {
                let source = SongsSource::Album(album.rri.id);
                if let Some(page) = page_with(&api, &source, &last.track_id, tag).await? {
                    found = Some((source, page));
                }
            }
        }
        let Some((source, page)) = found else {
            return Ok(vec![]);
        };
        let staged = StagedPlay {
            source,
            page,
            track_id: last.track_id,
            position_ms: last.position_ms,
        };
        Ok(vec![
            PlaybackAction::StageLastPlayed(Box::new(staged)).into()
        ])
    });
}

pub struct QueueLoader {
    app_model: Rc<AppModel>,
    dispatcher: Dispatcher,
    last_request: RefCell<Option<(SongsSource, usize, Instant)>>,
    // The last track is looked up once, on the first login of the run
    last_played_asked: bool,
}

impl QueueLoader {
    pub fn new(app_model: Rc<AppModel>, dispatcher: Dispatcher) -> Self {
        Self {
            app_model,
            dispatcher,
            last_request: RefCell::new(None),
            last_played_asked: false,
        }
    }

    fn load_next_page(&self) {
        let query = self.app_model.get_state().playback.next_query(false);
        let Some((source, request)) = query else {
            return;
        };
        let pending = self
            .last_request
            .borrow()
            .as_ref()
            .is_some_and(|(s, offset, at)| {
                *s == source && *offset == request.offset && at.elapsed() < RETRY_AFTER
            });
        if pending {
            return;
        }
        self.last_request
            .replace(Some((source.clone(), request.offset, Instant::now())));
        fetch_queue_page(&self.app_model, &self.dispatcher, source, request);
    }
}

impl EventListener for QueueLoader {
    fn on_event(&mut self, event: &AppEvent) {
        if let AppEvent::LoginEvent(LoginEvent::LoginCompleted) = event {
            if !self.last_played_asked {
                self.last_played_asked = true;
                stage_last_played(&self.app_model, &self.dispatcher);
            }
            return;
        }
        // Not on PlaylistChanged, which loading a page sends
        if let AppEvent::PlaybackEvent(
            PlaybackEvent::TrackChanged(_)
            | PlaybackEvent::SourceChanged
            | PlaybackEvent::RepeatModeChanged(_)
            | PlaybackEvent::ShuffleChanged(_),
        ) = event
        {
            self.load_next_page();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_source_of_context() {
        assert_eq!(
            source_of_context("spotify:album:abc"),
            Some(SongsSource::Album("abc".to_string()))
        );
        assert_eq!(
            source_of_context("spotify:playlist:xyz"),
            Some(SongsSource::Playlist("xyz".to_string()))
        );
        assert_eq!(
            source_of_context("spotify:user:gus:collection"),
            Some(SongsSource::SavedTracks)
        );
        assert_eq!(source_of_context("spotify:artist:abc"), None);
        assert_eq!(source_of_context("spotify:show:abc"), None);
    }
}
