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
    translator: super::translate::LyricsTranslator,
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LrclibItem {
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
    pub instrumental: Option<bool>,
    pub duration: Option<f64>,
}

fn clean_track_title(title: &str) -> Option<String> {
    let mut current = title.trim().to_string();
    let lower_patterns = [
        " - remaster",
        " - live",
        " - ao vivo",
        " - radio edit",
        " - single version",
        " - album version",
        " - deluxe",
        " - acoustic",
        " - instrumental",
        " - demo",
        " - explicit",
        " - clean",
        " - mono",
        " - stereo",
        " (remaster",
        " (live",
        " (ao vivo",
        " (radio edit",
        " (single version",
        " (album version",
        " (deluxe",
        " (acoustic",
        " (instrumental",
        " (explicit",
        " (feat.",
        " (ft.",
        " (with ",
        " [remaster",
        " [live",
        " [ao vivo",
        " [deluxe",
        " [explicit",
        " [feat.",
        " [ft.",
        " [with ",
    ];

    loop {
        let lower = current.to_ascii_lowercase();
        let mut cut_at: Option<usize> = None;

        for pattern in lower_patterns {
            if let Some(idx) = lower.find(pattern) {
                cut_at = Some(cut_at.map_or(idx, |prev| prev.min(idx)));
            }
        }

        // Also match year-prefixed dash suffixes such as " - 2011 Remaster" or " - 2023 Mix".
        if let Some(year_idx) = find_year_edition_dash(&lower) {
            cut_at = Some(cut_at.map_or(year_idx, |prev| prev.min(year_idx)));
        }
        // Match year-prefixed parenthetical editions like " (2011 Remaster)" or " (2009 Mix)".
        if let Some(year_paren_idx) = find_year_edition_paren(&lower) {
            cut_at = Some(cut_at.map_or(year_paren_idx, |prev| prev.min(year_paren_idx)));
        }

        let Some(idx) = cut_at else {
            break;
        };
        let next = current[..idx].trim();
        if next.is_empty() || next.len() == current.len() {
            break;
        }
        current = next.to_string();
    }

    (current != title.trim() && !current.is_empty()).then_some(current)
}

fn find_year_edition_dash(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let needle = b" - ";
    let mut i = 0;
    while i + 8 <= bytes.len() {
        if &bytes[i..i + 3] == needle && bytes[i + 3..i + 7].iter().all(|b| b.is_ascii_digit()) {
            let tail = &lower[i + 7..];
            if tail.contains("remaster")
                || tail.contains("mix")
                || tail.contains("version")
                || tail.contains("edit")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn find_year_edition_paren(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let needle = b" (";
    let mut i = 0;
    while i + 7 <= bytes.len() {
        if &bytes[i..i + 2] == needle && bytes[i + 2..i + 6].iter().all(|b| b.is_ascii_digit()) {
            let tail = &lower[i + 6..];
            if tail.contains("remaster")
                || tail.contains("mix")
                || tail.contains("version")
                || tail.contains("edition")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
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
            translator: super::translate::LyricsTranslator::new(client.clone()),
            client,
            cache: LyricsCache::new(),
        }
    }

    pub fn with_cache(cache: LyricsCache) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            translator: super::translate::LyricsTranslator::new(client.clone()),
            client,
            cache,
        }
    }

    pub fn cache(&self) -> &LyricsCache {
        &self.cache
    }

    pub fn translator(&self) -> &super::translate::LyricsTranslator {
        &self.translator
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
        self.fetch_lyrics_inner(
            key,
            track_name,
            artist_name,
            album_name,
            duration_secs,
            false,
        )
        .await
    }

    /// Refetch lyrics from LRCLIB bypassing the cached disk copy.
    pub async fn refetch_lyrics(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Option<Lyrics> {
        self.fetch_lyrics_inner(
            key,
            track_name,
            artist_name,
            album_name,
            duration_secs,
            true,
        )
        .await
    }

    async fn fetch_lyrics_inner(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
        force_refresh: bool,
    ) -> Option<Lyrics> {
        let track_name = track_name.trim();
        let artist_name = artist_name.trim();
        if track_name.is_empty() || artist_name.is_empty() {
            return None;
        }

        // 1. Check disk cache unless explicitly refreshing
        if !force_refresh {
            if let Some(cached) = self.cache.get(key) {
                return Some(cached);
            }
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

        // 4. Fallback: search endpoint with duration preference
        if let Some(lyrics) = self
            .query_search(track_name, artist_name, duration_secs)
            .await
        {
            let _ = self.cache.store(key, &lyrics);
            return Some(lyrics);
        }

        // 5. Fallback: clean track title (strip " - Remastered", " (Live)", etc.)
        if let Some(clean_title) = clean_track_title(track_name) {
            if let Some(lyrics) = self
                .query_get(&clean_title, artist_name, None, duration_secs)
                .await
            {
                let _ = self.cache.store(key, &lyrics);
                return Some(lyrics);
            }
            if let Some(lyrics) = self
                .query_search(&clean_title, artist_name, duration_secs)
                .await
            {
                let _ = self.cache.store(key, &lyrics);
                return Some(lyrics);
            }
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

    async fn query_search(
        &self,
        track_name: &str,
        artist_name: &str,
        duration_secs: Option<u32>,
    ) -> Option<Lyrics> {
        let query = [("track_name", track_name), ("artist_name", artist_name)];

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

        let mut items: Vec<LrclibItem> = resp.json().await.ok()?;
        // Filter out non-lyric instrumental items
        items.retain(|item| item.instrumental != Some(true));

        // If target duration is known, sort items by proximity to target duration
        if let Some(target) = duration_secs {
            let target_f = target as f64;
            items.sort_by(|a, b| {
                let diff_a = a.duration.map_or(f64::MAX, |d| (d - target_f).abs());
                let diff_b = b.duration.map_or(f64::MAX, |d| (d - target_f).abs());
                diff_a
                    .partial_cmp(&diff_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Prefer synced lyrics if available
        for item in &items {
            if let Some(synced) = &item.synced_lyrics {
                if let Some(lyrics) = parse_lyrics(Some(synced), None) {
                    return Some(lyrics);
                }
            }
        }
        for item in &items {
            if let Some(lyrics) =
                parse_lyrics(item.synced_lyrics.as_deref(), item.plain_lyrics.as_deref())
            {
                return Some(lyrics);
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_track_title_handles_remasters_features_and_live_editions() {
        assert_eq!(
            clean_track_title("Bohemian Rhapsody - Remastered 2011").as_deref(),
            Some("Bohemian Rhapsody")
        );
        assert_eq!(
            clean_track_title("Yesterday - 2009 Remaster").as_deref(),
            Some("Yesterday")
        );
        assert_eq!(
            clean_track_title("Wish You Were Here (2011 Remaster)").as_deref(),
            Some("Wish You Were Here")
        );
        assert_eq!(
            clean_track_title("Evidências (Ao Vivo) [Deluxe]").as_deref(),
            Some("Evidências")
        );
        assert_eq!(
            clean_track_title("Stay (with Justin Bieber)").as_deref(),
            Some("Stay")
        );
        assert_eq!(
            clean_track_title("Track One - radio edit (feat. Guest)").as_deref(),
            Some("Track One")
        );
        assert_eq!(clean_track_title("Let It Be"), None);
    }
}
