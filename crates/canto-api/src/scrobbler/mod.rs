//! Scrobbling support for Last.fm and ListenBrainz.
//!
//! Includes MD5 request signing for Last.fm 2.0, ListenBrainz JSON submission,
//! an active play-time tracker immune to pauses and Connect poll ticks, and a
//! bounded persistent offline retry queue (max 500 items).

pub mod lastfm;
pub mod listenbrainz;
pub mod queue;
pub mod tracker;

pub use lastfm::{
    build_auth_url, clean_lastfm_wiki, parse_track_insights, sign_lastfm_params, LastFmClient,
    LastFmSession, ListeningStats, Playcounts, StatsPeriod, TopItem, TrackInsights, TrackStats,
};
pub use listenbrainz::ListenBrainzClient;
pub use queue::{PendingScrobble, RetryQueue, ScrobbleTarget, MAX_QUEUE_ITEMS};
pub use tracker::{ScrobbleAction, ScrobbleTrackMetadata, ScrobbleTracker};

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScrobbleTrack {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub album_artist: Option<String>,
    pub duration_secs: Option<u32>,
    pub started_at_unix: i64,
}
