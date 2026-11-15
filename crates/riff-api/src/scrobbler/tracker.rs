//! Deterministic active play-time tracker for scrobbling.
//!
//! Rules:
//! - Active time accumulates ONLY from real elapsed wall-clock time while playing.
//! - Paused states stop accumulation immediately without resetting progress.
//! - Repeated Spotify Connect 3s poll ticks for the same playing track never double-count.
//! - Tracks >= 30s scrobble once per play when accumulated active play reaches
//!   `min(duration / 2, 240s)`.

use super::ScrobbleTrack;

const MIN_SCROBBLE_DURATION_MS: u64 = 30_000;
const MAX_SCROBBLE_THRESHOLD_MS: u64 = 240_000;
/// Cap single tick delta so system suspend does not count hours of sleep as listening.
const MAX_WALL_STEP_MS: u64 = 5_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScrobbleTrackMetadata {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub duration_ms: u32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ScrobbleAction {
    SendNowPlaying(ScrobbleTrack),
    SubmitScrobble(ScrobbleTrack),
}

#[derive(Debug, Default)]
pub struct ScrobbleTracker {
    active: Option<ActivePlay>,
}

#[derive(Debug)]
struct ActivePlay {
    track: ScrobbleTrackMetadata,
    started_at_unix: i64,
    accumulated_ms: u64,
    last_wall_ms: Option<u64>,
    last_position_ms: u32,
    now_playing_sent: bool,
    scrobbled: bool,
}

impl ScrobbleTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.active = None;
    }

    /// Update tracker with current player snapshot.
    ///
    /// Returns any scrobble actions triggered by this state transition or tick.
    pub fn update(
        &mut self,
        track: Option<&ScrobbleTrackMetadata>,
        is_playing: bool,
        position_ms: u32,
        is_remote: bool,
        respect_remote: bool,
        now_unix_ms: u64,
    ) -> Vec<ScrobbleAction> {
        if is_remote && !respect_remote {
            self.active = None;
            return Vec::new();
        }

        let Some(track) = track else {
            self.active = None;
            return Vec::new();
        };
        if track.id.trim().is_empty() || track.title.trim().is_empty() || track.artist.trim().is_empty() {
            self.active = None;
            return Vec::new();
        }

        let needs_new_session = match &self.active {
            None => true,
            Some(prev) => {
                prev.track.id != track.id
                    || is_track_restarted(prev, position_ms, track.duration_ms)
            }
        };

        if needs_new_session {
            self.active = Some(ActivePlay {
                track: track.clone(),
                started_at_unix: (now_unix_ms / 1000) as i64,
                accumulated_ms: 0,
                last_wall_ms: if is_playing { Some(now_unix_ms) } else { None },
                last_position_ms: position_ms,
                now_playing_sent: false,
                scrobbled: false,
            });
        }

        let Some(play) = self.active.as_mut() else {
            return Vec::new();
        };

        // Keep metadata fresh (e.g. if duration arrived late), without resetting play timers.
        play.track = track.clone();

        let mut actions = Vec::new();

        if !is_playing {
            play.last_wall_ms = None;
            play.last_position_ms = position_ms;
            return actions;
        }

        if let Some(prev_wall) = play.last_wall_ms {
            let delta = now_unix_ms.saturating_sub(prev_wall).min(MAX_WALL_STEP_MS);
            play.accumulated_ms = play.accumulated_ms.saturating_add(delta);
        }
        play.last_wall_ms = Some(now_unix_ms);
        play.last_position_ms = position_ms;

        if !play.now_playing_sent {
            play.now_playing_sent = true;
            actions.push(ScrobbleAction::SendNowPlaying(play.to_scrobble_track()));
        }

        let duration_ms = u64::from(play.track.duration_ms);
        if !play.scrobbled && duration_ms >= MIN_SCROBBLE_DURATION_MS {
            let threshold_ms = (duration_ms / 2).min(MAX_SCROBBLE_THRESHOLD_MS);
            if play.accumulated_ms >= threshold_ms {
                play.scrobbled = true;
                actions.push(ScrobbleAction::SubmitScrobble(play.to_scrobble_track()));
            }
        }

        actions
    }
}

impl ActivePlay {
    fn to_scrobble_track(&self) -> ScrobbleTrack {
        ScrobbleTrack {
            title: self.track.title.clone(),
            artist: self.track.artist.clone(),
            album: self.track.album.clone(),
            album_artist: self.track.album_artist.clone(),
            duration_secs: (self.track.duration_ms > 0).then_some(self.track.duration_ms / 1000),
            started_at_unix: self.started_at_unix,
        }
    }
}

fn is_track_restarted(prev: &ActivePlay, new_pos_ms: u32, duration_ms: u32) -> bool {
    // Same track looping on Repeat::Track or user restarting near the start
    // after listening substantially or already scrobbling.
    let threshold = (u64::from(duration_ms) / 2).min(MAX_SCROBBLE_THRESHOLD_MS);
    if prev.scrobbled && prev.last_position_ms > 15_000 && new_pos_ms < 5_000 {
        return true;
    }
    if u64::from(duration_ms) >= MIN_SCROBBLE_DURATION_MS
        && u64::from(prev.last_position_ms) >= threshold
        && new_pos_ms < 4_000
    {
        return true;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(id: &str, duration_ms: u32) -> ScrobbleTrackMetadata {
        ScrobbleTrackMetadata {
            id: id.into(),
            title: format!("Title {id}"),
            artist: "Pink Floyd".into(),
            album: Some("Animals".into()),
            album_artist: None,
            duration_ms,
        }
    }

    #[test]
    fn accumulates_only_while_playing_and_scrobbles_at_half_duration() {
        let mut tracker = ScrobbleTracker::new();
        let t = track("t1", 120_000); // 120s -> threshold 60s

        // Start playing at t=0
        let a = tracker.update(Some(&t), true, 0, false, true, 1_000_000);
        assert!(matches!(&a[..], [ScrobbleAction::SendNowPlaying(_)]));

        // Play 30s (in 5s steps -> accumulated = 30s)
        let mut now = 1_000_000;
        for _ in 0..6 {
            now += 5_000;
            let res = tracker.update(Some(&t), true, 30_000, false, true, now);
            assert!(res.is_empty());
        }

        // Pause for 120s -> must NOT accumulate!
        let paused = tracker.update(Some(&t), false, 30_000, false, true, now);
        assert!(paused.is_empty());
        now += 120_000;
        let still_paused = tracker.update(Some(&t), false, 30_000, false, true, now);
        assert!(still_paused.is_empty());

        // Resume -> baseline resets without counting paused 120s
        let resumed = tracker.update(Some(&t), true, 30_000, false, true, now);
        assert!(resumed.is_empty());

        // Play another 29s -> still under 60s
        for _ in 0..5 {
            now += 5_000;
            assert!(tracker.update(Some(&t), true, 55_000, false, true, now).is_empty());
        }
        now += 4_000;
        assert!(tracker.update(Some(&t), true, 59_000, false, true, now).is_empty());

        // 1 more second reaches 60_000ms -> submits scrobble once
        now += 1_000;
        let hit = tracker.update(Some(&t), true, 60_000, false, true, now);
        assert!(matches!(&hit[..], [ScrobbleAction::SubmitScrobble(_)]));

        // Subsequent ticks do NOT double-scrobble
        now += 5_000;
        assert!(tracker.update(Some(&t), true, 65_000, false, true, now).is_empty());
    }

    #[test]
    fn caps_threshold_at_240_seconds_for_long_tracks() {
        let mut tracker = ScrobbleTracker::new();
        let t = track("long", 900_000); // 15 min -> threshold min(450s, 240s) = 240s

        let mut now = 10_000;
        tracker.update(Some(&t), true, 0, false, true, now);

        for i in 1..=47 {
            now += 5_000; // 47 * 5s = 235s
            assert!(
                tracker.update(Some(&t), true, i * 5_000, false, true, now).is_empty(),
                "must not scrobble before 240s"
            );
        }

        now += 5_000; // 240s reached
        let hit = tracker.update(Some(&t), true, 240_000, false, true, now);
        assert!(matches!(&hit[..], [ScrobbleAction::SubmitScrobble(_)]));
    }

    #[test]
    fn ignores_tracks_shorter_than_30_seconds() {
        let mut tracker = ScrobbleTracker::new();
        let short = track("short", 25_000);
        let mut now = 1_000;
        tracker.update(Some(&short), true, 0, false, true, now);
        for _ in 0..5 {
            now += 5_000;
            assert!(tracker.update(Some(&short), true, 20_000, false, true, now).is_empty());
        }
    }

    #[test]
    fn connect_3s_poll_ticks_and_bursted_updates_do_not_double_count() {
        let mut tracker = ScrobbleTracker::new();
        let t = track("conn", 80_000); // threshold 40s

        // Initial Connect tick at t=0
        let a = tracker.update(Some(&t), true, 1_000, true, true, 100_000);
        assert_eq!(a.len(), 1);

        // Simulate duplicate poll/sync ticks firing at the exact same timestamp & 3s intervals
        for step in 1..=13 {
            let wall = 100_000 + step * 3_000; // up to 39s elapsed
            assert!(tracker.update(Some(&t), true, 1_000 + (step as u32) * 3_000, true, true, wall).is_empty());
            // Redundant tick 10ms later with stale position does not over-count
            assert!(tracker.update(Some(&t), true, 1_000 + (step as u32) * 3_000, true, true, wall + 10).is_empty());
        }

        // Real clock passes 40s total -> triggers once
        let res = tracker.update(Some(&t), true, 41_000, true, true, 140_500);
        assert!(matches!(&res[..], [ScrobbleAction::SubmitScrobble(_)]));
    }

    #[test]
    fn respects_remote_toggle_when_disabled() {
        let mut tracker = ScrobbleTracker::new();
        let t = track("rem", 120_000);
        assert!(tracker.update(Some(&t), true, 0, true, false, 10_000).is_empty());
        assert!(tracker.update(Some(&t), true, 5_000, true, false, 15_000).is_empty());
    }
}
