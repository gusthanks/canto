//! Lyrics from Genius, for the songs LRCLIB has none for.
//!
//! The song is found by search, strictly (wrong lyrics are worse than none),
//! and its text is read from the embed Genius offers websites
//! (`/songs/{id}/embed.js`), never by scraping the song page. With the
//! user's token the official API searches; without one, the search
//! genius.com itself uses. Never logs tokens.

mod embed;
mod matching;
#[cfg(test)]
mod tests;

pub use embed::{parse_embed, unescape_js};
pub use matching::{pick_lyrics_hit, search_hits};

use std::time::Duration;

use log::debug;

use super::{
    get_genius_json, get_json, normalize_genius_query_title, primary_artist_name, GENIUS_API_BASE,
    GENIUS_USER_AGENT,
};
use crate::error::DomainError;
use matching::{match_key, only_sections};

/// The search genius.com uses itself: no token needed.
const PUBLIC_SEARCH_URL: &str = "https://genius.com/api/search/song";
const SONGS_URL: &str = "https://genius.com/songs";
const TIMEOUT: Duration = Duration::from_secs(12);
/// The biggest embed seen is about 53 KB; far past that it isn't lyrics.
const MAX_EMBED_BYTES: usize = 2 * 1024 * 1024;

/// A song in a Genius search, from either search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GeniusSongHit {
    pub id: u64,
    pub title: String,
    pub primary_artist: String,
    /// Everyone credited, like "Nora Vale (Ft. Dani Rocha)"
    pub artist_names: String,
    pub url: String,
    /// "complete", or "unreleased" while there are no lyrics
    pub lyrics_state: String,
    pub instrumental: bool,
}

/// Lyrics read from Genius.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GeniusLyrics {
    pub song_id: u64,
    /// The song's page
    pub url: String,
    /// Plain lines, `""` between stanzas
    pub lines: Vec<String>,
}

/// Where a lyrics look-up asks: genius.com, or a server in tests.
#[derive(Clone, Debug)]
pub struct GeniusEndpoints {
    /// genius.com's own search, which needs no token
    pub public_search: String,
    /// The official API's search, with the user's token
    pub api_search: String,
    /// Where `/{id}/embed.js` lives
    pub songs: String,
}

impl Default for GeniusEndpoints {
    fn default() -> Self {
        Self {
            public_search: PUBLIC_SEARCH_URL.to_string(),
            api_search: format!("{GENIUS_API_BASE}/search"),
            songs: SONGS_URL.to_string(),
        }
    }
}

/// The track's lyrics on Genius. `Ok(None)` when no song there clearly is
/// the track, or it has no lyrics (unreleased, instrumental); an error when
/// Genius can't be reached or answers in a way this doesn't understand.
/// `token` may be empty.
pub async fn fetch_genius_lyrics(
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    token: &str,
) -> Result<Option<GeniusLyrics>, DomainError> {
    fetch_genius_lyrics_from(&GeniusEndpoints::default(), client, artist, title, token).await
}

/// [`fetch_genius_lyrics`], asking `at`. Never fails with
/// [`DomainError::AuthExpired`]: the app answers that one by asking for a
/// new Spotify login, and Genius refusing a search is no reason to.
pub async fn fetch_genius_lyrics_from(
    at: &GeniusEndpoints,
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    token: &str,
) -> Result<Option<GeniusLyrics>, DomainError> {
    find_lyrics(at, client, artist, title, token)
        .await
        .map_err(|e| match e {
            DomainError::AuthExpired => DomainError::ClientError {
                status: 403,
                message: "Genius refused the request".into(),
            },
            e => e,
        })
}

async fn find_lyrics(
    at: &GeniusEndpoints,
    client: &reqwest::Client,
    artist: &str,
    title: &str,
    token: &str,
) -> Result<Option<GeniusLyrics>, DomainError> {
    let primary = primary_artist_name(artist);
    let clean_title = normalize_genius_query_title(title);
    if primary.is_empty() || clean_title.is_empty() || only_sections(&match_key(&clean_title)) {
        return Ok(None);
    }
    let mut token = token.trim();
    // "Artist title" usually puts the song first; when translation pages
    // crowd it out, "title artist" still finds it
    let queries = [
        format!("{primary} {clean_title}"),
        format!("{clean_title} {primary}"),
    ];
    for query in &queries {
        let hits = match search(client, at, query, token).await {
            Err(DomainError::AuthExpired) if !token.is_empty() => {
                debug!("Genius refused the token; searching without it");
                token = "";
                search(client, at, query, token).await?
            }
            result => result?,
        };
        let Some(hit) = pick_lyrics_hit(&hits, artist, title) else {
            continue;
        };
        let lines = match fetch_embed(client, at, hit.id).await? {
            Some(js) => parse_embed(&js)?,
            None => None,
        };
        return Ok(lines.map(|lines| GeniusLyrics {
            song_id: hit.id,
            url: song_page(hit),
            lines,
        }));
    }
    Ok(None)
}

/// One search: the official API with a token, else genius.com's own.
async fn search(
    client: &reqwest::Client,
    at: &GeniusEndpoints,
    query: &str,
    token: &str,
) -> Result<Vec<GeniusSongHit>, DomainError> {
    let value = if token.is_empty() {
        get_json(client, &search_url(&at.public_search, query)?, None).await?
    } else {
        get_genius_json(client, &search_url(&at.api_search, query)?, token).await?
    };
    Ok(search_hits(&value))
}

fn search_url(base: &str, query: &str) -> Result<String, DomainError> {
    url::Url::parse_with_params(base, &[("q", query)])
        .map(|url| url.to_string())
        .map_err(|e| DomainError::Parse(e.to_string()))
}

/// The song's embed.js, or None when Genius has no such song (404).
async fn fetch_embed(
    client: &reqwest::Client,
    at: &GeniusEndpoints,
    song_id: u64,
) -> Result<Option<String>, DomainError> {
    let network = |e: reqwest::Error| DomainError::Network(e.without_url().to_string());
    let mut response = client
        .get(format!("{}/{song_id}/embed.js", at.songs))
        .header(reqwest::header::USER_AGENT, GENIUS_USER_AGENT)
        .timeout(TIMEOUT)
        .send()
        .await
        .map_err(network)?;
    let status = response.status();
    if status == reqwest::StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(DomainError::RateLimited {
            retry_after_ms: None,
        });
    }
    if !status.is_success() {
        let message = "Genius embed".to_string();
        let status = status.as_u16();
        return Err(if status >= 500 {
            DomainError::ServerError { status, message }
        } else {
            DomainError::ClientError { status, message }
        });
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network)? {
        body.extend_from_slice(&chunk);
        if body.len() > MAX_EMBED_BYTES {
            return Err(DomainError::Parse("the Genius embed is too big".into()));
        }
    }
    Ok(Some(String::from_utf8_lossy(&body).into_owned()))
}

/// The song's page; only ever on genius.com, since the app opens it.
fn song_page(hit: &GeniusSongHit) -> String {
    if hit.url.starts_with("https://genius.com/") {
        hit.url.clone()
    } else {
        format!("{SONGS_URL}/{}", hit.id)
    }
}
