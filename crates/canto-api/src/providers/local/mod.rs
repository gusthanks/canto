//! Local filesystem music library provider.
//!
//! Recursively scans user-selected music folders for `.mp3`, `.flac`, `.ogg`,
//! `.oga`, and `.wav` files, extracts tags / durations / embedded cover art
//! using `symphonia` (plus a built-in RIFF/WAVE header parser), falls back to
//! directory structure and folder artwork (`cover.*`, `folder.*`, `front.*`),
//! and persists likes and playlists locally.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, MetadataRevision, StandardTagKey};
use symphonia::core::probe::Hint;
use url::Url;

use super::provider::{MusicProvider, ProviderCapabilities, ProviderKind};
use crate::defaults::{self, EntityKind};
use crate::error::DomainError;
use crate::models::*;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct PersistedPlaylist {
    id: String,
    title: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    track_ids: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LocalUserState {
    #[serde(default)]
    liked_track_ids: Vec<String>,
    #[serde(default)]
    saved_album_ids: Vec<String>,
    #[serde(default)]
    playlists: Vec<PersistedPlaylist>,
}

#[derive(Clone, Default)]
struct LocalIndex {
    tracks: HashMap<String, Track>,
    track_paths: HashMap<String, PathBuf>,
    track_order: Vec<String>,
    albums: HashMap<String, Album>,
    album_tracks: HashMap<String, Vec<String>>,
    album_order: Vec<String>,
    artists: HashMap<String, Artist>,
    artist_albums: HashMap<String, Vec<String>>,
    artist_tracks: HashMap<String, Vec<String>>,
    artist_order: Vec<String>,
}

pub struct LocalProvider {
    folders: RwLock<Vec<PathBuf>>,
    state_path: PathBuf,
    covers_dir: PathBuf,
    index: RwLock<LocalIndex>,
    user_state: RwLock<LocalUserState>,
}

pub fn local_provider(folders: Vec<PathBuf>) -> Arc<dyn MusicProvider> {
    Arc::new(LocalProvider::new(folders))
}

impl LocalProvider {
    pub fn new(folders: Vec<PathBuf>) -> Self {
        let state_path = glib::user_data_dir()
            .join("canto")
            .join("local-library-state.json");
        let covers_dir = glib::user_cache_dir().join("canto").join("local-covers");
        Self::new_with_paths(folders, state_path, covers_dir)
    }

    pub fn new_with_paths(folders: Vec<PathBuf>, state_path: PathBuf, covers_dir: PathBuf) -> Self {
        let user_state = load_user_state(&state_path);
        let provider = Self {
            folders: RwLock::new(folders),
            state_path,
            covers_dir,
            index: RwLock::new(LocalIndex::default()),
            user_state: RwLock::new(user_state),
        };
        provider.rebuild_index();
        provider
    }

    pub fn folders(&self) -> Vec<PathBuf> {
        self.folders.read().unwrap().clone()
    }

    pub fn set_folders(&self, folders: Vec<PathBuf>) {
        *self.folders.write().unwrap() = folders;
        self.rebuild_index();
    }

    pub fn rebuild_index(&self) {
        let folders = self.folders.read().unwrap().clone();
        let next_index = scan_folders(&folders, &self.covers_dir);
        *self.index.write().unwrap() = next_index;
    }

    fn persist_state(&self, state: &LocalUserState) {
        if let Some(parent) = self.state_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Ok(json) = serde_json::to_vec_pretty(state) {
            let _ = std::fs::write(&self.state_path, json);
        }
    }

    fn paginate<T: Clone>(items: &[T], offset: usize, limit: usize) -> Page<T> {
        let total = items.len();
        let slice = if offset >= total {
            Vec::new()
        } else {
            let end = (offset + limit).min(total);
            items[offset..end].to_vec()
        };
        Page::offset_paged(slice, offset, total)
    }

    fn decorate_track(&self, track: &Track, state: &LocalUserState) -> Track {
        let mut out = track.clone();
        out.saved = Some(state.liked_track_ids.iter().any(|id| id == &track.rri.id));
        out
    }

    fn decorate_album(&self, album: &Album, state: &LocalUserState) -> Album {
        let mut out = album.clone();
        // Every scanned local album appears in the user's library, while allowing
        // explicit heart/bookmark toggling persisted in LocalUserState.
        out.saved = Some(
            state.saved_album_ids.is_empty()
                || state.saved_album_ids.iter().any(|id| id == &album.rri.id),
        );
        out
    }

    fn to_playlist(
        &self,
        pl: &PersistedPlaylist,
        idx: &LocalIndex,
        state: &LocalUserState,
    ) -> Playlist {
        let first_art = pl
            .track_ids
            .iter()
            .find_map(|tid| idx.tracks.get(tid))
            .map(|t| t.art.clone())
            .filter(|a| !a.is_resource())
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Playlist));
        let items: Vec<PlaylistItem> = pl
            .track_ids
            .iter()
            .enumerate()
            .filter_map(|(pos, tid)| {
                idx.tracks.get(tid).map(|t| PlaylistItem {
                    track: self.decorate_track(t, state),
                    added_at: None,
                    added_by: None,
                    position: Some(pos),
                })
            })
            .collect();
        Playlist {
            rri: ResourceId::new(Provider::Local, &pl.id),
            title: pl.title.clone(),
            description: pl.description.clone(),
            owner: Some(UserRef {
                rri: ResourceId::new(Provider::Local, "local"),
                display_name: "Local Library".to_string(),
            }),
            art: first_art,
            total_tracks: Some(items.len() as u32),
            collaborative: Some(false),
            public: Some(false),
            version: None,
            tracks: Some(Page::offset_paged(items.clone(), 0, items.len())),
            following: Some(true),
            url: None,
        }
    }
}

#[async_trait]
impl MusicProvider for LocalProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Local
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::local()
    }

    fn stream_url(&self, track_id: &str) -> Option<String> {
        let idx = self.index.read().unwrap();
        idx.track_paths
            .get(track_id)
            .map(|p| p.to_string_lossy().into_owned())
    }

    async fn rescan(&self) -> Result<(), DomainError> {
        let folders = self.folders();
        let covers_dir = self.covers_dir.clone();
        let scanned = tokio::task::spawn_blocking(move || scan_folders(&folders, &covers_dir))
            .await
            .map_err(|e| DomainError::Parse(format!("local scan task failed: {e}")))?;
        *self.index.write().unwrap() = scanned;
        Ok(())
    }

    async fn get_recommendations_for_track(
        &self,
        track_id: &str,
        limit: usize,
    ) -> Result<Vec<Track>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let Some(seed) = idx.tracks.get(track_id) else {
            return Ok(Vec::new());
        };
        let seed_artist = seed.artists.first().map(|a| a.rri.id.clone());
        let seed_album = seed.album.as_ref().map(|a| a.rri.id.clone());

        let mut recs = Vec::new();
        let mut seen = HashSet::new();
        seen.insert(track_id.to_string());

        if let Some(artist_id) = seed_artist {
            if let Some(tids) = idx.artist_tracks.get(&artist_id) {
                for tid in tids {
                    if seen.insert(tid.clone()) {
                        if let Some(t) = idx.tracks.get(tid) {
                            recs.push(self.decorate_track(t, &state));
                            if recs.len() >= limit {
                                return Ok(recs);
                            }
                        }
                    }
                }
            }
        }
        if let Some(album_id) = seed_album {
            if let Some(tids) = idx.album_tracks.get(&album_id) {
                for tid in tids {
                    if seen.insert(tid.clone()) {
                        if let Some(t) = idx.tracks.get(tid) {
                            recs.push(self.decorate_track(t, &state));
                            if recs.len() >= limit {
                                return Ok(recs);
                            }
                        }
                    }
                }
            }
        }
        for tid in &idx.track_order {
            if seen.insert(tid.clone()) {
                if let Some(t) = idx.tracks.get(tid) {
                    recs.push(self.decorate_track(t, &state));
                    if recs.len() >= limit {
                        break;
                    }
                }
            }
        }
        Ok(recs)
    }

    async fn get_album(&self, id: &str) -> Result<Album, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let Some(album) = idx.albums.get(id) else {
            return Err(DomainError::NotFound {
                resource: format!("local album {id}"),
            });
        };
        let mut out = self.decorate_album(album, &state);
        if let Some(tids) = idx.album_tracks.get(id) {
            let tracks: Vec<Track> = tids
                .iter()
                .filter_map(|tid| idx.tracks.get(tid).map(|t| self.decorate_track(t, &state)))
                .collect();
            let total = tracks.len();
            out.tracks = Some(Page::offset_paged(tracks, 0, total));
        }
        Ok(out)
    }

    async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let Some(tids) = idx.album_tracks.get(id) else {
            return Err(DomainError::NotFound {
                resource: format!("local album {id}"),
            });
        };
        let tracks: Vec<Track> = tids
            .iter()
            .filter_map(|tid| idx.tracks.get(tid).map(|t| self.decorate_track(t, &state)))
            .collect();
        Ok(Self::paginate(&tracks, offset, limit))
    }

    async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let albums: Vec<Album> = idx
            .album_order
            .iter()
            .filter_map(|aid| idx.albums.get(aid).map(|a| self.decorate_album(a, &state)))
            .collect();
        Ok(Self::paginate(&albums, offset, limit))
    }

    async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let state = self.user_state.read().unwrap();
        Ok(ids
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|id| {
                state.saved_album_ids.is_empty()
                    || state.saved_album_ids.iter().any(|saved| saved == id)
            })
            .collect())
    }

    async fn save_albums(&self, ids: &str) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        for id in ids.split(',').filter(|s| !s.is_empty()) {
            if !state.saved_album_ids.iter().any(|s| s == id) {
                state.saved_album_ids.push(id.to_string());
            }
        }
        self.persist_state(&state);
        Ok(())
    }

    async fn remove_albums(&self, ids: &str) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        let drop_set: HashSet<&str> = ids.split(',').filter(|s| !s.is_empty()).collect();
        state
            .saved_album_ids
            .retain(|id| !drop_set.contains(id.as_str()));
        self.persist_state(&state);
        Ok(())
    }

    async fn get_track(&self, id: &str) -> Result<Track, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        idx.tracks
            .get(id)
            .map(|t| self.decorate_track(t, &state))
            .ok_or_else(|| DomainError::NotFound {
                resource: format!("local track {id}"),
            })
    }

    async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        // When user has explicitly liked tracks, show those in Liked Songs;
        // otherwise return an empty liked collection so likes stay meaningful.
        let tracks: Vec<Track> = state
            .liked_track_ids
            .iter()
            .rev()
            .filter_map(|tid| idx.tracks.get(tid).map(|t| self.decorate_track(t, &state)))
            .collect();
        Ok(Self::paginate(&tracks, offset, limit))
    }

    async fn check_saved_tracks(&self, ids: Vec<String>) -> Result<Vec<bool>, DomainError> {
        let state = self.user_state.read().unwrap();
        Ok(ids
            .into_iter()
            .map(|id| state.liked_track_ids.iter().any(|s| s == &id))
            .collect())
    }

    async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        for id in ids {
            if !state.liked_track_ids.iter().any(|s| s == &id) {
                state.liked_track_ids.push(id);
            }
        }
        self.persist_state(&state);
        Ok(())
    }

    async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        let drop_set: HashSet<&str> = ids.iter().map(String::as_str).collect();
        state
            .liked_track_ids
            .retain(|id| !drop_set.contains(id.as_str()));
        self.persist_state(&state);
        Ok(())
    }

    async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let playlists: Vec<Playlist> = state
            .playlists
            .iter()
            .map(|p| self.to_playlist(p, &idx, &state))
            .collect();
        Ok(Self::paginate(&playlists, offset, limit))
    }

    async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        state
            .playlists
            .iter()
            .find(|p| p.id == id)
            .map(|p| self.to_playlist(p, &idx, &state))
            .ok_or_else(|| DomainError::NotFound {
                resource: format!("local playlist {id}"),
            })
    }

    async fn get_playlist_item_uris(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Option<String>>, usize), DomainError> {
        let state = self.user_state.read().unwrap();
        let Some(pl) = state.playlists.iter().find(|p| p.id == id) else {
            return Err(DomainError::NotFound {
                resource: format!("local playlist {id}"),
            });
        };
        let total = pl.track_ids.len();
        let items = if offset >= total {
            Vec::new()
        } else {
            pl.track_ids[offset..(offset + limit).min(total)]
                .iter()
                .map(|tid| Some(tid.clone()))
                .collect()
        };
        Ok((items, total))
    }

    async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let Some(pl) = state.playlists.iter().find(|p| p.id == id) else {
            return Err(DomainError::NotFound {
                resource: format!("local playlist {id}"),
            });
        };
        let tracks: Vec<Track> = pl
            .track_ids
            .iter()
            .filter_map(|tid| idx.tracks.get(tid).map(|t| self.decorate_track(t, &state)))
            .collect();
        Ok(Self::paginate(&tracks, offset, limit))
    }

    async fn add_to_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        position: Option<i32>,
    ) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        let Some(pl) = state.playlists.iter_mut().find(|p| p.id == id) else {
            return Err(DomainError::NotFound {
                resource: format!("local playlist {id}"),
            });
        };
        let insert_at = position
            .and_then(|p| usize::try_from(p).ok())
            .map(|p| p.min(pl.track_ids.len()))
            .unwrap_or(pl.track_ids.len());
        for (i, uri) in uris.into_iter().enumerate() {
            pl.track_ids.insert(insert_at + i, uri);
        }
        self.persist_state(&state);
        Ok(())
    }

    async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        _snapshot_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        let Some(pl) = state.playlists.iter_mut().find(|p| p.id == id) else {
            return Err(DomainError::NotFound {
                resource: format!("local playlist {id}"),
            });
        };
        let drop_set: HashSet<String> = uris.into_iter().collect();
        pl.track_ids.retain(|tid| !drop_set.contains(tid));
        self.persist_state(&state);
        Ok(())
    }

    async fn create_playlist(
        &self,
        _user_id: &str,
        name: &str,
        _public: Option<bool>,
        _collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<Playlist, DomainError> {
        let idx = self.index.read().unwrap();
        let mut state = self.user_state.write().unwrap();
        let id = short_id("loc_p", &format!("{}:{}", name, state.playlists.len()));
        let pl = PersistedPlaylist {
            id,
            title: name.to_string(),
            description: description.map(str::to_string),
            track_ids: Vec::new(),
        };
        let out = self.to_playlist(&pl, &idx, &state);
        state.playlists.push(pl);
        self.persist_state(&state);
        Ok(out)
    }

    async fn upload_playlist_cover(
        &self,
        _id: &str,
        _jpeg_base64: String,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn follow_playlist(&self, _id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn unfollow_playlist(&self, id: &str) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        state.playlists.retain(|p| p.id != id);
        self.persist_state(&state);
        Ok(())
    }

    async fn update_playlist_details(
        &self,
        id: &str,
        name: Option<&str>,
        _public: Option<bool>,
        _collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut state = self.user_state.write().unwrap();
        if let Some(pl) = state.playlists.iter_mut().find(|p| p.id == id) {
            if let Some(name) = name {
                pl.title = name.to_string();
            }
            if let Some(desc) = description {
                pl.description = if desc.is_empty() {
                    None
                } else {
                    Some(desc.to_string())
                };
            }
        }
        self.persist_state(&state);
        Ok(())
    }

    async fn set_playlist_public(&self, _id: &str, _public: bool) -> Result<(), DomainError> {
        Ok(())
    }

    async fn get_artist(&self, id: &str) -> Result<Artist, DomainError> {
        let idx = self.index.read().unwrap();
        idx.artists
            .get(id)
            .cloned()
            .ok_or_else(|| DomainError::NotFound {
                resource: format!("local artist {id}"),
            })
    }

    async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        let albums: Vec<Album> = idx
            .artist_albums
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|aid| idx.albums.get(aid).map(|a| self.decorate_album(a, &state)))
            .collect();
        Ok(Self::paginate(&albums, offset, limit))
    }

    async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError> {
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();
        Ok(idx
            .artist_tracks
            .get(id)
            .into_iter()
            .flatten()
            .filter_map(|tid| idx.tracks.get(tid).map(|t| self.decorate_track(t, &state)))
            .collect())
    }

    async fn get_followed_artists(
        &self,
        _after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError> {
        let idx = self.index.read().unwrap();
        let artists: Vec<Artist> = idx
            .artist_order
            .iter()
            .take(limit)
            .filter_map(|id| idx.artists.get(id).cloned())
            .collect();
        Ok((artists, None))
    }

    async fn follow_artists(&self, _ids: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn unfollow_artists(&self, _ids: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn check_following_artists(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let count = ids.split(',').filter(|s| !s.is_empty()).count();
        Ok(vec![true; count])
    }

    async fn get_current_user(&self) -> Result<User, DomainError> {
        Ok(User {
            rri: ResourceId::new(Provider::Local, "local"),
            display_name: "Local Library".to_string(),
            art: defaults::image_set_for(EntityKind::User),
            follower_count: None,
            product: Some("local".to_string()),
            country: None,
            explicit_filter: None,
            url: None,
        })
    }

    async fn get_user(&self, id: &str) -> Result<User, DomainError> {
        Ok(User {
            rri: ResourceId::new(Provider::Local, id),
            display_name: "Local Library".to_string(),
            art: defaults::image_set_for(EntityKind::User),
            follower_count: None,
            product: Some("local".to_string()),
            country: None,
            explicit_filter: None,
            url: None,
        })
    }

    async fn get_user_playlists(
        &self,
        _id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        self.get_saved_playlists(offset, limit).await
    }

    async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let q = query.trim().to_lowercase();
        let idx = self.index.read().unwrap();
        let state = self.user_state.read().unwrap();

        let matched_tracks: Vec<Track> = idx
            .track_order
            .iter()
            .filter_map(|id| idx.tracks.get(id))
            .filter(|t| {
                t.title.to_lowercase().contains(&q)
                    || t.artists.iter().any(|a| a.name.to_lowercase().contains(&q))
                    || t.album
                        .as_ref()
                        .is_some_and(|a| a.name.to_lowercase().contains(&q))
            })
            .map(|t| self.decorate_track(t, &state))
            .collect();

        let matched_albums: Vec<Album> = idx
            .album_order
            .iter()
            .filter_map(|id| idx.albums.get(id))
            .filter(|a| {
                a.title.to_lowercase().contains(&q)
                    || a.artists
                        .iter()
                        .any(|ar| ar.name.to_lowercase().contains(&q))
            })
            .map(|a| self.decorate_album(a, &state))
            .collect();

        let matched_artists: Vec<Artist> = idx
            .artist_order
            .iter()
            .filter_map(|id| idx.artists.get(id))
            .filter(|a| a.name.to_lowercase().contains(&q))
            .cloned()
            .collect();

        let matched_playlists: Vec<Playlist> = state
            .playlists
            .iter()
            .filter(|p| p.title.to_lowercase().contains(&q))
            .map(|p| self.to_playlist(p, &idx, &state))
            .collect();

        Ok(SearchResults {
            tracks: Self::paginate(&matched_tracks, offset, limit),
            albums: Self::paginate(&matched_albums, offset, limit),
            artists: Self::paginate(&matched_artists, offset, limit),
            playlists: Self::paginate(&matched_playlists, offset, limit),
        })
    }

    async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError> {
        let all = self.search(query, offset, limit).await?;
        Ok(match kind {
            SearchType::Track => SearchResults {
                tracks: all.tracks,
                ..Default::default()
            },
            SearchType::Album => SearchResults {
                albums: all.albums,
                ..Default::default()
            },
            SearchType::Artist => SearchResults {
                artists: all.artists,
                ..Default::default()
            },
            SearchType::Playlist => SearchResults {
                playlists: all.playlists,
                ..Default::default()
            },
        })
    }

    async fn get_devices(&self) -> Result<Vec<Device>, DomainError> {
        Ok(Vec::new())
    }

    async fn get_player_queue(&self) -> Result<Queue, DomainError> {
        Ok(Queue::default())
    }

    async fn get_player_state(&self) -> Result<PlayerState, DomainError> {
        Ok(PlayerState::default())
    }

    async fn player_resume(&self, _device_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_play_in_context(
        &self,
        _device_id: &str,
        _context_uri: &str,
        _offset: usize,
        _position_ms: Option<u32>,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_play_uris(
        &self,
        _device_id: &str,
        _uris: Vec<String>,
        _offset: usize,
        _position_ms: Option<u32>,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_pause(&self, _device_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_seek(&self, _device_id: &str, _position_ms: u32) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_repeat(&self, _device_id: &str, _mode: RepeatMode) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_shuffle(&self, _device_id: &str, _state: bool) -> Result<(), DomainError> {
        Ok(())
    }

    async fn player_volume(
        &self,
        _device_id: &str,
        _volume_percent: u8,
    ) -> Result<(), DomainError> {
        Ok(())
    }

    fn invalidate_config(&self) {}
}

fn load_user_state(path: &Path) -> LocalUserState {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<LocalUserState>(&bytes).ok())
        .unwrap_or_default()
}

fn short_id(prefix: &str, key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(key.as_bytes());
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(prefix.len() + 17);
    hex.push_str(prefix);
    hex.push('_');
    for byte in &digest[..8] {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

fn is_supported_audio(path: &Path) -> bool {
    let Some(ext) = path.extension().and_then(|e| e.to_str()) else {
        return false;
    };
    matches!(
        ext.to_ascii_lowercase().as_str(),
        "mp3" | "flac" | "ogg" | "oga" | "wav"
    )
}

#[derive(Default)]
struct ScannedFileMeta {
    title: Option<String>,
    artist: Option<String>,
    album: Option<String>,
    track_number: Option<u32>,
    disc_number: Option<u32>,
    year: Option<i32>,
    duration_ms: u32,
    cover_url: Option<String>,
}

fn scan_folders(folders: &[PathBuf], covers_dir: &Path) -> LocalIndex {
    let mut index = LocalIndex::default();
    let mut visited_dirs: HashSet<PathBuf> = HashSet::new();
    let mut dir_covers: HashMap<PathBuf, Option<String>> = HashMap::new();
    let mut audio_files: Vec<PathBuf> = Vec::new();

    let mut stack: Vec<PathBuf> = folders.to_vec();
    while let Some(dir) = stack.pop() {
        let Ok(canon) = std::fs::canonicalize(&dir) else {
            continue;
        };
        if !visited_dirs.insert(canon.clone()) {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(&canon) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
        children.sort();
        for path in children {
            let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
                continue;
            };
            if name.starts_with('.') {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
            } else if path.is_file() && is_supported_audio(&path) {
                audio_files.push(path);
            }
        }
    }

    let mut album_artists_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for path in audio_files {
        let meta = inspect_audio_file(&path, covers_dir);
        let title = meta
            .title
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| {
                path.file_stem()
                    .and_then(|s| s.to_str())
                    .map(str::to_string)
                    .unwrap_or_else(defaults::track_title)
            });

        let parent = path.parent().map(Path::to_path_buf);
        let grandparent = parent
            .as_deref()
            .and_then(Path::parent)
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .map(str::to_string);

        let artist_name = meta
            .artist
            .filter(|s| !s.trim().is_empty())
            .or(grandparent)
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(defaults::artist_name);

        let album_title = meta
            .album
            .filter(|s| !s.trim().is_empty())
            .or_else(|| {
                parent
                    .as_deref()
                    .and_then(Path::file_name)
                    .and_then(|n| n.to_str())
                    .map(str::to_string)
            })
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(defaults::album_title);

        let folder_cover = parent.as_ref().and_then(|dir| {
            dir_covers
                .entry(dir.clone())
                .or_insert_with(|| find_folder_cover(dir))
                .clone()
        });
        let cover_url = meta.cover_url.or(folder_cover);

        let artist_id = short_id("loc_r", &artist_name.to_lowercase());
        let album_key = format!(
            "{}::{}",
            artist_name.to_lowercase(),
            album_title.to_lowercase()
        );
        let album_id = short_id("loc_a", &album_key);
        let track_id = short_id("loc_t", &path.to_string_lossy());

        let artist_art = cover_url
            .as_deref()
            .map(image_set_from_url)
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Artist));
        index.artists.entry(artist_id.clone()).or_insert_with(|| {
            index.artist_order.push(artist_id.clone());
            Artist {
                rri: ResourceId::new(Provider::Local, &artist_id),
                name: artist_name.clone(),
                art: artist_art,
                genres: Vec::new(),
                popularity: None,
                follower_count: None,
                bio: None,
                following: Some(true),
                url: None,
            }
        });

        let artist_ref = ArtistRef {
            rri: ResourceId::new(Provider::Local, &artist_id),
            name: artist_name,
        };
        let album_art = cover_url
            .as_deref()
            .map(image_set_from_url)
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Album));
        let release_date = meta.year.map(|year| ReleaseDate {
            year,
            month: None,
            day: None,
        });

        index.albums.entry(album_id.clone()).or_insert_with(|| {
            index.album_order.push(album_id.clone());
            Album {
                rri: ResourceId::new(Provider::Local, &album_id),
                title: album_title.clone(),
                artists: vec![artist_ref.clone()],
                art: album_art.clone(),
                album_type: AlbumType::Album,
                release_date,
                total_tracks: Some(0),
                label: None,
                copyright: None,
                upc: None,
                genres: Vec::new(),
                popularity: None,
                content_rating: ContentRating::None,
                saved: Some(true),
                tracks: None,
                url: None,
            }
        });

        let track_art = cover_url
            .as_deref()
            .map(image_set_from_url)
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Track));

        let track = Track {
            rri: ResourceId::new(Provider::Local, &track_id),
            title,
            artists: vec![artist_ref],
            album: Some(AlbumRef {
                rri: ResourceId::new(Provider::Local, &album_id),
                name: album_title,
            }),
            duration_ms: meta.duration_ms,
            track_number: meta.track_number,
            disc_number: meta.disc_number,
            content_rating: ContentRating::None,
            isrc: None,
            art: track_art,
            playable: true,
            popularity: None,
            saved: Some(false),
            preview_url: None,
            url: None,
        };

        index.tracks.insert(track_id.clone(), track);
        index.track_paths.insert(track_id.clone(), path);
        index.track_order.push(track_id.clone());
        album_artists_map
            .entry(album_id.clone())
            .or_default()
            .push(track_id.clone());
        index
            .artist_tracks
            .entry(artist_id.clone())
            .or_default()
            .push(track_id);
        let artist_albs = index.artist_albums.entry(artist_id).or_default();
        if !artist_albs.contains(&album_id) {
            artist_albs.push(album_id);
        }
    }

    for (album_id, mut tids) in album_artists_map {
        tids.sort_by_key(|tid| {
            let t = &index.tracks[tid];
            (
                t.disc_number.unwrap_or(1),
                t.track_number.unwrap_or(u32::MAX),
                t.title.clone(),
            )
        });
        if let Some(album) = index.albums.get_mut(&album_id) {
            album.total_tracks = Some(tids.len() as u32);
        }
        index.album_tracks.insert(album_id, tids);
    }

    index.album_order.sort_by(|a, b| {
        index.albums[a]
            .title
            .to_lowercase()
            .cmp(&index.albums[b].title.to_lowercase())
    });
    index.artist_order.sort_by(|a, b| {
        index.artists[a]
            .name
            .to_lowercase()
            .cmp(&index.artists[b].name.to_lowercase())
    });

    index
}

fn image_set_from_url(url: &str) -> ImageSet {
    ImageSet {
        images: vec![Image {
            url: url.to_string(),
            width: None,
            height: None,
        }],
        template: None,
    }
}

fn find_folder_cover(dir: &Path) -> Option<String> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        if matches!(stem.as_str(), "cover" | "folder" | "front" | "album")
            && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp")
        {
            if let Ok(url) = Url::from_file_path(&path) {
                return Some(url.into());
            }
        }
    }
    None
}

fn inspect_audio_file(path: &Path, covers_dir: &Path) -> ScannedFileMeta {
    if path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return ScannedFileMeta {
            duration_ms: parse_wav_duration_ms(path).unwrap_or(0),
            ..Default::default()
        };
    }

    let Ok(file) = File::open(path) else {
        return ScannedFileMeta::default();
    };
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let Ok(mut probed) = symphonia::default::get_probe().format(
        &hint,
        mss,
        &FormatOptions::default(),
        &MetadataOptions::default(),
    ) else {
        return ScannedFileMeta::default();
    };

    let mut meta = ScannedFileMeta::default();
    if let Some(track) = probed.format.default_track() {
        let params = &track.codec_params;
        if let (Some(n_frames), Some(rate)) = (params.n_frames, params.sample_rate) {
            if rate > 0 {
                meta.duration_ms = ((n_frames * 1000) / rate as u64) as u32;
            }
        }
    }

    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        apply_revision(&mut meta, rev, covers_dir);
    }
    if let Some(rev) = probed.format.metadata().current() {
        apply_revision(&mut meta, rev, covers_dir);
    }
    meta
}

fn apply_revision(meta: &mut ScannedFileMeta, rev: &MetadataRevision, covers_dir: &Path) {
    for tag in rev.tags() {
        let val = tag.value.to_string();
        match tag.std_key {
            Some(StandardTagKey::TrackTitle) if meta.title.is_none() => meta.title = Some(val),
            Some(StandardTagKey::Artist) if meta.artist.is_none() => meta.artist = Some(val),
            Some(StandardTagKey::AlbumArtist) if meta.artist.is_none() => meta.artist = Some(val),
            Some(StandardTagKey::Album) if meta.album.is_none() => meta.album = Some(val),
            Some(StandardTagKey::TrackNumber) if meta.track_number.is_none() => {
                let num = val
                    .split('/')
                    .next()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                meta.track_number = num;
            }
            Some(StandardTagKey::DiscNumber) if meta.disc_number.is_none() => {
                let num = val
                    .split('/')
                    .next()
                    .and_then(|s| s.trim().parse::<u32>().ok());
                meta.disc_number = num;
            }
            Some(StandardTagKey::Date | StandardTagKey::ReleaseDate) if meta.year.is_none() => {
                meta.year = val.get(..4).and_then(|y| y.parse::<i32>().ok());
            }
            _ => {}
        }
    }
    if meta.cover_url.is_none() {
        if let Some(visual) = rev.visuals().first() {
            if !visual.data.is_empty() {
                meta.cover_url = cache_embedded_cover(covers_dir, &visual.data, &visual.media_type);
            }
        }
    }
}

fn cache_embedded_cover(covers_dir: &Path, data: &[u8], media_type: &str) -> Option<String> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let digest = hasher.finalize();
    let mut hex = String::with_capacity(32);
    for b in &digest[..16] {
        hex.push_str(&format!("{b:02x}"));
    }
    let ext = match media_type {
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "jpg",
    };
    let path = covers_dir.join(format!("{hex}.{ext}"));
    if !path.exists() {
        std::fs::create_dir_all(covers_dir).ok()?;
        std::fs::write(&path, data).ok()?;
    }
    Url::from_file_path(&path).ok().map(String::from)
}

/// Minimal RIFF/WAVE header parser returning duration in milliseconds.
pub fn parse_wav_duration_ms(path: &Path) -> Option<u32> {
    let mut f = File::open(path).ok()?;
    let mut hdr = [0u8; 12];
    f.read_exact(&mut hdr).ok()?;
    if &hdr[0..4] != b"RIFF" || &hdr[8..12] != b"WAVE" {
        return None;
    }

    let mut byte_rate: u32 = 0;
    let mut data_bytes: u32 = 0;
    let mut chunk_hdr = [0u8; 8];
    while f.read_exact(&mut chunk_hdr).is_ok() {
        let size = u32::from_le_bytes(chunk_hdr[4..8].try_into().ok()?);
        if &chunk_hdr[0..4] == b"fmt " {
            let read_len = size.min(64) as usize;
            let mut buf = vec![0u8; read_len];
            f.read_exact(&mut buf).ok()?;
            if read_len >= 12 {
                byte_rate = u32::from_le_bytes(buf[8..12].try_into().ok()?);
            }
            if size as usize > read_len {
                f.seek(SeekFrom::Current((size as usize - read_len) as i64))
                    .ok()?;
            }
        } else if &chunk_hdr[0..4] == b"data" {
            data_bytes = size;
            break;
        } else {
            let skip = (size as i64) + (size % 2) as i64;
            f.seek(SeekFrom::Current(skip)).ok()?;
        }
    }

    if byte_rate == 0 || data_bytes == 0 {
        return None;
    }
    Some(((data_bytes as u64 * 1000) / byte_rate as u64) as u32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write_test_wav(path: &Path, sample_rate: u32, duration_ms: u32) {
        let channels: u16 = 2;
        let bits_per_sample: u16 = 16;
        let num_frames = (sample_rate as u64 * duration_ms as u64) / 1000;
        let data_bytes = (num_frames * channels as u64 * (bits_per_sample as u64 / 8)) as u32;
        let byte_rate = sample_rate * channels as u32 * (bits_per_sample as u32 / 8);
        let block_align = channels * (bits_per_sample / 8);

        let mut buf = Vec::with_capacity(44 + data_bytes as usize);
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&(36 + data_bytes).to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes());
        buf.extend_from_slice(&1u16.to_le_bytes());
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_bytes.to_le_bytes());
        buf.resize(44 + data_bytes as usize, 0);
        std::fs::write(path, buf).unwrap();
    }

    #[tokio::test]
    async fn scans_folders_extracts_metadata_covers_likes_and_playlists() {
        let root =
            std::env::temp_dir().join(format!("canto_local_provider_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        let album_dir = root.join("music").join("Frank Ocean").join("Blonde");
        std::fs::create_dir_all(&album_dir).unwrap();

        write_test_wav(&album_dir.join("01 Nikes.wav"), 44_100, 1200);
        write_test_wav(&album_dir.join("02 Ivy.wav"), 44_100, 800);
        std::fs::write(&album_dir.join("cover.png"), b"\x89PNG\r\n\x1a\n").unwrap();

        let provider = LocalProvider::new_with_paths(
            vec![root.join("music")],
            root.join("state.json"),
            root.join("covers"),
        );
        assert_eq!(provider.kind(), ProviderKind::Local);
        assert!(!provider.capabilities().supports_connect);

        let albums = provider.get_saved_albums(0, 20).await.unwrap();
        assert_eq!(albums.items.len(), 1);
        assert_eq!(albums.items[0].title, "Blonde");
        assert_eq!(albums.items[0].artists[0].name, "Frank Ocean");
        assert!(albums.items[0]
            .art
            .largest()
            .is_some_and(|u| u.starts_with("file://") && u.ends_with("cover.png")));

        let album_tracks = provider
            .get_album_tracks(&albums.items[0].rri.id, 0, 20)
            .await
            .unwrap();
        assert_eq!(album_tracks.items.len(), 2);
        let nikes = album_tracks
            .items
            .iter()
            .find(|t| t.title.contains("Nikes"))
            .unwrap();
        assert_eq!(nikes.duration_ms, 1200);
        assert!(provider
            .stream_url(&nikes.rri.id)
            .is_some_and(|p| p.ends_with("01 Nikes.wav")));

        let found = provider.search("nikes", 0, 10).await.unwrap();
        assert_eq!(found.tracks.items.len(), 1);
        assert_eq!(found.albums.items.len(), 0);

        provider
            .save_tracks(vec![nikes.rri.id.clone()])
            .await
            .unwrap();
        let saved = provider.get_saved_tracks(0, 10).await.unwrap();
        assert_eq!(saved.items.len(), 1);
        assert_eq!(saved.items[0].rri.id, nikes.rri.id);

        let pl = provider
            .create_playlist("local", "Late Night", None, None, Some("Local faves"))
            .await
            .unwrap();
        provider
            .add_to_playlist(&pl.rri.id, vec![nikes.rri.id.clone()], None)
            .await
            .unwrap();
        let pl_tracks = provider
            .get_playlist_tracks(&pl.rri.id, 0, 10)
            .await
            .unwrap();
        assert_eq!(pl_tracks.items.len(), 1);

        let _ = std::fs::remove_dir_all(&root);
    }
}
