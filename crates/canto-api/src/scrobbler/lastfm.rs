//! Last.fm Web API 2.0 client and MD5 `api_sig` signer.
//!
//! Credentials (`api_key`, `api_secret`, `session_key`) are never logged.

use std::time::Duration;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ScrobbleTrack;
use crate::error::DomainError;

const LASTFM_API_ROOT: &str = "https://ws.audioscrobbler.com/2.0/";
const LASTFM_AUTH_URL: &str = "https://www.last.fm/api/auth/";
const LASTFM_USER_AGENT: &str = "Canto/0.1 (https://github.com/gusthanks/canto)";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LastFmSession {
    pub key: String,
    pub username: String,
}

/// Sign Last.fm 2.0 parameters using MD5:
/// sort parameters alphabetically by key excluding `format` and `callback`,
/// concatenate `key + value`, append `api_secret`, and return lowercase MD5 hex.
pub fn sign_lastfm_params(params: &[(&str, &str)], api_secret: &str) -> String {
    let mut filtered: Vec<(&str, &str)> = params
        .iter()
        .copied()
        .filter(|&(k, _)| k != "format" && k != "callback")
        .collect();
    filtered.sort_by(|a, b| a.0.cmp(b.0).then_with(|| a.1.cmp(b.1)));

    let mut payload = String::new();
    for (key, value) in filtered {
        payload.push_str(key);
        payload.push_str(value);
    }
    payload.push_str(api_secret);

    let digest = Md5::digest(payload.as_bytes());
    hex::encode(digest)
}

/// Build the browser authorization URL for Last.fm desktop auth (`Step 1`).
pub fn build_auth_url(api_key: &str, token: &str) -> String {
    if let Ok(url) = url::Url::parse_with_params(
        LASTFM_AUTH_URL,
        &[("api_key", api_key.trim()), ("token", token.trim())],
    ) {
        return url.to_string();
    }
    LASTFM_AUTH_URL.to_string()
}

pub struct LastFmClient;

/// How many times the user has played a track, its album and its artist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Playcounts {
    pub track: Option<u64>,
    pub album: Option<u64>,
    pub artist: Option<u64>,
}

/// A count Last.fm sends as a number or a string ("37").
fn count(value: Option<&Value>) -> Option<u64> {
    match value? {
        Value::Number(n) => n.as_u64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

/// A time window of the user's listening stats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StatsPeriod {
    Week,
    Month,
    Year,
    Overall,
}

impl StatsPeriod {
    pub const ALL: [StatsPeriod; 4] = [Self::Week, Self::Month, Self::Year, Self::Overall];

    fn api_value(self) -> &'static str {
        match self {
            Self::Week => "7day",
            Self::Month => "1month",
            Self::Year => "12month",
            Self::Overall => "overall",
        }
    }
}

/// One line of a top list: a name, who made it (tracks and albums), how
/// many times the user played it and a picture when Last.fm has one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TopItem {
    pub name: String,
    pub artist: Option<String>,
    pub plays: u64,
    pub image: Option<String>,
}

/// The user's most played artists, tracks and albums in a period.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ListeningStats {
    pub artists: Vec<TopItem>,
    pub tracks: Vec<TopItem>,
    pub albums: Vec<TopItem>,
    /// How many different artists, tracks and albums (not plays)
    pub artist_count: Option<u64>,
    pub track_count: Option<u64>,
    pub album_count: Option<u64>,
}

/// Last.fm's placeholder picture (a grey star), served for every artist
/// and track since it dropped their images.
const LASTFM_PLACEHOLDER: &str = "2a96cbd8b46e442fc41c2b86b821562f";

/// The items of a `user.getTop*` answer (`root` like "topartists", `key` like "artist").
fn top_items(value: &Value, root: &str, key: &str) -> (Vec<TopItem>, Option<u64>) {
    let Some(list) = value.get(root) else {
        return (vec![], None);
    };
    let total = count(list.get("@attr").and_then(|a| a.get("total")));
    let items = list
        .get(key)
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    let name = item.get("name")?.as_str()?.trim().to_string();
                    if name.is_empty() {
                        return None;
                    }
                    let artist = item
                        .get("artist")
                        .and_then(|a| a.get("name").or(Some(a)))
                        .and_then(Value::as_str)
                        .map(str::to_string);
                    let image = item
                        .get("image")
                        .and_then(Value::as_array)
                        .and_then(|images| {
                            ["extralarge", "large", "medium"].iter().find_map(|size| {
                                images
                                    .iter()
                                    .find(|i| i.get("size").and_then(Value::as_str) == Some(size))
                                    .and_then(|i| i.get("#text")?.as_str())
                            })
                        })
                        .filter(|url| !url.is_empty() && !url.contains(LASTFM_PLACEHOLDER))
                        .map(str::to_string);
                    Some(TopItem {
                        name,
                        artist,
                        plays: count(item.get("playcount")).unwrap_or(0),
                        image,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    (items, total)
}

/// What Last.fm knows about the playing track, its album and its artist.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TrackInsights {
    /// The user's own plays (empty without a user name)
    pub playcounts: Playcounts,
    /// Whether the user loved the track on Last.fm
    pub loved: Option<bool>,
    pub track_listeners: Option<u64>,
    pub track_plays: Option<u64>,
    pub artist_listeners: Option<u64>,
    pub artist_plays: Option<u64>,
    /// Genre-like tags of the track, album and artist, most relevant first
    pub tags: Vec<String>,
    /// The artist's bio, as plain text
    pub artist_bio: Option<String>,
    /// The album's wiki text, as plain text
    pub album_wiki: Option<String>,
    pub similar_artists: Vec<String>,
    /// Similar tracks (`name` and `artist`)
    pub similar_tracks: Vec<TopItem>,
    /// The artist's most played tracks worldwide (`plays` is everyone's)
    pub top_tracks: Vec<TopItem>,
    pub artist_url: Option<String>,
    pub track_url: Option<String>,
    /// The user's total scrobbles
    pub scrobbles: Option<u64>,
    /// When the user joined Last.fm (Unix seconds)
    pub registered: Option<i64>,
}

impl TrackInsights {
    /// Whether there's anything to show.
    pub fn is_empty(&self) -> bool {
        *self
            == Self {
                scrobbles: self.scrobbles,
                registered: self.registered,
                ..Default::default()
            }
    }
}

/// Parameters of an `*.getInfo` call, the same for the play counts and the
/// insights so both share the memoized answer.
fn info_params(
    method: &'static str,
    api_key: &str,
    username: &str,
    extra: Vec<(&'static str, String)>,
) -> Vec<(&'static str, String)> {
    let mut params = vec![
        ("method", method.to_string()),
        ("api_key", api_key.to_string()),
    ];
    if !username.is_empty() {
        params.push(("username", username.to_string()));
    }
    params.push(("autocorrect", "1".to_string()));
    params.push(("format", "json".to_string()));
    params.extend(extra);
    params
}

/// Read-only answers kept for a few minutes, so the Now Playing card and the
/// Stats tab don't ask twice for the same track. Memory only.
const MEMO_TTL: Duration = Duration::from_secs(5 * 60);
const MEMO_MAX: usize = 48;
type Memo = std::sync::Mutex<Vec<(String, std::time::Instant, Value)>>;
static MEMO: std::sync::OnceLock<Memo> = std::sync::OnceLock::new();

async fn get_memoized(
    client: &reqwest::Client,
    params: &[(&'static str, String)],
) -> Result<Value, DomainError> {
    let url = url::Url::parse_with_params(LASTFM_API_ROOT, params)
        .map_err(|e| DomainError::Parse(e.to_string()))?;
    let key = url.as_str().to_string();
    let memo = MEMO.get_or_init(Default::default);
    if let Ok(mut entries) = memo.lock() {
        entries.retain(|(_, at, _)| at.elapsed() < MEMO_TTL);
        if let Some((_, _, value)) = entries.iter().find(|(k, _, _)| *k == key) {
            return Ok(value.clone());
        }
    }
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;
    let value = parse_lastfm_response(response).await?;
    if let Ok(mut entries) = memo.lock() {
        if entries.len() >= MEMO_MAX {
            entries.remove(0);
        }
        entries.push((key, std::time::Instant::now(), value.clone()));
    }
    Ok(value)
}

/// A Last.fm wiki or bio as plain text, without its "Read more on Last.fm"
/// link and license line. `None` when what's left is too short to be about
/// the music (track wikis are often notes about scrobbling glitches).
pub fn clean_lastfm_wiki(text: &str) -> Option<String> {
    let text = match text.find("<a href=\"https://www.last.fm") {
        Some(i) => &text[..i],
        None => text,
    };
    let text = match text.find("User-contributed text is available") {
        Some(i) => &text[..i],
        None => text,
    };
    let mut plain = String::with_capacity(text.len());
    let mut in_tag = false;
    for c in text.chars() {
        match c {
            '<' => in_tag = true,
            '>' if in_tag => in_tag = false,
            _ if !in_tag => plain.push(c),
            _ => {}
        }
    }
    let plain = plain
        .replace("&amp;", "&")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">");
    let paragraphs: Vec<String> = plain
        .split("\n\n")
        .map(|p| p.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|p| !p.is_empty())
        .collect();
    let out = paragraphs.join("\n\n");
    (out.chars().count() >= 80).then_some(out)
}

/// Names in `value[root][key][*].name`, at most `max`.
fn names_in(value: Option<&Value>, path: &str, max: usize) -> Vec<String> {
    value
        .and_then(|v| v.pointer(path))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|i| i.get("name")?.as_str())
                .map(str::trim)
                .filter(|n| !n.is_empty())
                .take(max)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Builds [`TrackInsights`] from the answers of `track.getInfo`,
/// `artist.getInfo`, `album.getInfo`, `track.getSimilar`,
/// `artist.getTopTracks` and `user.getInfo` (any may be missing).
pub fn parse_track_insights(
    track: Option<&Value>,
    artist: Option<&Value>,
    album: Option<&Value>,
    similar: Option<&Value>,
    top: Option<&Value>,
    user: Option<&Value>,
) -> TrackInsights {
    let at = |value: Option<&Value>, path: &str| value.and_then(|v| v.pointer(path)).cloned();
    let text = |value: Option<&Value>, path: &str| {
        at(value, path)
            .and_then(|v| v.as_str().map(str::to_string))
            .filter(|s| !s.trim().is_empty())
    };
    let mut tags: Vec<String> = Vec::new();
    for tag in names_in(track, "/track/toptags/tag", 8)
        .into_iter()
        .chain(names_in(album, "/album/tags/tag", 8))
        .chain(names_in(artist, "/artist/tags/tag", 8))
    {
        let tag = tag.to_lowercase();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags.truncate(8);
    let (similar_tracks, _) = similar.map_or((vec![], None), |v| {
        top_items(v, "similartracks", "track")
    });
    let (top_tracks, _) = top.map_or((vec![], None), |v| top_items(v, "toptracks", "track"));
    TrackInsights {
        playcounts: Playcounts {
            track: count(at(track, "/track/userplaycount").as_ref()),
            album: count(at(album, "/album/userplaycount").as_ref()),
            artist: count(at(artist, "/artist/stats/userplaycount").as_ref()),
        },
        loved: count(at(track, "/track/userloved").as_ref()).map(|n| n > 0),
        track_listeners: count(at(track, "/track/listeners").as_ref()),
        track_plays: count(at(track, "/track/playcount").as_ref()),
        artist_listeners: count(at(artist, "/artist/stats/listeners").as_ref()),
        artist_plays: count(at(artist, "/artist/stats/playcount").as_ref()),
        tags,
        artist_bio: text(artist, "/artist/bio/summary").and_then(|t| clean_lastfm_wiki(&t)),
        album_wiki: text(album, "/album/wiki/summary").and_then(|t| clean_lastfm_wiki(&t)),
        similar_artists: names_in(artist, "/artist/similar/artist", 8),
        similar_tracks: similar_tracks.into_iter().take(8).collect(),
        top_tracks: top_tracks.into_iter().take(5).collect(),
        artist_url: text(artist, "/artist/url"),
        track_url: text(track, "/track/url"),
        scrobbles: count(at(user, "/user/playcount").as_ref()),
        registered: count(at(user, "/user/registered/unixtime").as_ref())
            .or_else(|| count(at(user, "/user/registered/#text").as_ref()))
            .map(|t| t as i64),
    }
}

impl LastFmClient {
    /// The user's top artists, tracks and albums in `period`
    /// (`user.getTopArtists`, `user.getTopTracks`, `user.getTopAlbums`).
    pub async fn listening_stats(
        api_key: &str,
        username: &str,
        period: StatsPeriod,
        limit: usize,
    ) -> Result<ListeningStats, DomainError> {
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        let client = CLIENT.get_or_init(reqwest::Client::new).clone();
        let (api_key, username) = (api_key.trim(), username.trim());
        if api_key.is_empty() || username.is_empty() {
            return Err(DomainError::NoToken);
        }
        let get = |method: &'static str| {
            let params = [
                ("method", method.to_string()),
                ("api_key", api_key.to_string()),
                ("user", username.to_string()),
                ("period", period.api_value().to_string()),
                ("limit", limit.to_string()),
                ("format", "json".to_string()),
            ];
            let client = client.clone();
            async move {
                let url = url::Url::parse_with_params(LASTFM_API_ROOT, &params)
                    .map_err(|e| DomainError::Parse(e.to_string()))?;
                let response = client
                    .get(url)
                    .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
                    .timeout(Duration::from_secs(15))
                    .send()
                    .await
                    .map_err(|e| DomainError::Network(e.without_url().to_string()))?;
                parse_lastfm_response(response).await
            }
        };
        let (artists, tracks, albums) = futures::join!(
            get("user.getTopArtists"),
            get("user.getTopTracks"),
            get("user.getTopAlbums")
        );
        let (artists, artist_count) = top_items(&artists?, "topartists", "artist");
        let (tracks, track_count) = top_items(&tracks?, "toptracks", "track");
        let (albums, album_count) = top_items(&albums?, "topalbums", "album");
        Ok(ListeningStats {
            artists,
            tracks,
            albums,
            artist_count,
            track_count,
            album_count,
        })
    }

    /// Everything Last.fm says about the playing track, its album and artist
    /// (read-only calls; `username` may be empty, which leaves out the
    /// user's own numbers).
    pub async fn track_insights(
        api_key: &str,
        username: &str,
        artist: &str,
        track: &str,
        album: Option<&str>,
    ) -> Result<TrackInsights, DomainError> {
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        let client = CLIENT.get_or_init(reqwest::Client::new).clone();
        let (api_key, username) = (api_key.trim(), username.trim());
        if api_key.is_empty() {
            return Err(DomainError::NoToken);
        }
        let client = &client;
        let get = |method: &'static str, extra: Vec<(&'static str, String)>| {
            let params = info_params(method, api_key, username, extra);
            async move { get_memoized(client, &params).await }
        };
        let artist_s = artist.to_string();
        let track_s = track.to_string();
        let track_info = get(
            "track.getInfo",
            vec![("artist", artist_s.clone()), ("track", track_s.clone())],
        );
        let artist_info = get("artist.getInfo", vec![("artist", artist_s.clone())]);
        let album_info = async {
            match album {
                Some(album) if !album.trim().is_empty() => Some(
                    get(
                        "album.getInfo",
                        vec![("artist", artist_s.clone()), ("album", album.to_string())],
                    )
                    .await,
                ),
                _ => None,
            }
        };
        let similar = get(
            "track.getSimilar",
            vec![
                ("artist", artist_s.clone()),
                ("track", track_s.clone()),
                ("limit", "8".to_string()),
            ],
        );
        let top = get(
            "artist.getTopTracks",
            vec![("artist", artist_s.clone()), ("limit", "5".to_string())],
        );
        let user = async {
            if username.is_empty() {
                return None;
            }
            let params = vec![
                ("method", "user.getInfo".to_string()),
                ("api_key", api_key.to_string()),
                ("user", username.to_string()),
                ("format", "json".to_string()),
            ];
            Some(get_memoized(client, &params).await)
        };
        let (track_info, artist_info, album_info, similar, top, user) =
            futures::join!(track_info, artist_info, album_info, similar, top, user);
        // Nothing at all came back: say why (no network, a refused key...)
        let (track_info, artist_info) = match (track_info, artist_info) {
            (Err(_), Err(e)) => return Err(e),
            (track, artist) => (track.ok(), artist.ok()),
        };
        Ok(parse_track_insights(
            track_info.as_ref(),
            artist_info.as_ref(),
            album_info.and_then(Result::ok).as_ref(),
            similar.ok().as_ref(),
            top.ok().as_ref(),
            user.and_then(Result::ok).as_ref(),
        ))
    }

    /// [`Self::playcounts`] with a shared HTTP client.
    pub async fn user_playcounts(
        api_key: &str,
        username: &str,
        artist: &str,
        track: &str,
        album: Option<&str>,
    ) -> Result<Playcounts, DomainError> {
        static CLIENT: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        let client = CLIENT.get_or_init(reqwest::Client::new).clone();
        Self::playcounts(&client, api_key, username, artist, track, album).await
    }

    /// The user's play counts for a track, its album and its artist
    /// (`track.getInfo`, `album.getInfo`, `artist.getInfo` with `username`).
    pub async fn playcounts(
        client: &reqwest::Client,
        api_key: &str,
        username: &str,
        artist: &str,
        track: &str,
        album: Option<&str>,
    ) -> Result<Playcounts, DomainError> {
        let (api_key, username) = (api_key.trim(), username.trim());
        if api_key.is_empty() || username.is_empty() {
            return Err(DomainError::NoToken);
        }
        let get = |method: &'static str, extra: Vec<(&'static str, String)>| {
            let params = info_params(method, api_key, username, extra);
            async move { get_memoized(client, &params).await }
        };
        let artist_s = artist.to_string();
        let track_info = get(
            "track.getInfo",
            vec![("artist", artist_s.clone()), ("track", track.to_string())],
        );
        let artist_info = get("artist.getInfo", vec![("artist", artist_s.clone())]);
        let album_info = async {
            match album {
                Some(album) if !album.trim().is_empty() => Some(
                    get(
                        "album.getInfo",
                        vec![("artist", artist_s.clone()), ("album", album.to_string())],
                    )
                    .await,
                ),
                _ => None,
            }
        };
        let (track_info, artist_info, album_info) =
            futures::join!(track_info, artist_info, album_info);
        Ok(Playcounts {
            track: track_info
                .ok()
                .and_then(|v| count(v.pointer("/track/userplaycount"))),
            album: album_info
                .and_then(Result::ok)
                .and_then(|v| count(v.pointer("/album/userplaycount"))),
            artist: artist_info
                .ok()
                .and_then(|v| count(v.pointer("/artist/stats/userplaycount"))),
        })
    }

    /// Step 1 of Last.fm auth: request a short-lived auth token (`auth.getToken`).
    pub async fn get_token(
        client: &reqwest::Client,
        api_key: &str,
        api_secret: &str,
    ) -> Result<String, DomainError> {
        let api_key = api_key.trim();
        let api_secret = api_secret.trim();
        if api_key.is_empty() || api_secret.is_empty() {
            return Err(DomainError::NoToken);
        }

        let params = [("api_key", api_key), ("method", "auth.getToken")];
        let sig = sign_lastfm_params(&params, api_secret);
        let url = url::Url::parse_with_params(
            LASTFM_API_ROOT,
            &[
                ("method", "auth.getToken"),
                ("api_key", api_key),
                ("api_sig", sig.as_str()),
                ("format", "json"),
            ],
        )
        .map_err(|e| DomainError::Parse(e.to_string()))?;

        let response = client
            .get(url)
            .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
            .timeout(Duration::from_secs(12))
            .send()
            .await
            .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

        let val = parse_lastfm_response(response).await?;
        val.get("token")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToString::to_string)
            .ok_or_else(|| DomainError::Parse("Last.fm auth.getToken missing token".into()))
    }

    /// Step 2 of Last.fm auth: exchange the approved token for a persistent session key.
    pub async fn get_session(
        client: &reqwest::Client,
        api_key: &str,
        api_secret: &str,
        token: &str,
    ) -> Result<LastFmSession, DomainError> {
        let api_key = api_key.trim();
        let api_secret = api_secret.trim();
        let token = token.trim();
        if api_key.is_empty() || api_secret.is_empty() || token.is_empty() {
            return Err(DomainError::NoToken);
        }

        let params = [
            ("api_key", api_key),
            ("method", "auth.getSession"),
            ("token", token),
        ];
        let sig = sign_lastfm_params(&params, api_secret);
        let url = url::Url::parse_with_params(
            LASTFM_API_ROOT,
            &[
                ("method", "auth.getSession"),
                ("api_key", api_key),
                ("token", token),
                ("api_sig", sig.as_str()),
                ("format", "json"),
            ],
        )
        .map_err(|e| DomainError::Parse(e.to_string()))?;

        let response = client
            .get(url)
            .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
            .timeout(Duration::from_secs(12))
            .send()
            .await
            .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

        let val = parse_lastfm_response(response).await?;
        let session = val
            .get("session")
            .ok_or_else(|| DomainError::Parse("Last.fm auth.getSession missing session".into()))?;
        let key = session
            .get("key")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| DomainError::Parse("Last.fm session missing key".into()))?
            .to_string();
        let username = session
            .get("name")
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string();

        Ok(LastFmSession { key, username })
    }

    /// Submit `track.updateNowPlaying`.
    pub async fn update_now_playing(
        client: &reqwest::Client,
        api_key: &str,
        api_secret: &str,
        session_key: &str,
        track: &ScrobbleTrack,
    ) -> Result<(), DomainError> {
        let api_key = api_key.trim();
        let api_secret = api_secret.trim();
        let session_key = session_key.trim();
        if api_key.is_empty() || api_secret.is_empty() || session_key.is_empty() {
            return Err(DomainError::NoToken);
        }

        let duration_str = track.duration_secs.map(|d| d.to_string());
        let mut owned: Vec<(&str, &str)> = vec![
            ("api_key", api_key),
            ("artist", track.artist.as_str()),
            ("method", "track.updateNowPlaying"),
            ("sk", session_key),
            ("track", track.title.as_str()),
        ];
        if let Some(album) = track.album.as_deref().filter(|s| !s.is_empty()) {
            owned.push(("album", album));
        }
        if let Some(album_artist) = track.album_artist.as_deref().filter(|s| !s.is_empty()) {
            owned.push(("albumArtist", album_artist));
        }
        if let Some(dur) = duration_str.as_deref() {
            owned.push(("duration", dur));
        }

        post_signed(client, &owned, api_secret).await.map(|_| ())
    }

    /// Submit `track.scrobble`.
    pub async fn scrobble(
        client: &reqwest::Client,
        api_key: &str,
        api_secret: &str,
        session_key: &str,
        track: &ScrobbleTrack,
    ) -> Result<(), DomainError> {
        let api_key = api_key.trim();
        let api_secret = api_secret.trim();
        let session_key = session_key.trim();
        if api_key.is_empty() || api_secret.is_empty() || session_key.is_empty() {
            return Err(DomainError::NoToken);
        }

        let ts_str = track.started_at_unix.to_string();
        let duration_str = track.duration_secs.map(|d| d.to_string());
        let mut owned: Vec<(&str, &str)> = vec![
            ("api_key", api_key),
            ("artist", track.artist.as_str()),
            ("method", "track.scrobble"),
            ("sk", session_key),
            ("timestamp", ts_str.as_str()),
            ("track", track.title.as_str()),
        ];
        if let Some(album) = track.album.as_deref().filter(|s| !s.is_empty()) {
            owned.push(("album", album));
        }
        if let Some(album_artist) = track.album_artist.as_deref().filter(|s| !s.is_empty()) {
            owned.push(("albumArtist", album_artist));
        }
        if let Some(dur) = duration_str.as_deref() {
            owned.push(("duration", dur));
        }

        post_signed(client, &owned, api_secret).await.map(|_| ())
    }
}

async fn post_signed(
    client: &reqwest::Client,
    params: &[(&str, &str)],
    api_secret: &str,
) -> Result<Value, DomainError> {
    let sig = sign_lastfm_params(params, api_secret);
    let body = {
        let mut serializer = url::form_urlencoded::Serializer::new(String::new());
        for &(k, v) in params {
            serializer.append_pair(k, v);
        }
        serializer.append_pair("api_sig", &sig);
        serializer.append_pair("format", "json");
        serializer.finish()
    };

    let response = client
        .post(LASTFM_API_ROOT)
        .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .timeout(Duration::from_secs(12))
        .body(body)
        .send()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

    parse_lastfm_response(response).await
}

async fn parse_lastfm_response(response: reqwest::Response) -> Result<Value, DomainError> {
    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(DomainError::RateLimited {
            retry_after_ms: None,
        });
    }
    if status.is_server_error() {
        return Err(DomainError::ServerError {
            status: status.as_u16(),
            message: "Last.fm server error".into(),
        });
    }

    let val: Value =
        serde_json::from_slice(&bytes).map_err(|e| DomainError::Parse(e.to_string()))?;

    if let Some(code) = val.get("error").and_then(Value::as_u64) {
        let message = val
            .get("message")
            .and_then(Value::as_str)
            .unwrap_or("Last.fm API error")
            .to_string();
        return Err(lastfm_error(code, message, status.as_u16()));
    }

    if !status.is_success() {
        return Err(DomainError::ClientError {
            status: status.as_u16(),
            message: format!("Last.fm returned HTTP {}", status.as_u16()),
        });
    }

    Ok(val)
}

/// The error for a Last.fm error code (its API answers 200 with `error`).
fn lastfm_error(code: u64, message: String, http_status: u16) -> DomainError {
    match code {
        // Invalid session, invalid or suspended API key: the connection
        // itself is refused, not this one call
        9 | 10 | 26 => DomainError::AuthExpired,
        8 | 11 | 16 => DomainError::ServerError {
            status: 503,
            message,
        },
        29 => DomainError::RateLimited {
            retry_after_ms: None,
        },
        _ => DomainError::ClientError {
            status: http_status.max(400),
            message,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_refused_connection_is_told_apart_from_a_bad_scrobble() {
        for code in [9, 10, 26] {
            assert!(matches!(
                lastfm_error(code, String::new(), 200),
                DomainError::AuthExpired
            ));
        }
        // Bad parameters: this scrobble only
        assert!(matches!(
            lastfm_error(6, "Invalid parameters".into(), 200),
            DomainError::ClientError { status: 400, .. }
        ));
        assert!(lastfm_error(11, String::new(), 200).is_transient());
        assert!(lastfm_error(29, String::new(), 200).is_transient());
    }

    #[test]
    fn api_sig_sorts_alphabetically_excludes_format_and_callback_and_matches_md5() {
        let params = [
            ("track", "Time"),
            ("format", "json"),
            ("artist", "Pink Floyd"),
            ("callback", "ignored"),
            ("method", "track.scrobble"),
            ("api_key", "mykey"),
            ("timestamp", "1700000000"),
            ("sk", "mysession"),
        ];
        // Expected canonical concat:
        // api_keymykeyartistPink Floydmethodtrack.scrobbleskmysessiontimestamp1700000000trackTime + mysecret
        let expected_plain =
            "api_keymykeyartistPink Floydmethodtrack.scrobbleskmysessiontimestamp1700000000trackTimemysecret";
        let expected_hex = hex::encode(Md5::digest(expected_plain.as_bytes()));

        let sig = sign_lastfm_params(&params, "mysecret");
        assert_eq!(sig, expected_hex);
        assert_eq!(sig.len(), 32);
        assert!(sig.chars().all(|c| !c.is_ascii_uppercase()));
    }

    #[test]
    fn api_sig_handles_utf8_and_parameter_permutation_invariance() {
        let a = sign_lastfm_params(
            &[
                ("method", "track.updateNowPlaying"),
                ("artist", "Café Tacvba"),
                ("track", "Eres"),
            ],
            "sec",
        );
        let b = sign_lastfm_params(
            &[
                ("track", "Eres"),
                ("artist", "Café Tacvba"),
                ("method", "track.updateNowPlaying"),
                ("format", "xml"),
            ],
            "sec",
        );
        assert_eq!(a, b);
    }

    #[test]
    fn insights_read_world_numbers_tags_bio_similar_and_profile() {
        let track: Value = serde_json::from_str(
            r##"{"track":{"name":"Pink + White","url":"https://www.last.fm/music/Frank+Ocean/_/Pink+++White",
            "listeners":"5434","playcount":"16748","userplaycount":"12","userloved":"1",
            "toptags":{"tag":[{"name":"RnB"}]},
            "wiki":{"summary":"A glitch with scrobblers"}}}"##,
        )
        .unwrap();
        let artist: Value = serde_json::from_str(
            r##"{"artist":{"name":"Frank Ocean","url":"https://www.last.fm/music/Frank+Ocean",
            "stats":{"listeners":"4472285","playcount":"815125124","userplaycount":"2529"},
            "similar":{"artist":[{"name":"Daniel Caesar"},{"name":"SZA"}]},
            "tags":{"tag":[{"name":"rnb"},{"name":"soul"}]},
            "bio":{"summary":"Christopher Edwin Breaux, professionally known as Frank Ocean, is an American singer &amp; songwriter. <a href=\"https://www.last.fm/music/Frank+Ocean\">Read more on Last.fm</a>"}}}"##,
        )
        .unwrap();
        let album: Value = serde_json::from_str(
            r##"{"album":{"userplaycount":1832,"tags":{"tag":[{"name":"soul"},{"name":"2016"}]}}}"##,
        )
        .unwrap();
        let similar: Value = serde_json::from_str(
            r##"{"similartracks":{"track":[{"name":"Nikes","playcount":22058623,"artist":{"name":"Frank Ocean"},
            "image":[{"#text":"https://x/2a96cbd8b46e442fc41c2b86b821562f.png","size":"large"}]}]}}"##,
        )
        .unwrap();
        let top: Value = serde_json::from_str(
            r##"{"toptracks":{"track":[{"name":"Pink + White","playcount":"62021220","artist":{"name":"Frank Ocean"}}]}}"##,
        )
        .unwrap();
        let user: Value = serde_json::from_str(
            r##"{"user":{"playcount":"122741","registered":{"unixtime":"1548544192","#text":1548544192}}}"##,
        )
        .unwrap();
        let info = parse_track_insights(
            Some(&track),
            Some(&artist),
            Some(&album),
            Some(&similar),
            Some(&top),
            Some(&user),
        );
        assert_eq!(
            info.playcounts,
            Playcounts {
                track: Some(12),
                album: Some(1832),
                artist: Some(2529)
            }
        );
        assert_eq!(info.loved, Some(true));
        assert_eq!(info.track_listeners, Some(5434));
        assert_eq!(info.artist_plays, Some(815125124));
        assert_eq!(info.tags, vec!["rnb", "soul", "2016"]);
        let bio = info.artist_bio.clone().unwrap();
        assert!(bio.ends_with("singer & songwriter."), "{}", bio);
        assert_eq!(info.album_wiki, None);
        assert_eq!(info.similar_artists, vec!["Daniel Caesar", "SZA"]);
        assert_eq!(info.similar_tracks[0].name, "Nikes");
        assert_eq!(info.similar_tracks[0].artist.as_deref(), Some("Frank Ocean"));
        assert_eq!(info.similar_tracks[0].image, None);
        assert_eq!(info.top_tracks[0].plays, 62021220);
        assert_eq!(info.scrobbles, Some(122741));
        assert_eq!(info.registered, Some(1548544192));
        assert!(!info.is_empty());
        assert!(parse_track_insights(None, None, None, None, None, Some(&user)).is_empty());
    }

    #[test]
    fn short_or_link_only_wikis_are_dropped() {
        assert_eq!(clean_lastfm_wiki("A glitch with scrobblers"), None);
        assert_eq!(
            clean_lastfm_wiki(r#" <a href="https://www.last.fm/music/X">Read more on Last.fm</a>"#),
            None
        );
        let long = "Blonde is the second album by American singer Frank Ocean. It was released on August 20, 2016.\n\nSecond paragraph.";
        assert_eq!(clean_lastfm_wiki(long).as_deref(), Some(long));
    }

    #[test]
    fn info_params_leave_out_an_empty_user() {
        let with = info_params("track.getInfo", "k", "me", vec![("track", "t".into())]);
        assert!(with.iter().any(|(k, v)| *k == "username" && v == "me"));
        let without = info_params("track.getInfo", "k", "", vec![]);
        assert!(!without.iter().any(|(k, _)| *k == "username"));
        assert_eq!(without.last().map(|(k, _)| *k), Some("format"));
    }

    #[test]
    fn play_counts_come_as_numbers_or_strings() {
        let v: Value = serde_json::json!({"track": {"userplaycount": "37"}, "n": 5});
        assert_eq!(count(v.pointer("/track/userplaycount")), Some(37));
        assert_eq!(count(v.pointer("/n")), Some(5));
        assert_eq!(count(v.pointer("/missing")), None);
    }

    #[test]
    fn builds_lastfm_browser_auth_url() {
        assert_eq!(
            build_auth_url("abc123", "tok789"),
            "https://www.last.fm/api/auth/?api_key=abc123&token=tok789"
        );
    }
}

#[cfg(test)]
mod stats_tests {
    use super::*;

    #[test]
    fn top_lists_parse_and_skip_the_placeholder_picture() {
        let value: Value = serde_json::from_str(
            r##"{"toptracks":{"track":[
                {"name":"MEU CEMITÉRIO","playcount":"9","artist":{"name":"Matuê"},
                 "image":[{"size":"large","#text":"https://x/2a96cbd8b46e442fc41c2b86b821562f.png"}]},
                {"name":"","playcount":"1"}
            ],"@attr":{"total":"503"}}}"##,
        )
        .unwrap();
        let (items, total) = top_items(&value, "toptracks", "track");
        assert_eq!(total, Some(503));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].artist.as_deref(), Some("Matuê"));
        assert_eq!(items[0].plays, 9);
        assert_eq!(items[0].image, None);

        let value: Value = serde_json::from_str(
            r##"{"topalbums":{"album":[{"name":"XTRANHO","playcount":92,"artist":{"name":"Matuê"},
                "image":[{"size":"medium","#text":"https://x/m.png"},{"size":"extralarge","#text":"https://x/xl.png"}]}]}}"##,
        )
        .unwrap();
        let (items, _) = top_items(&value, "topalbums", "album");
        assert_eq!(items[0].image.as_deref(), Some("https://x/xl.png"));
        assert_eq!(items[0].plays, 92);
    }
}
