use std::fs;
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use super::model::{FoundLyrics, Lyrics};

/// Bumped when saved lyrics lack something new (v2: word timings), so
/// they are fetched again.
const CACHE_VERSION: &str = "v2";
/// Lyrics from Genius are saved apart from LRCLIB's, so neither source is
/// ever taken for the other.
const GENIUS_CACHE_VERSION: &str = "genius.v1";
/// Genius lyrics are used this long before LRCLIB is asked again: a song
/// LRCLIB lacks would otherwise cost LRCLIB five requests at every play.
pub const GENIUS_FRESH_SECS: u64 = 7 * 24 * 60 * 60;

/// Lyrics Genius had for a track, as saved on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusLyricsEntry {
    /// Plain lines, `""` between stanzas
    pub lines: Vec<String>,
    /// The song's page on Genius
    pub url: String,
    /// When they were fetched, in Unix seconds
    pub fetched_at: u64,
}

impl GeniusLyricsEntry {
    pub fn is_fresh(&self, now: u64) -> bool {
        now.saturating_sub(self.fetched_at) < GENIUS_FRESH_SECS
    }

    pub fn into_found(self) -> FoundLyrics {
        FoundLyrics::genius(self.lines, self.url)
    }
}

#[derive(Clone, Debug)]
pub struct LyricsCache {
    root: PathBuf,
}

impl Default for LyricsCache {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsCache {
    pub fn default_dir() -> PathBuf {
        glib::user_cache_dir().join("canto").join("lyrics")
    }

    pub fn new() -> Self {
        Self {
            root: Self::default_dir(),
        }
    }

    pub fn with_dir(root: PathBuf) -> Self {
        Self { root }
    }

    pub fn root_dir(&self) -> &PathBuf {
        &self.root
    }

    /// Normalize Spotify track URIs or IDs to a canonical key.
    pub fn normalize_key(key: &str) -> &str {
        key.trim()
            .strip_prefix("spotify:track:")
            .unwrap_or(key.trim())
    }

    pub fn cache_path(&self, key: &str) -> PathBuf {
        self.root.join(Self::file_name(key, CACHE_VERSION))
    }

    /// Where Genius's lyrics for `key` are saved, next to LRCLIB's.
    pub fn genius_cache_path(&self, key: &str) -> PathBuf {
        self.root.join(Self::file_name(key, GENIUS_CACHE_VERSION))
    }

    fn file_name(key: &str, version: &str) -> String {
        let canonical = Self::normalize_key(key);
        let safe_key: String = canonical
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '_',
            })
            .collect();
        if safe_key.is_empty() {
            format!("default.{version}.json")
        } else {
            format!("{safe_key}.{version}.json")
        }
    }

    /// The lyrics Genius had for `key`, whatever their age.
    pub fn get_genius(&self, key: &str) -> Option<GeniusLyricsEntry> {
        if Self::normalize_key(key).is_empty() {
            return None;
        }
        let data = fs::read_to_string(self.genius_cache_path(key)).ok()?;
        let entry: GeniusLyricsEntry = serde_json::from_str(&data).ok()?;
        entry
            .lines
            .iter()
            .any(|line| !line.trim().is_empty())
            .then_some(entry)
    }

    pub fn store_genius(&self, key: &str, entry: &GeniusLyricsEntry) -> io::Result<()> {
        if Self::normalize_key(key).is_empty() {
            return Ok(());
        }
        let _ = fs::create_dir_all(&self.root);
        let data = serde_json::to_string(entry)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.genius_cache_path(key), data)
    }

    /// Lyrics saved by an older Canto (before word timings), for when
    /// nothing can be fetched.
    pub fn get_previous(&self, key: &str) -> Option<Lyrics> {
        let canonical = Self::normalize_key(key);
        if canonical.is_empty() {
            return None;
        }
        let old = self
            .cache_path(canonical)
            .with_extension("")
            .with_extension("json");
        serde_json::from_str(&fs::read_to_string(old).ok()?).ok()
    }

    pub fn get(&self, key: &str) -> Option<Lyrics> {
        let canonical = Self::normalize_key(key);
        if canonical.is_empty() {
            return None;
        }
        let path = self.cache_path(canonical);
        let data = fs::read_to_string(&path).ok().or_else(|| {
            // Backward-compatibility: check legacy raw key format (e.g. spotify_track_<id>.json)
            let legacy_safe: String = key
                .chars()
                .map(|c| match c {
                    'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                    _ => '_',
                })
                .collect();
            if !legacy_safe.is_empty() && legacy_safe != canonical {
                fs::read_to_string(
                    self.root
                        .join(format!("{legacy_safe}.{CACHE_VERSION}.json")),
                )
                .ok()
            } else {
                None
            }
        })?;
        serde_json::from_str(&data).ok()
    }

    pub fn store(&self, key: &str, lyrics: &Lyrics) -> io::Result<()> {
        let canonical = Self::normalize_key(key);
        if canonical.is_empty() {
            return Ok(());
        }
        let _ = fs::create_dir_all(&self.root);
        let data = serde_json::to_string(lyrics)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.cache_path(canonical), &data)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::model::LyricLine;

    fn unique_temp_dir(prefix: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("{prefix}_{nanos}"))
    }

    #[test]
    fn test_lyrics_cache_roundtrip() {
        let temp_dir = unique_temp_dir("canto_test_cache");
        let cache = LyricsCache::with_dir(temp_dir.clone());

        let lines = vec![
            LyricLine {
                start_time_ms: 1000,
                text: "Hello world".into(),
                ..Default::default()
            },
            LyricLine {
                start_time_ms: 5000,
                text: "Second line".into(),
                ..Default::default()
            },
        ];
        let lyrics = Lyrics::Synced(lines);

        // Miss before store
        assert_eq!(cache.get("track_123"), None);

        // Store and hit
        cache.store("track_123", &lyrics).expect("store");
        let cached = cache.get("track_123").expect("cached hit");
        assert_eq!(cached, lyrics);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn lyrics_saved_before_word_timings_are_fetched_again() {
        let temp_dir = unique_temp_dir("canto_test_cache_v1");
        let cache = LyricsCache::with_dir(temp_dir.clone());
        let _ = fs::create_dir_all(&temp_dir);
        let lyrics = Lyrics::Plain(vec!["old".into()]);
        fs::write(
            temp_dir.join("abc.json"),
            serde_json::to_string(&lyrics).unwrap(),
        )
        .unwrap();
        assert_eq!(cache.get("abc"), None);
        assert_eq!(cache.get_previous("abc"), Some(lyrics));
        assert!(cache.cache_path("abc").ends_with("abc.v2.json"));
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lyrics_cache_uri_and_id_keying() {
        let temp_dir = unique_temp_dir("canto_test_cache_uri");
        let cache = LyricsCache::with_dir(temp_dir.clone());

        let lyrics = Lyrics::Plain(vec!["Just plain lyrics".into()]);

        // Store using full spotify URI
        cache
            .store("spotify:track:4cOdK2wGLETKBW3PvgPWqT", &lyrics)
            .expect("store");

        // Lookup using bare ID should succeed
        let hit_by_id = cache.get("4cOdK2wGLETKBW3PvgPWqT").expect("hit by id");
        assert_eq!(hit_by_id, lyrics);

        // Lookup using URI should also succeed
        let hit_by_uri = cache
            .get("spotify:track:4cOdK2wGLETKBW3PvgPWqT")
            .expect("hit by uri");
        assert_eq!(hit_by_uri, lyrics);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lyrics_cache_corrupted_file() {
        let temp_dir = unique_temp_dir("canto_test_corrupt");
        let cache = LyricsCache::with_dir(temp_dir.clone());
        let _ = fs::create_dir_all(&temp_dir);

        // Write invalid JSON
        let path = cache.cache_path("corrupted_key");
        fs::write(path, "not json content").expect("write");

        assert_eq!(cache.get("corrupted_key"), None);

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn genius_lyrics_are_saved_apart_from_lrclib_lyrics() {
        let temp_dir = unique_temp_dir("canto_test_cache_genius");
        let cache = LyricsCache::with_dir(temp_dir.clone());
        let entry = GeniusLyricsEntry {
            lines: vec!["first".into(), String::new(), "second".into()],
            url: "https://genius.com/Nora-vale-lua-de-papel-lyrics".into(),
            fetched_at: 1_800_000_000,
        };

        assert_eq!(cache.get_genius("abc"), None);
        cache
            .store_genius("spotify:track:abc", &entry)
            .expect("store");
        assert!(temp_dir.join("abc.genius.v1.json").exists());
        // The LRCLIB file keeps its name and stays empty
        assert!(cache.cache_path("abc").ends_with("abc.v2.json"));
        assert_eq!(cache.get("abc"), None);
        assert_eq!(cache.get_previous("abc"), None);
        // Same file by URI or by bare id
        assert_eq!(cache.get_genius("abc"), Some(entry.clone()));
        assert_eq!(cache.get_genius("spotify:track:abc"), Some(entry.clone()));
        assert_eq!(
            entry.into_found(),
            FoundLyrics::genius(
                vec!["first".into(), String::new(), "second".into()],
                "https://genius.com/Nora-vale-lua-de-papel-lyrics".into()
            )
        );

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn genius_lyrics_stay_fresh_for_a_week() {
        let day = 24 * 60 * 60;
        let entry = |fetched_at| GeniusLyricsEntry {
            lines: vec!["x".into()],
            url: String::new(),
            fetched_at,
        };
        let now = 1_800_000_000;
        assert!(entry(now).is_fresh(now));
        assert!(entry(now - 6 * day).is_fresh(now));
        assert!(!entry(now - 7 * day).is_fresh(now));
        // A clock set back keeps them, rather than asking again at every play
        assert!(entry(now + day).is_fresh(now));
    }

    #[test]
    fn broken_or_empty_genius_files_are_ignored() {
        let temp_dir = unique_temp_dir("canto_test_cache_genius_bad");
        let cache = LyricsCache::with_dir(temp_dir.clone());
        let _ = fs::create_dir_all(&temp_dir);
        fs::write(cache.genius_cache_path("bad"), "not json").unwrap();
        assert_eq!(cache.get_genius("bad"), None);
        let blank = GeniusLyricsEntry {
            lines: vec![String::new(), " ".into()],
            url: "https://genius.com/x".into(),
            fetched_at: 1,
        };
        cache.store_genius("blank", &blank).unwrap();
        assert_eq!(cache.get_genius("blank"), None);
        cache.store_genius("", &blank).unwrap();
        assert_eq!(cache.get_genius(""), None);
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_lyrics_cache_empty_key() {
        let temp_dir = unique_temp_dir("canto_test_empty");
        let cache = LyricsCache::with_dir(temp_dir.clone());
        let lyrics = Lyrics::Plain(vec!["text".into()]);

        assert_eq!(cache.get(""), None);
        assert_eq!(cache.get("   "), None);
        cache.store("", &lyrics).expect("store empty key");
        assert_eq!(cache.get(""), None);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
