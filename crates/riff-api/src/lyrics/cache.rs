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

    pub fn cache_path(&self, key: &str) -> PathBuf {
        let safe_key: String = key
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
        let path = self.cache_path(key);
        let data = fs::read_to_string(&path).ok().or_else(|| {
            if let Some(stripped) = key.strip_prefix("spotify:track:") {
                fs::read_to_string(self.cache_path(stripped)).ok()
            } else {
                let prefixed = format!("spotify:track:{key}");
                fs::read_to_string(self.cache_path(&prefixed)).ok()
            }
        })?;
        serde_json::from_str(&data).ok()
    }

    pub fn store(&self, key: &str, lyrics: &Lyrics) -> io::Result<()> {
        let _ = fs::create_dir_all(&self.root);
        let data = serde_json::to_string(lyrics)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        fs::write(self.cache_path(key), &data)?;
        // If key starts with spotify:track:, save under bare id as well for instant dual-lookup
        if let Some(stripped) = key.strip_prefix("spotify:track:") {
            let _ = fs::write(self.cache_path(stripped), &data);
        }
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
}
