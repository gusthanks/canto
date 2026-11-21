#[macro_use]
extern crate log;

pub mod genius;
pub mod lyrics;
pub mod models;
pub mod radio;
pub mod scrobbler;

mod defaults;
mod error;
mod token;

pub(crate) mod cache;
pub(crate) mod http;
mod image;
pub(crate) mod providers;
mod scheduler;
pub(crate) mod service;

#[cfg(debug_assertions)]
pub(crate) mod dev;
#[cfg(test)]
mod test_server;

pub use error::DomainError;
pub use genius::{
    build_genius_search_url, normalize_genius_query_title, GeniusArtistInfo, GeniusBioStatus,
    GeniusCredit, GeniusRelationship, GeniusSongStats, GeniusTrackInfo, GeniusTranslation,
};
pub use lyrics::{
    FoundLyrics, LyricLine, Lyrics, LyricsCache, LyricsService, LyricsSource, LyricsToPublish,
};
pub use providers::spotify::domain::{check_user_profile, UserProfileCheck};
pub use providers::spotify::spotify_service;
pub use providers::{ProviderCapabilities, ProviderKind};
pub use radio::{build_radio_queue, DEFAULT_RADIO_TARGET_COUNT};
pub use reqwest::Client as HttpClient;
pub use scheduler::{Load, LoadPriority, BACKGROUND_EPOCH};
pub use service::ApiService;

#[cfg(debug_assertions)]
pub use dev::{is_simulate_offline, set_injected_error, set_simulate_offline};

pub use models::{Device, DeviceKind, PlayerState, Queue, RepeatMode};
pub use token::TokenProvider;
