//! The provider contract.
//!
//! [`MusicProvider`] is a generic music vendor interface the
//! [`crate::service::ApiService`] depends on.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::error::DomainError;
use crate::models::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderKind {
    Spotify,
    YouTubeMusic,
    Local,
    Subsonic,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProviderCapabilities {
    pub supports_connect: bool,
    pub supports_recommendations: bool,
    pub supports_audio_engine_dsp: bool,
    pub supports_lyrics: bool,
    pub supports_search: bool,
    pub supports_library_sync: bool,
}

impl ProviderCapabilities {
    pub const fn spotify() -> Self {
        Self {
            supports_connect: true,
            supports_recommendations: true,
            supports_audio_engine_dsp: true,
            supports_lyrics: true,
            supports_search: true,
            supports_library_sync: true,
        }
    }

    pub const fn youtube_music() -> Self {
        Self {
            supports_connect: false,
            supports_recommendations: true,
            supports_audio_engine_dsp: true,
            supports_lyrics: true,
            supports_search: true,
            supports_library_sync: true,
        }
    }

    pub const fn local() -> Self {
        Self {
            supports_connect: false,
            supports_recommendations: true,
            supports_audio_engine_dsp: true,
            supports_lyrics: true,
            supports_search: true,
            supports_library_sync: false,
        }
    }

    pub const fn subsonic() -> Self {
        Self {
            supports_connect: false,
            supports_recommendations: true,
            supports_audio_engine_dsp: true,
            supports_lyrics: true,
            supports_search: true,
            supports_library_sync: true,
        }
    }

    pub const fn for_kind(kind: ProviderKind) -> Self {
        match kind {
            ProviderKind::Spotify => Self::spotify(),
            ProviderKind::YouTubeMusic => Self::youtube_music(),
            ProviderKind::Local => Self::local(),
            ProviderKind::Subsonic => Self::subsonic(),
        }
    }
}

impl Default for ProviderCapabilities {
    fn default() -> Self {
        Self::spotify()
    }
}

#[async_trait]
pub trait MusicProvider: Send + Sync + 'static {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Spotify
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::for_kind(self.kind())
    }

    async fn get_recommendations_for_track(
        &self,
        _track_id: &str,
        _limit: usize,
    ) -> Result<Vec<Track>, DomainError> {
        Ok(Vec::new())
    }

    async fn get_related_artists(&self, _artist_id: &str) -> Result<Vec<Artist>, DomainError> {
        Ok(Vec::new())
    }

    async fn get_album(&self, id: &str) -> Result<Album, DomainError>;
    async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError>;
    async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError>;
    async fn save_albums(&self, ids: &str) -> Result<(), DomainError>;
    async fn remove_albums(&self, ids: &str) -> Result<(), DomainError>;

    async fn get_track(&self, id: &str) -> Result<Track, DomainError>;
    async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn check_saved_tracks(&self, ids: Vec<String>) -> Result<Vec<bool>, DomainError>;
    async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError>;
    async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError>;

    async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError>;
    async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError>;

    async fn get_playlist_item_uris(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Option<String>>, usize), DomainError>;
    async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError>;
    async fn add_to_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        position: Option<i32>,
    ) -> Result<(), DomainError>;
    async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        snapshot_id: Option<&str>,
    ) -> Result<(), DomainError>;
    async fn create_playlist(
        &self,
        user_id: &str,
        name: &str,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<Playlist, DomainError>;
    async fn upload_playlist_cover(&self, id: &str, jpeg_base64: String)
        -> Result<(), DomainError>;
    async fn follow_playlist(&self, id: &str) -> Result<(), DomainError>;
    async fn unfollow_playlist(&self, id: &str) -> Result<(), DomainError>;
    async fn update_playlist_details(
        &self,
        id: &str,
        name: Option<&str>,
        public: Option<bool>,
        collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<(), DomainError>;

    async fn set_playlist_public(&self, id: &str, public: bool) -> Result<(), DomainError>;

    async fn get_artist(&self, id: &str) -> Result<Artist, DomainError>;
    async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError>;
    async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError>;
    async fn get_followed_artists(
        &self,
        after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError>;
    async fn follow_artists(&self, ids: &str) -> Result<(), DomainError>;
    async fn unfollow_artists(&self, ids: &str) -> Result<(), DomainError>;
    async fn check_following_artists(&self, ids: &str) -> Result<Vec<bool>, DomainError>;

    async fn get_current_user(&self) -> Result<User, DomainError>;
    async fn get_user(&self, id: &str) -> Result<User, DomainError>;
    async fn get_user_playlists(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError>;

    async fn search(
        &self,
        query: &str,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError>;
    async fn search_scoped(
        &self,
        query: &str,
        kind: SearchType,
        offset: usize,
        limit: usize,
    ) -> Result<SearchResults, DomainError>;

    async fn get_devices(&self) -> Result<Vec<Device>, DomainError>;
    async fn get_player_queue(&self) -> Result<Queue, DomainError>;
    async fn get_player_state(&self) -> Result<PlayerState, DomainError>;
    async fn player_resume(&self, device_id: &str) -> Result<(), DomainError>;
    /// What the user played lately, newest first, as library keys
    /// ("playlist:ID", "album:ID", "artist:ID", "liked").
    async fn get_recent_contexts(&self) -> Result<Vec<String>, DomainError> {
        Ok(vec![])
    }
    /// The track the account played last, while nothing plays now. `None`
    /// when something is playing somewhere (Canto follows that instead).
    async fn get_last_played(&self) -> Result<Option<LastPlayed>, DomainError> {
        Ok(None)
    }
    /// The songs the account played lately, newest first (at most `limit`).
    async fn get_recently_played(&self, limit: usize) -> Result<Vec<PlayedTrack>, DomainError> {
        let _ = limit;
        Ok(vec![])
    }

    /// Moves playback to `device_id`; `play` keeps it playing there.
    async fn player_transfer(&self, device_id: &str, play: bool) -> Result<(), DomainError> {
        let _ = (device_id, play);
        Ok(())
    }
    async fn player_play_in_context(
        &self,
        device_id: &str,
        context_uri: &str,
        offset: usize,
        position_ms: Option<u32>,
    ) -> Result<(), DomainError>;
    async fn player_play_uris(
        &self,
        device_id: &str,
        uris: Vec<String>,
        offset: usize,
        position_ms: Option<u32>,
    ) -> Result<(), DomainError>;
    async fn player_pause(&self, device_id: &str) -> Result<(), DomainError>;
    async fn player_seek(&self, device_id: &str, position_ms: u32) -> Result<(), DomainError>;
    async fn player_repeat(&self, device_id: &str, mode: RepeatMode) -> Result<(), DomainError>;
    async fn player_shuffle(&self, device_id: &str, state: bool) -> Result<(), DomainError>;
    async fn player_volume(&self, device_id: &str, volume_percent: u8) -> Result<(), DomainError>;
    async fn player_add_to_queue(&self, _device_id: &str, _uri: &str) -> Result<(), DomainError> {
        Ok(())
    }
    async fn player_next(&self, _device_id: &str) -> Result<(), DomainError> {
        Ok(())
    }
    async fn player_previous(&self, _device_id: &str) -> Result<(), DomainError> {
        Ok(())
    }

    /// Drop any cached auth configuration (e.g. after a 401).
    fn invalidate_config(&self);

    /// Resolve a playable local file path or HTTP stream URL for non-Spotify providers.
    fn stream_url(&self, _track_id: &str) -> Option<String> {
        None
    }

    /// Rescan library folders on local providers.
    async fn rescan(&self) -> Result<(), DomainError> {
        Ok(())
    }
}
