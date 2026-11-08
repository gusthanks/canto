use serde::Deserialize;

use super::cache::LyricsCache;
use super::model::Lyrics;
use super::parser::parse_lyrics;

const USER_AGENT: &str = "Canto/0.1 (https://github.com/gusthanks/canto)";
const LRCLIB_GET_URL: &str = "https://lrclib.net/api/get";
const LRCLIB_SEARCH_URL: &str = "https://lrclib.net/api/search";

#[derive(Clone, Debug)]
pub struct LyricsService {
    client: reqwest::Client,
    cache: LyricsCache,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LrclibItem {
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
    #[allow(dead_code)]
    pub instrumental: Option<bool>,
}

impl Default for LyricsService {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsService {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            client,
            cache: LyricsCache::new(),
        }
    }

    pub fn with_cache(cache: LyricsCache) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self { client, cache }
    }

    pub fn cache(&self) -> &LyricsCache {
        &self.cache
    }

    /// Fetch lyrics for a track, checking disk cache first (0ms), then LRCLIB API.
    pub async fn fetch_lyrics(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Option<Lyrics> {
        // 1. Check disk cache
        if let Some(cached) = self.cache.get(key) {
            return Some(cached);
        }

        // 2. Fetch from LRCLIB get endpoint with full parameters
        if let Some(lyrics) = self
            .query_get(track_name, artist_name, album_name, duration_secs)
            .await
        {
            let _ = self.cache.store(key, &lyrics);
            return Some(lyrics);
        }

        // 3. Fallback: try get endpoint with just track_name and artist_name
        if album_name.is_some() || duration_secs.is_some() {
            if let Some(lyrics) = self.query_get(track_name, artist_name, None, None).await {
                let _ = self.cache.store(key, &lyrics);
                return Some(lyrics);
            }
        }

        // 4. Fallback: search endpoint
        if let Some(lyrics) = self.query_search(track_name, artist_name).await {
            let _ = self.cache.store(key, &lyrics);
            return Some(lyrics);
        }

        None
    }

    async fn query_get(
        &self,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Option<Lyrics> {
        let mut query: Vec<(&str, String)> = vec![
            ("track_name", track_name.to_string()),
            ("artist_name", artist_name.to_string()),
        ];
        if let Some(album) = album_name {
            if !album.is_empty() {
                query.push(("album_name", album.to_string()));
            }
        }
        if let Some(duration) = duration_secs {
            if duration > 0 {
                query.push(("duration", duration.to_string()));
            }
        }

        let resp = self
            .client
            .get(LRCLIB_GET_URL)
            .query(&query)
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let item: LrclibItem = resp.json().await.ok()?;
        parse_lyrics(item.synced_lyrics.as_deref(), item.plain_lyrics.as_deref())
    }

    async fn query_search(&self, track_name: &str, artist_name: &str) -> Option<Lyrics> {
        let query = [
            ("track_name", track_name),
            ("artist_name", artist_name),
        ];

        let resp = self
            .client
            .get(LRCLIB_SEARCH_URL)
            .query(&query)
            .send()
            .await
            .ok()?;

        if !resp.status().is_success() {
            return None;
        }

        let items: Vec<LrclibItem> = resp.json().await.ok()?;
        // Prefer synced lyrics if available
        for item in &items {
            if let Some(synced) = &item.synced_lyrics {
                if let Some(lyrics) = parse_lyrics(Some(synced), None) {
                    return Some(lyrics);
                }
            }
        }
        for item in &items {
            if let Some(lyrics) = parse_lyrics(
                item.synced_lyrics.as_deref(),
                item.plain_lyrics.as_deref(),
            ) {
                return Some(lyrics);
            }
        }

        None
    }
}
