//! Navidrome / OpenSubsonic API provider.
//!
//! Authenticates with standard Subsonic salted MD5 tokens (`v=1.16.1`,
//! `c=canto`, `f=json`) and maps albums, artists, starred tracks, playlists,
//! search, similar tracks, cover art URLs, and stream URLs to Canto's
//! provider-neutral models.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use async_trait::async_trait;
use md5::{Digest, Md5};
use serde::Deserialize;
use url::Url;

use super::provider::{MusicProvider, ProviderCapabilities, ProviderKind};
use crate::defaults::{self, EntityKind};
use crate::error::DomainError;
use crate::http::build_shared_pool;
use crate::models::*;

const SUBSONIC_API_VERSION: &str = "1.16.1";
const SUBSONIC_CLIENT_NAME: &str = "canto";

static SALT_COUNTER: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct SubsonicConfig {
    pub server_url: String,
    pub username: String,
    pub password: String,
}

impl SubsonicConfig {
    pub fn is_valid(&self) -> bool {
        !self.server_url.trim().is_empty()
            && !self.username.trim().is_empty()
            && !self.password.is_empty()
    }
}

pub struct SubsonicProvider {
    config: SubsonicConfig,
    client: reqwest::Client,
}

pub fn subsonic_provider(config: SubsonicConfig) -> Arc<dyn MusicProvider> {
    Arc::new(SubsonicProvider::new(config))
}

pub async fn test_subsonic_connection(config: &SubsonicConfig) -> Result<String, DomainError> {
    SubsonicProvider::new(config.clone()).ping().await
}

/// Generate `(salt, token)` where `token = md5(password + salt)` per the
/// Subsonic REST API specification.
pub fn subsonic_auth_token(password: &str, salt: &str) -> String {
    let mut hasher = Md5::new();
    hasher.update(password.as_bytes());
    hasher.update(salt.as_bytes());
    hex::encode(hasher.finalize())
}

fn next_salt() -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = SALT_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{:016x}{:08x}", nanos as u64, seq as u32)
}

impl SubsonicProvider {
    pub fn new(mut config: SubsonicConfig) -> Self {
        config.server_url = config.server_url.trim().trim_end_matches('/').to_string();
        config.username = config.username.trim().to_string();
        Self {
            config,
            client: build_shared_pool(),
        }
    }

    fn endpoint_url(&self, method: &str, extra: &[(&str, &str)]) -> Result<String, DomainError> {
        let base = self.config.server_url.trim_end_matches('/');
        if base.is_empty() {
            return Err(DomainError::NoToken);
        }
        let mut url = Url::parse(&format!("{base}/rest/{method}.view"))
            .map_err(|e| DomainError::Parse(format!("invalid Subsonic URL: {e}")))?;
        let salt = next_salt();
        let token = subsonic_auth_token(&self.config.password, &salt);
        {
            let mut q = url.query_pairs_mut();
            q.append_pair("u", &self.config.username);
            q.append_pair("t", &token);
            q.append_pair("s", &salt);
            q.append_pair("v", SUBSONIC_API_VERSION);
            q.append_pair("c", SUBSONIC_CLIENT_NAME);
            q.append_pair("f", "json");
            for (k, v) in extra {
                q.append_pair(k, v);
            }
        }
        Ok(url.into())
    }

    async fn call<T: serde::de::DeserializeOwned>(
        &self,
        method: &str,
        extra: &[(&str, &str)],
    ) -> Result<T, DomainError> {
        let url = self.endpoint_url(method, extra)?;
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| DomainError::Network(format!("subsonic {method}: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            return Err(DomainError::ServerError {
                status: status.as_u16(),
                message: format!("subsonic {method} failed"),
            });
        }
        let text = resp
            .text()
            .await
            .map_err(|e| DomainError::Network(format!("subsonic {method} body: {e}")))?;
        let wrapper: SubsonicEnvelope = serde_json::from_str(&text)
            .map_err(|e| DomainError::Parse(format!("subsonic {method} json: {e}")))?;
        let body = wrapper.subsonic_response;
        if body.status != "ok" {
            let msg = body
                .error
                .map(|e| format!("{} ({})", e.message, e.code))
                .unwrap_or_else(|| "unknown Subsonic error".to_string());
            return Err(DomainError::ClientError {
                status: 400,
                message: msg,
            });
        }
        serde_json::from_value(serde_json::Value::Object(body.rest))
            .map_err(|e| DomainError::Parse(format!("subsonic {method} payload: {e}")))
    }

    pub async fn ping(&self) -> Result<String, DomainError> {
        let url = self.endpoint_url("ping", &[])?;
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| DomainError::Network(format!("subsonic ping: {e}")))?;
        let text = resp
            .text()
            .await
            .map_err(|e| DomainError::Network(format!("subsonic ping body: {e}")))?;
        let wrapper: SubsonicEnvelope = serde_json::from_str(&text)
            .map_err(|e| DomainError::Parse(format!("subsonic ping json: {e}")))?;
        let body = wrapper.subsonic_response;
        if body.status != "ok" {
            let msg = body
                .error
                .map(|e| e.message)
                .unwrap_or_else(|| "authentication failed".to_string());
            warn!("subsonic ping failed: {msg}");
            return Err(DomainError::AuthExpired);
        }
        let ver = body.server_version.unwrap_or(body.version);
        Ok(if ver.is_empty() {
            "ok".to_string()
        } else {
            ver
        })
    }

    fn cover_art_url(&self, cover_id: Option<&str>) -> ImageSet {
        let Some(cid) = cover_id.filter(|s| !s.is_empty()) else {
            return defaults::image_set_for(EntityKind::Album);
        };
        let Ok(url) = self.endpoint_url("getCoverArt", &[("id", cid), ("size", "600")]) else {
            return defaults::image_set_for(EntityKind::Album);
        };
        ImageSet {
            images: vec![Image {
                url,
                width: Some(600),
                height: Some(600),
            }],
            template: None,
        }
    }

    fn map_song(&self, s: SubsonicSong) -> Track {
        let artist_name = s.artist.clone().unwrap_or_else(defaults::artist_name);
        let artist_id = s
            .artist_id
            .clone()
            .unwrap_or_else(|| artist_name.to_lowercase());
        let album_ref = s.album.clone().map(|name| AlbumRef {
            rri: ResourceId::new(
                Provider::Subsonic,
                s.album_id.clone().unwrap_or_else(|| name.clone()),
            ),
            name,
        });
        let art = self.cover_art_url(s.cover_art.as_deref());
        Track {
            rri: ResourceId::new(Provider::Subsonic, s.id),
            title: s.title,
            artists: vec![ArtistRef {
                rri: ResourceId::new(Provider::Subsonic, artist_id),
                name: artist_name,
            }],
            album: album_ref,
            duration_ms: s.duration.unwrap_or(0).saturating_mul(1000),
            track_number: s.track,
            disc_number: s.disc_number,
            content_rating: ContentRating::None,
            isrc: None,
            art,
            playable: true,
            popularity: None,
            saved: Some(s.starred.is_some()),
            preview_url: None,
            url: None,
        }
    }

    fn map_album(&self, a: SubsonicAlbum) -> Album {
        let artist_name = a.artist.clone().unwrap_or_else(defaults::artist_name);
        let artist_id = a
            .artist_id
            .clone()
            .unwrap_or_else(|| artist_name.to_lowercase());
        let art = self.cover_art_url(a.cover_art.as_deref());
        let tracks = a.song.map(|songs| {
            let mapped: Vec<Track> = songs.into_iter().map(|s| self.map_song(s)).collect();
            let total = mapped.len();
            Page::offset_paged(mapped, 0, total)
        });
        Album {
            rri: ResourceId::new(Provider::Subsonic, a.id),
            title: a.name.or(a.title).unwrap_or_else(defaults::album_title),
            artists: vec![ArtistRef {
                rri: ResourceId::new(Provider::Subsonic, artist_id),
                name: artist_name,
            }],
            art,
            album_type: AlbumType::Album,
            release_date: a.year.map(|year| ReleaseDate {
                year,
                month: None,
                day: None,
            }),
            total_tracks: a.song_count,
            label: None,
            copyright: None,
            upc: None,
            genres: a.genre.into_iter().collect(),
            popularity: None,
            content_rating: ContentRating::None,
            saved: Some(a.starred.is_some()),
            tracks,
            url: None,
        }
    }

    fn map_artist(&self, a: SubsonicArtist) -> Artist {
        let art = a
            .cover_art
            .as_deref()
            .map(|c| self.cover_art_url(Some(c)))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Artist));
        Artist {
            rri: ResourceId::new(Provider::Subsonic, a.id),
            name: a.name,
            art,
            genres: Vec::new(),
            popularity: None,
            follower_count: None,
            bio: None,
            following: Some(true),
            url: None,
        }
    }

    fn map_playlist(&self, p: SubsonicPlaylist) -> Playlist {
        let art = p
            .cover_art
            .as_deref()
            .map(|c| self.cover_art_url(Some(c)))
            .unwrap_or_else(|| defaults::image_set_for(EntityKind::Playlist));
        let mapped_items = p.entry.map(|entries| {
            let items: Vec<PlaylistItem> = entries
                .into_iter()
                .enumerate()
                .map(|(pos, s)| PlaylistItem {
                    track: self.map_song(s),
                    added_at: None,
                    added_by: None,
                    position: Some(pos),
                })
                .collect();
            let total = items.len();
            Page::offset_paged(items, 0, total)
        });
        Playlist {
            rri: ResourceId::new(Provider::Subsonic, p.id),
            title: p.name,
            description: p.comment,
            owner: p.owner.map(|name| UserRef {
                rri: ResourceId::new(Provider::Subsonic, &name),
                display_name: name,
            }),
            art,
            total_tracks: p.song_count,
            collaborative: Some(false),
            public: p.public,
            version: None,
            tracks: mapped_items,
            following: Some(true),
            url: None,
        }
    }
}

#[async_trait]
impl MusicProvider for SubsonicProvider {
    fn kind(&self) -> ProviderKind {
        ProviderKind::Subsonic
    }

    fn capabilities(&self) -> ProviderCapabilities {
        ProviderCapabilities::subsonic()
    }

    fn stream_url(&self, track_id: &str) -> Option<String> {
        self.endpoint_url("stream", &[("id", track_id)]).ok()
    }

    async fn get_recommendations_for_track(
        &self,
        track_id: &str,
        limit: usize,
    ) -> Result<Vec<Track>, DomainError> {
        let count_str = limit.to_string();
        let resp: SimilarSongsResp = self
            .call(
                "getSimilarSongs2",
                &[("id", track_id), ("count", &count_str)],
            )
            .await?;
        let songs = resp.similar_songs2.map(|s| s.song).unwrap_or_default();
        Ok(songs.into_iter().map(|s| self.map_song(s)).collect())
    }

    async fn get_album(&self, id: &str) -> Result<Album, DomainError> {
        let resp: AlbumResp = self.call("getAlbum", &[("id", id)]).await?;
        Ok(self.map_album(resp.album))
    }

    async fn get_album_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let album = self.get_album(id).await?;
        let all = album.tracks.map(|p| p.items).unwrap_or_default();
        Ok(slice_page(&all, offset, limit))
    }

    async fn get_saved_albums(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let size_s = limit.to_string();
        let off_s = offset.to_string();
        let resp: AlbumListResp = self
            .call(
                "getAlbumList2",
                &[
                    ("type", "alphabeticalByName"),
                    ("size", &size_s),
                    ("offset", &off_s),
                ],
            )
            .await?;
        let items: Vec<Album> = resp
            .album_list2
            .map(|l| l.album)
            .unwrap_or_default()
            .into_iter()
            .map(|a| self.map_album(a))
            .collect();
        let total = if items.len() < limit {
            offset + items.len()
        } else {
            offset + items.len() + 1
        };
        Ok(Page::offset_paged(items, offset, total))
    }

    async fn check_saved_albums(&self, ids: &str) -> Result<Vec<bool>, DomainError> {
        let starred: StarredResp = self.call("getStarred2", &[]).await?;
        let starred_ids: std::collections::HashSet<String> = starred
            .starred2
            .map(|s| s.album)
            .unwrap_or_default()
            .into_iter()
            .map(|a| a.id)
            .collect();
        Ok(ids
            .split(',')
            .filter(|s| !s.is_empty())
            .map(|id| starred_ids.contains(id))
            .collect())
    }

    async fn save_albums(&self, ids: &str) -> Result<(), DomainError> {
        for id in ids.split(',').filter(|s| !s.is_empty()) {
            let _: serde_json::Value = self.call("star", &[("albumId", id)]).await?;
        }
        Ok(())
    }

    async fn remove_albums(&self, ids: &str) -> Result<(), DomainError> {
        for id in ids.split(',').filter(|s| !s.is_empty()) {
            let _: serde_json::Value = self.call("unstar", &[("albumId", id)]).await?;
        }
        Ok(())
    }

    async fn get_track(&self, id: &str) -> Result<Track, DomainError> {
        let resp: SongResp = self.call("getSong", &[("id", id)]).await?;
        Ok(self.map_song(resp.song))
    }

    async fn get_saved_tracks(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let resp: StarredResp = self.call("getStarred2", &[]).await?;
        let songs: Vec<Track> = resp
            .starred2
            .map(|s| s.song)
            .unwrap_or_default()
            .into_iter()
            .map(|s| self.map_song(s))
            .collect();
        Ok(slice_page(&songs, offset, limit))
    }

    async fn check_saved_tracks(&self, ids: Vec<String>) -> Result<Vec<bool>, DomainError> {
        let resp: StarredResp = self.call("getStarred2", &[]).await?;
        let set: std::collections::HashSet<String> = resp
            .starred2
            .map(|s| s.song)
            .unwrap_or_default()
            .into_iter()
            .map(|s| s.id)
            .collect();
        Ok(ids.into_iter().map(|id| set.contains(&id)).collect())
    }

    async fn save_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        for id in ids {
            let _: serde_json::Value = self.call("star", &[("id", &id)]).await?;
        }
        Ok(())
    }

    async fn remove_tracks(&self, ids: Vec<String>) -> Result<(), DomainError> {
        for id in ids {
            let _: serde_json::Value = self.call("unstar", &[("id", &id)]).await?;
        }
        Ok(())
    }

    async fn get_saved_playlists(
        &self,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Playlist>, DomainError> {
        let resp: PlaylistsResp = self.call("getPlaylists", &[]).await?;
        let items: Vec<Playlist> = resp
            .playlists
            .map(|p| p.playlist)
            .unwrap_or_default()
            .into_iter()
            .map(|p| self.map_playlist(p))
            .collect();
        Ok(slice_page(&items, offset, limit))
    }

    async fn get_playlist(&self, id: &str) -> Result<Playlist, DomainError> {
        let resp: PlaylistResp = self.call("getPlaylist", &[("id", id)]).await?;
        Ok(self.map_playlist(resp.playlist))
    }

    async fn get_playlist_item_uris(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<(Vec<Option<String>>, usize), DomainError> {
        let playlist = self.get_playlist(id).await?;
        let items = playlist.tracks.map(|p| p.items).unwrap_or_default();
        let total = items.len();
        let slice = if offset >= total {
            Vec::new()
        } else {
            items[offset..(offset + limit).min(total)]
                .iter()
                .map(|i| Some(i.track.rri.id.clone()))
                .collect()
        };
        Ok((slice, total))
    }

    async fn get_playlist_tracks(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Track>, DomainError> {
        let playlist = self.get_playlist(id).await?;
        let tracks: Vec<Track> = playlist
            .tracks
            .map(|p| p.items)
            .unwrap_or_default()
            .into_iter()
            .map(|item| item.track)
            .collect();
        Ok(slice_page(&tracks, offset, limit))
    }

    async fn add_to_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        _position: Option<i32>,
    ) -> Result<(), DomainError> {
        let mut params: Vec<(&str, String)> = vec![("playlistId", id.to_string())];
        for uri in uris {
            params.push(("songIdToAdd", uri));
        }
        let borrowed: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let _: serde_json::Value = self.call("updatePlaylist", &borrowed).await?;
        Ok(())
    }

    async fn remove_from_playlist(
        &self,
        id: &str,
        uris: Vec<String>,
        _snapshot_id: Option<&str>,
    ) -> Result<(), DomainError> {
        let playlist = self.get_playlist(id).await?;
        let drop_set: std::collections::HashSet<String> = uris.into_iter().collect();
        let items = playlist.tracks.map(|p| p.items).unwrap_or_default();
        let mut params: Vec<(&str, String)> = vec![("playlistId", id.to_string())];
        for (idx, item) in items.iter().enumerate() {
            if drop_set.contains(&item.track.rri.id) {
                params.push(("songIndexToRemove", idx.to_string()));
            }
        }
        if params.len() > 1 {
            let borrowed: Vec<(&str, &str)> =
                params.iter().map(|(k, v)| (*k, v.as_str())).collect();
            let _: serde_json::Value = self.call("updatePlaylist", &borrowed).await?;
        }
        Ok(())
    }

    async fn create_playlist(
        &self,
        _user_id: &str,
        name: &str,
        _public: Option<bool>,
        _collaborative: Option<bool>,
        _description: Option<&str>,
    ) -> Result<Playlist, DomainError> {
        let resp: PlaylistResp = self.call("createPlaylist", &[("name", name)]).await?;
        Ok(self.map_playlist(resp.playlist))
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
        let _: serde_json::Value = self.call("deletePlaylist", &[("id", id)]).await?;
        Ok(())
    }

    async fn update_playlist_details(
        &self,
        id: &str,
        name: Option<&str>,
        public: Option<bool>,
        _collaborative: Option<bool>,
        description: Option<&str>,
    ) -> Result<(), DomainError> {
        let mut params: Vec<(&str, String)> = vec![("playlistId", id.to_string())];
        if let Some(n) = name {
            params.push(("name", n.to_string()));
        }
        if let Some(d) = description {
            params.push(("comment", d.to_string()));
        }
        if let Some(p) = public {
            params.push(("public", p.to_string()));
        }
        let borrowed: Vec<(&str, &str)> = params.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let _: serde_json::Value = self.call("updatePlaylist", &borrowed).await?;
        Ok(())
    }

    async fn set_playlist_public(&self, id: &str, public: bool) -> Result<(), DomainError> {
        self.update_playlist_details(id, None, Some(public), None, None)
            .await
    }

    async fn get_artist(&self, id: &str) -> Result<Artist, DomainError> {
        let resp: ArtistResp = self.call("getArtist", &[("id", id)]).await?;
        Ok(self.map_artist(resp.artist))
    }

    async fn get_artist_albums(
        &self,
        id: &str,
        offset: usize,
        limit: usize,
    ) -> Result<Page<Album>, DomainError> {
        let resp: ArtistResp = self.call("getArtist", &[("id", id)]).await?;
        let albums: Vec<Album> = resp
            .artist
            .album
            .into_iter()
            .map(|a| self.map_album(a))
            .collect();
        Ok(slice_page(&albums, offset, limit))
    }

    async fn get_artist_top_tracks(&self, id: &str) -> Result<Vec<Track>, DomainError> {
        let artist = self.get_artist(id).await?;
        if let Ok(resp) = self
            .call::<TopSongsResp>("getTopSongs", &[("artist", &artist.name), ("count", "20")])
            .await
        {
            if let Some(songs) = resp.top_songs.map(|t| t.song) {
                if !songs.is_empty() {
                    return Ok(songs.into_iter().map(|s| self.map_song(s)).collect());
                }
            }
        }
        // Fallback: grab tracks from artist's albums.
        let albums = self.get_artist_albums(id, 0, 5).await?;
        let mut tracks = Vec::new();
        for alb in albums.items {
            if let Ok(full) = self.get_album(&alb.rri.id).await {
                if let Some(page) = full.tracks {
                    tracks.extend(page.items);
                }
            }
        }
        Ok(tracks)
    }

    async fn get_followed_artists(
        &self,
        _after: Option<&str>,
        limit: usize,
    ) -> Result<(Vec<Artist>, Option<String>), DomainError> {
        let resp: ArtistsResp = self.call("getArtists", &[]).await?;
        let artists: Vec<Artist> = resp
            .artists
            .map(|a| a.index)
            .unwrap_or_default()
            .into_iter()
            .flat_map(|idx| idx.artist)
            .take(limit)
            .map(|a| self.map_artist(a))
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
        Ok(vec![true; ids.split(',').filter(|s| !s.is_empty()).count()])
    }

    async fn get_current_user(&self) -> Result<User, DomainError> {
        Ok(User {
            rri: ResourceId::new(Provider::Subsonic, &self.config.username),
            display_name: self.config.username.clone(),
            art: defaults::image_set_for(EntityKind::User),
            follower_count: None,
            product: Some("subsonic".to_string()),
            country: None,
            explicit_filter: None,
            url: Some(self.config.server_url.clone()),
        })
    }

    async fn get_user(&self, id: &str) -> Result<User, DomainError> {
        Ok(User {
            rri: ResourceId::new(Provider::Subsonic, id),
            display_name: id.to_string(),
            art: defaults::image_set_for(EntityKind::User),
            follower_count: None,
            product: Some("subsonic".to_string()),
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
        let count_s = limit.to_string();
        let off_s = offset.to_string();
        let resp: Search3Resp = self
            .call(
                "search3",
                &[
                    ("query", query),
                    ("songCount", &count_s),
                    ("songOffset", &off_s),
                    ("albumCount", &count_s),
                    ("albumOffset", &off_s),
                    ("artistCount", &count_s),
                    ("artistOffset", &off_s),
                ],
            )
            .await?;
        let r = resp.search_result3.unwrap_or_default();
        let tracks: Vec<Track> = r.song.into_iter().map(|s| self.map_song(s)).collect();
        let albums: Vec<Album> = r.album.into_iter().map(|a| self.map_album(a)).collect();
        let artists: Vec<Artist> = r.artist.into_iter().map(|a| self.map_artist(a)).collect();
        Ok(SearchResults {
            tracks: Page::offset_paged(tracks, offset, offset + limit),
            albums: Page::offset_paged(albums, offset, offset + limit),
            artists: Page::offset_paged(artists, offset, offset + limit),
            playlists: Page::default(),
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
            SearchType::Playlist => SearchResults::default(),
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

fn slice_page<T: Clone>(items: &[T], offset: usize, limit: usize) -> Page<T> {
    let total = items.len();
    let slice = if offset >= total {
        Vec::new()
    } else {
        items[offset..(offset + limit).min(total)].to_vec()
    };
    Page::offset_paged(slice, offset, total)
}

fn de_id<'de, D>(de: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = serde_json::Value::deserialize(de)?;
    match val {
        serde_json::Value::String(s) => Ok(s),
        serde_json::Value::Number(n) => Ok(n.to_string()),
        _ => Ok(String::new()),
    }
}

fn de_opt_id<'de, D>(de: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let val = Option::<serde_json::Value>::deserialize(de)?;
    Ok(val.and_then(|v| match v {
        serde_json::Value::String(s) if !s.is_empty() => Some(s),
        serde_json::Value::Number(n) => Some(n.to_string()),
        _ => None,
    }))
}

#[derive(Deserialize)]
struct SubsonicEnvelope {
    #[serde(rename = "subsonic-response")]
    subsonic_response: SubsonicResponseBody,
}

#[derive(Deserialize)]
struct SubsonicResponseBody {
    status: String,
    #[serde(default)]
    version: String,
    #[serde(default, rename = "serverVersion")]
    server_version: Option<String>,
    #[serde(default)]
    error: Option<SubsonicError>,
    #[serde(flatten)]
    rest: serde_json::Map<String, serde_json::Value>,
}

#[derive(Deserialize)]
struct SubsonicError {
    code: i32,
    message: String,
}

#[derive(Deserialize)]
struct SubsonicSong {
    #[serde(deserialize_with = "de_id")]
    id: String,
    title: String,
    #[serde(default)]
    album: Option<String>,
    #[serde(default, rename = "albumId", deserialize_with = "de_opt_id")]
    album_id: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default, rename = "artistId", deserialize_with = "de_opt_id")]
    artist_id: Option<String>,
    #[serde(default)]
    track: Option<u32>,
    #[serde(default, rename = "discNumber")]
    disc_number: Option<u32>,
    #[serde(default)]
    duration: Option<u32>,
    #[serde(default, rename = "coverArt", deserialize_with = "de_opt_id")]
    cover_art: Option<String>,
    #[serde(default)]
    starred: Option<String>,
}

#[derive(Deserialize)]
struct SubsonicAlbum {
    #[serde(deserialize_with = "de_id")]
    id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    artist: Option<String>,
    #[serde(default, rename = "artistId", deserialize_with = "de_opt_id")]
    artist_id: Option<String>,
    #[serde(default, rename = "coverArt", deserialize_with = "de_opt_id")]
    cover_art: Option<String>,
    #[serde(default, rename = "songCount")]
    song_count: Option<u32>,
    #[serde(default)]
    year: Option<i32>,
    #[serde(default)]
    genre: Option<String>,
    #[serde(default)]
    starred: Option<String>,
    #[serde(default)]
    song: Option<Vec<SubsonicSong>>,
}

#[derive(Deserialize)]
struct SubsonicArtist {
    #[serde(deserialize_with = "de_id")]
    id: String,
    name: String,
    #[serde(default, rename = "coverArt", deserialize_with = "de_opt_id")]
    cover_art: Option<String>,
    #[serde(default)]
    album: Vec<SubsonicAlbum>,
}

#[derive(Deserialize)]
struct SubsonicPlaylist {
    #[serde(deserialize_with = "de_id")]
    id: String,
    name: String,
    #[serde(default)]
    comment: Option<String>,
    #[serde(default)]
    owner: Option<String>,
    #[serde(default)]
    public: Option<bool>,
    #[serde(default, rename = "songCount")]
    song_count: Option<u32>,
    #[serde(default, rename = "coverArt", deserialize_with = "de_opt_id")]
    cover_art: Option<String>,
    #[serde(default)]
    entry: Option<Vec<SubsonicSong>>,
}

#[derive(Deserialize)]
struct AlbumResp {
    album: SubsonicAlbum,
}

#[derive(Deserialize)]
struct SongResp {
    song: SubsonicSong,
}

#[derive(Deserialize)]
struct AlbumListResp {
    #[serde(default, rename = "albumList2")]
    album_list2: Option<AlbumListInner>,
}

#[derive(Deserialize)]
struct AlbumListInner {
    #[serde(default)]
    album: Vec<SubsonicAlbum>,
}

#[derive(Deserialize)]
struct StarredResp {
    #[serde(default)]
    starred2: Option<StarredInner>,
}

#[derive(Deserialize)]
struct StarredInner {
    #[serde(default)]
    album: Vec<SubsonicAlbum>,
    #[serde(default)]
    song: Vec<SubsonicSong>,
}

#[derive(Deserialize)]
struct PlaylistsResp {
    #[serde(default)]
    playlists: Option<PlaylistsInner>,
}

#[derive(Deserialize)]
struct PlaylistsInner {
    #[serde(default)]
    playlist: Vec<SubsonicPlaylist>,
}

#[derive(Deserialize)]
struct PlaylistResp {
    playlist: SubsonicPlaylist,
}

#[derive(Deserialize)]
struct ArtistResp {
    artist: SubsonicArtist,
}

#[derive(Deserialize)]
struct ArtistsResp {
    #[serde(default)]
    artists: Option<ArtistsInner>,
}

#[derive(Deserialize)]
struct ArtistsInner {
    #[serde(default)]
    index: Vec<ArtistsIndex>,
}

#[derive(Deserialize)]
struct ArtistsIndex {
    #[serde(default)]
    artist: Vec<SubsonicArtist>,
}

#[derive(Deserialize)]
struct TopSongsResp {
    #[serde(default, rename = "topSongs")]
    top_songs: Option<SongsInner>,
}

#[derive(Deserialize)]
struct SimilarSongsResp {
    #[serde(default, rename = "similarSongs2")]
    similar_songs2: Option<SongsInner>,
}

#[derive(Deserialize)]
struct SongsInner {
    #[serde(default)]
    song: Vec<SubsonicSong>,
}

#[derive(Deserialize)]
struct Search3Resp {
    #[serde(default, rename = "searchResult3")]
    search_result3: Option<Search3Inner>,
}

#[derive(Deserialize, Default)]
struct Search3Inner {
    #[serde(default)]
    artist: Vec<SubsonicArtist>,
    #[serde(default)]
    album: Vec<SubsonicAlbum>,
    #[serde(default)]
    song: Vec<SubsonicSong>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn subsonic_salted_md5_matches_spec() {
        // Subsonic spec example: password="sesame", salt="c19b2d" => md5("sesamec19b2d")
        assert_eq!(
            subsonic_auth_token("sesame", "c19b2d"),
            "26719a1196d2a940705a59634eb18eab"
        );
    }

    async fn spawn_mock_subsonic() -> String {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut stream, _)) = listener.accept().await {
                tokio::spawn(async move {
                    let mut buf = [0u8; 4096];
                    let n = stream.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let path = req
                        .lines()
                        .next()
                        .and_then(|l| l.split_whitespace().nth(1))
                        .unwrap_or("/");

                    let body = if path.starts_with("/rest/ping.view") {
                        r#"{"subsonic-response":{"status":"ok","version":"1.16.1","serverVersion":"Navidrome 0.53"}}"#
                    } else if path.starts_with("/rest/getAlbumList2.view") {
                        r#"{"subsonic-response":{"status":"ok","albumList2":{"album":[{"id":"alb-1","name":"Channel ORANGE","artist":"Frank Ocean","artistId":"art-1","coverArt":"cov-1","songCount":1,"year":2012}]}}}"#
                    } else if path.starts_with("/rest/getAlbum.view") {
                        r#"{"subsonic-response":{"status":"ok","album":{"id":"alb-1","name":"Channel ORANGE","artist":"Frank Ocean","artistId":"art-1","coverArt":"cov-1","song":[{"id":"trk-1","title":"Thinkin Bout You","album":"Channel ORANGE","albumId":"alb-1","artist":"Frank Ocean","artistId":"art-1","duration":200,"track":1,"coverArt":"cov-1"}]}}}"#
                    } else if path.starts_with("/rest/getStarred2.view") {
                        r#"{"subsonic-response":{"status":"ok","starred2":{"song":[{"id":"trk-1","title":"Thinkin Bout You","artist":"Frank Ocean","duration":200,"starred":"2026-01-01"}]}}}"#
                    } else if path.starts_with("/rest/getPlaylists.view") {
                        r#"{"subsonic-response":{"status":"ok","playlists":{"playlist":[{"id":"pl-1","name":"Drive","songCount":1,"owner":"gus"}]}}}"#
                    } else if path.starts_with("/rest/getPlaylist.view") {
                        r#"{"subsonic-response":{"status":"ok","playlist":{"id":"pl-1","name":"Drive","entry":[{"id":"trk-1","title":"Thinkin Bout You","artist":"Frank Ocean","duration":200}]}}}"#
                    } else if path.starts_with("/rest/search3.view") {
                        r#"{"subsonic-response":{"status":"ok","searchResult3":{"song":[{"id":"trk-1","title":"Thinkin Bout You","artist":"Frank Ocean","duration":200}],"album":[{"id":"alb-1","name":"Channel ORANGE","artist":"Frank Ocean"}],"artist":[{"id":"art-1","name":"Frank Ocean"}]}}}"#
                    } else {
                        r#"{"subsonic-response":{"status":"ok"}}"#
                    };

                    let response = format!(
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                        body.len(),
                        body
                    );
                    let _ = stream.write_all(response.as_bytes()).await;
                });
            }
        });
        format!("http://{addr}")
    }

    #[tokio::test]
    async fn subsonic_provider_queries_mock_server_and_builds_stream_url() {
        let base_url = spawn_mock_subsonic().await;
        let cfg = SubsonicConfig {
            server_url: base_url.clone(),
            username: "gus".into(),
            password: "secret".into(),
        };
        assert_eq!(
            test_subsonic_connection(&cfg).await.unwrap(),
            "Navidrome 0.53"
        );

        let provider = SubsonicProvider::new(cfg);
        assert_eq!(provider.kind(), ProviderKind::Subsonic);

        let albums = provider.get_saved_albums(0, 10).await.unwrap();
        assert_eq!(albums.items.len(), 1);
        assert_eq!(albums.items[0].title, "Channel ORANGE");
        assert!(albums.items[0]
            .art
            .largest()
            .is_some_and(|u| u.contains("getCoverArt.view") && u.contains("id=cov-1")));

        let album = provider.get_album("alb-1").await.unwrap();
        assert_eq!(album.tracks.as_ref().unwrap().items.len(), 1);

        let starred = provider.get_saved_tracks(0, 10).await.unwrap();
        assert_eq!(starred.items[0].title, "Thinkin Bout You");

        let playlists = provider.get_saved_playlists(0, 10).await.unwrap();
        assert_eq!(playlists.items[0].title, "Drive");

        let search = provider.search("Thinkin", 0, 10).await.unwrap();
        assert_eq!(search.tracks.items.len(), 1);

        let stream = provider.stream_url("trk-1").unwrap();
        assert!(stream.starts_with(&format!("{base_url}/rest/stream.view")));
        assert!(stream.contains("id=trk-1"));
        assert!(stream.contains("v=1.16.1"));
    }
}
