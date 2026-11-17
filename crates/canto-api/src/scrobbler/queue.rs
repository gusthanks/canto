//! Bounded persistent retry queue for offline scrobble submissions.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::ScrobbleTrack;

pub const MAX_QUEUE_ITEMS: usize = 500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScrobbleTarget {
    LastFm,
    ListenBrainz,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingScrobble {
    pub target: ScrobbleTarget,
    pub track: ScrobbleTrack,
    pub attempts: u32,
}

#[derive(Clone, Debug)]
pub struct RetryQueue {
    path: PathBuf,
    items: VecDeque<PendingScrobble>,
}

impl RetryQueue {
    pub fn default_path() -> PathBuf {
        glib::user_data_dir()
            .join("canto")
            .join("scrobble_queue.json")
    }

    pub fn load_default() -> Self {
        Self::load_from(Self::default_path())
    }

    pub fn load_from(path: PathBuf) -> Self {
        let items = match std::fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<VecDeque<PendingScrobble>>(&bytes) {
                Ok(mut parsed) => {
                    while parsed.len() > MAX_QUEUE_ITEMS {
                        parsed.pop_front();
                    }
                    parsed
                }
                Err(e) => {
                    warn!("scrobbler: ignoring corrupt retry queue {}: {e}", path.display());
                    VecDeque::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => VecDeque::new(),
            Err(e) => {
                warn!("scrobbler: failed to read retry queue {}: {e}", path.display());
                VecDeque::new()
            }
        };
        Self { path, items }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    pub fn enqueue(&mut self, target: ScrobbleTarget, track: ScrobbleTrack) {
        while self.items.len() >= MAX_QUEUE_ITEMS {
            self.items.pop_front();
        }
        self.items.push_back(PendingScrobble {
            target,
            track,
            attempts: 0,
        });
        self.save();
    }

    pub fn pop_front(&mut self) -> Option<PendingScrobble> {
        let item = self.items.pop_front();
        if item.is_some() {
            self.save();
        }
        item
    }

    pub fn push_front_retry(&mut self, mut item: PendingScrobble) {
        item.attempts = item.attempts.saturating_add(1);
        if self.items.len() >= MAX_QUEUE_ITEMS {
            // Keep oldest bounded; drop tail only if already full.
            self.items.pop_back();
        }
        self.items.push_front(item);
        self.save();
    }

    pub fn take_all(&mut self) -> Vec<PendingScrobble> {
        let drained: Vec<_> = self.items.drain(..).collect();
        self.save();
        drained
    }

    pub fn replace_all(&mut self, items: Vec<PendingScrobble>) {
        self.items.clear();
        let skip = items.len().saturating_sub(MAX_QUEUE_ITEMS);
        self.items.extend(items.into_iter().skip(skip));
        self.save();
    }

    /// Re-insert unflushed items ahead of any items concurrently enqueued during
    /// an async flush, trimming the oldest entries if the combined queue exceeds
    /// [`MAX_QUEUE_ITEMS`].
    pub fn requeue_remaining(&mut self, remaining: Vec<PendingScrobble>) {
        if remaining.is_empty() {
            return;
        }
        let concurrent_len = self.items.len();
        let mut combined = VecDeque::with_capacity(remaining.len() + concurrent_len);
        combined.extend(remaining);
        combined.append(&mut self.items);
        while combined.len() > MAX_QUEUE_ITEMS {
            combined.pop_front();
        }
        self.items = combined;
        self.save();
    }

    fn save(&self) {
        if self.items.is_empty() {
            if self.path.exists() {
                let _ = std::fs::remove_file(&self.path);
            }
            return;
        }
        let Some(parent) = self.path.parent() else {
            return;
        };
        if let Err(e) = std::fs::create_dir_all(parent) {
            warn!("scrobbler: failed to create {}: {e}", parent.display());
            return;
        }
        let Ok(bytes) = serde_json::to_vec(&self.items) else {
            return;
        };
        let tmp = self.path.with_extension(format!("tmp.{}", std::process::id()));
        if let Err(e) = std::fs::write(&tmp, &bytes) {
            warn!("scrobbler: failed to write {}: {e}", tmp.display());
            return;
        }
        if let Err(e) = std::fs::rename(&tmp, &self.path) {
            warn!("scrobbler: failed to commit {}: {e}", self.path.display());
            let _ = std::fs::remove_file(&tmp);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_track(n: usize) -> ScrobbleTrack {
        ScrobbleTrack {
            title: format!("Track {n}"),
            artist: "Artist".into(),
            album: None,
            album_artist: None,
            duration_secs: Some(180),
            started_at_unix: 1_700_000_000 + n as i64,
        }
    }

    #[test]
    fn enforces_max_500_items_and_persists_atomically() {
        let dir = std::env::temp_dir().join(format!(
            "canto_scrobble_queue_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("queue.json");
        let mut q = RetryQueue::load_from(path.clone());

        for i in 0..520 {
            q.enqueue(ScrobbleTarget::LastFm, sample_track(i));
        }

        assert_eq!(q.len(), MAX_QUEUE_ITEMS);
        let reloaded = RetryQueue::load_from(path.clone());
        assert_eq!(reloaded.len(), MAX_QUEUE_ITEMS);
        assert_eq!(
            reloaded.items.front().unwrap().track.title,
            "Track 20"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn requeue_remaining_preserves_concurrently_enqueued_items() {
        let dir = std::env::temp_dir().join(format!(
            "canto_scrobble_requeue_test_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let path = dir.join("queue.json");
        let mut q = RetryQueue::load_from(path.clone());

        q.enqueue(ScrobbleTarget::LastFm, sample_track(1));
        q.enqueue(ScrobbleTarget::ListenBrainz, sample_track(2));

        let taken = q.take_all();
        assert_eq!(taken.len(), 2);

        // Simulate a new offline scrobble arriving while async HTTP flush runs.
        q.enqueue(ScrobbleTarget::LastFm, sample_track(3));
        q.requeue_remaining(vec![taken[1].clone()]);

        let titles: Vec<String> = q.items.iter().map(|i| i.track.title.clone()).collect();
        assert_eq!(titles, vec!["Track 2", "Track 3"]);

        let reloaded = RetryQueue::load_from(path);
        let reloaded_titles: Vec<String> = reloaded
            .items
            .iter()
            .map(|i| i.track.title.clone())
            .collect();
        assert_eq!(reloaded_titles, vec!["Track 2", "Track 3"]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
