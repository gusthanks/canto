//! Playing one song "in its album": the album becomes the queue, starting at
//! that song, as Spotify does when a song is picked outside any list.

use canto_api::{ApiService, DomainError, Load};
use canto_core::play_queue::CONTEXT_PAGE_SIZE;

use crate::app::models::Track;
use crate::app::state::{load_context, PlaybackAction};
use crate::app::{AppAction, SongsSource};

/// The actions that play `track` from its album (or alone, without one).
pub async fn play_in_album_actions(
    api: &ApiService,
    track: Track,
    load: Load,
) -> Result<Vec<AppAction>, DomainError> {
    let id = track.rri.id.clone();
    let mut actions: Vec<AppAction> = vec![PlaybackAction::SetShuffled(false).into()];
    match track
        .album
        .as_ref()
        .map(|a| (a.rri.id.clone(), a.name.clone()))
    {
        Some((album_id, album_name)) => {
            let batch = api
                .get_album_tracks(&album_id, 0, CONTEXT_PAGE_SIZE, load)
                .await?;
            let source = SongsSource::Album(album_id);
            let load = if batch.items.iter().any(|t| t.rri.id == id) {
                PlaybackAction::LoadPagedSongs(source.clone(), batch)
            } else {
                PlaybackAction::LoadContextSongs(source.clone(), vec![track])
            };
            actions.push(PlaybackAction::ReplaceQueue.into());
            actions.extend(load_context(source, Some(album_name), load));
        }
        None => {
            #[allow(deprecated)]
            actions.push(PlaybackAction::LoadSongs(vec![track]).into());
        }
    }
    actions.push(PlaybackAction::Load(id).into());
    Ok(actions)
}
