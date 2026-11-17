use std::fs;
use std::io;
use std::path::PathBuf;

use super::model::Lyrics;

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
        key.trim().strip_prefix("spotify:track:").unwrap_or(key.trim())
    }

    pub fn cache_path(&self, key: &str) -> PathBuf {
        let canonical = Self::normalize_key(key);
        let safe_key: String = canonical
            .chars()
            .map(|c| match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => c,
                _ => '_',
            })
            .collect();
        let filename = if safe_key.is_empty() {
            "default.json".to_string()
        } else {
            format!("{safe_key}.json")
        };
        self.root.join(filename)
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
                fs::read_to_string(self.root.join(format!("{legacy_safe}.json"))).ok()
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
            },
            LyricLine {
                start_time_ms: 5000,
                text: "Second line".into(),
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
    fn test_lyrics_cache_uri_and_id_keying() {
        let temp_dir = unique_temp_dir("canto_test_cache_uri");
        let cache = LyricsCache::with_dir(temp_dir.clone());

        let lyrics = Lyrics::Plain(vec!["Just plain lyrics".into()]);

        // Store using full spotify URI
        cache.store("spotify:track:4cOdK2wGLETKBW3PvgPWqT", &lyrics).expect("store");

        // Lookup using bare ID should succeed
        let hit_by_id = cache.get("4cOdK2wGLETKBW3PvgPWqT").expect("hit by id");
        assert_eq!(hit_by_id, lyrics);

        // Lookup using URI should also succeed
        let hit_by_uri = cache.get("spotify:track:4cOdK2wGLETKBW3PvgPWqT").expect("hit by uri");
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
