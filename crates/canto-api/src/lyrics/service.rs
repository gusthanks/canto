use log::debug;
use serde::Deserialize;

use super::cache::LyricsCache;
use super::lyricsfile;
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
    /// Whether `lyricsfile` has the time of each word.
    pub has_word_sync: Option<bool>,
    /// The lyrics as a Lyricsfile (YAML), with word timings when synced.
    pub lyricsfile: Option<String>,
}

impl LrclibItem {
    /// The item's lyrics, with real word timings when it has them.
    fn lyrics(&self) -> Option<Lyrics> {
        let parsed = parse_lyrics(self.synced_lyrics.as_deref(), self.plain_lyrics.as_deref())?;
        if self.has_word_sync != Some(true) {
            return Some(parsed);
        }
        let Lyrics::Synced(lrc) = &parsed else {
            return Some(parsed);
        };
        let words = self
            .lyricsfile
            .as_deref()
            .and_then(lyricsfile::parse)
            .and_then(|file| lyricsfile::to_lines(&file, lrc));
        match words {
            Some(lines) => Some(Lyrics::Synced(lines)),
            None => {
                debug!("this Lyricsfile could not be read; using its LRC lines");
                Some(parsed)
            }
        }
    }

    fn has_words(&self) -> bool {
        self.has_word_sync == Some(true) && self.synced_lyrics.is_some()
    }
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

        // Nothing online: lyrics saved before word timings are still lyrics
        self.cache.get_previous(key)
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
        item.lyrics()
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

        // Prefer word timings for the right length, then any synced lyrics
        let fits = |item: &LrclibItem| match (duration_secs, item.duration) {
            (Some(target), Some(d)) => (d - f64::from(target)).abs() <= 2.0,
            _ => true,
        };
        if let Some(lyrics) = items
            .iter()
            .filter(|item| item.has_words() && fits(item))
            .find_map(LrclibItem::lyrics)
        {
            return Some(lyrics);
        }
        for item in &items {
            if item.synced_lyrics.is_some() {
                if let Some(lyrics @ Lyrics::Synced(_)) = item.lyrics() {
                    return Some(lyrics);
                }
            }
        }
        for item in &items {
            if let Some(lyrics) = item.lyrics() {
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
    fn an_item_with_word_timings_lights_real_words() {
        let json = r#"{"trackName":"Blinding Lights","duration":200.0,"instrumental":false,
            "plainLyrics":"Yeah","syncedLyrics":"[00:13.12] Yeah\n[00:20.00] ♪\n",
            "hasWordSync":true,
            "lyricsfile":"lines:\n- text: Yeah\n  start_ms: 13120\n  end_ms: 14551\n  words:\n  - text: Yeah\n    start_ms: 13159\n"}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        let Some(Lyrics::Synced(lines)) = item.lyrics() else {
            panic!("synced lyrics expected");
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].words.len(), 1);
        assert_eq!(lines[0].end_time_ms, Some(14551));
        assert_eq!(lines[1].text, "♪");
    }

    #[test]
    fn an_old_style_item_still_reads() {
        let json = r#"{"plainLyrics":"a","syncedLyrics":"[00:01.00] a\n","duration":10}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        assert!(matches!(item.lyrics(), Some(Lyrics::Synced(l)) if l[0].words.is_empty()));
        // Word sync promised but the file is unreadable: the LRC lines stay
        let json = r#"{"syncedLyrics":"[00:01.00] a\n","hasWordSync":true,"lyricsfile":"lines:\n- text: |\n"}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        assert!(matches!(item.lyrics(), Some(Lyrics::Synced(l)) if l[0].text == "a"));
    }

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
