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

impl LastFmClient {
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
            let mut params = vec![
                ("method", method.to_string()),
                ("api_key", api_key.to_string()),
                ("username", username.to_string()),
                ("autocorrect", "1".to_string()),
                ("format", "json".to_string()),
            ];
            params.extend(extra);
            async move {
                let url = url::Url::parse_with_params(LASTFM_API_ROOT, &params)
                    .map_err(|e| DomainError::Parse(e.to_string()))?;
                let response = client
                    .get(url)
                    .header(reqwest::header::USER_AGENT, LASTFM_USER_AGENT)
                    .timeout(Duration::from_secs(12))
                    .send()
                    .await
                    .map_err(|e| DomainError::Network(e.without_url().to_string()))?;
                parse_lastfm_response(response).await
            }
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
        return Err(match code {
            9 => DomainError::AuthExpired,
            8 | 11 | 16 => DomainError::ServerError {
                status: 503,
                message,
            },
            29 => DomainError::RateLimited {
                retry_after_ms: None,
            },
            _ => DomainError::ClientError {
                status: status.as_u16().max(400),
                message,
            },
        });
    }

    if !status.is_success() {
        return Err(DomainError::ClientError {
            status: status.as_u16(),
            message: format!("Last.fm returned HTTP {}", status.as_u16()),
        });
    }

    Ok(val)
}

#[cfg(test)]
mod tests {
    use super::*;

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
            &[("method", "track.updateNowPlaying"), ("artist", "Café Tacvba"), ("track", "Eres")],
            "sec",
        );
        let b = sign_lastfm_params(
            &[("track", "Eres"), ("artist", "Café Tacvba"), ("method", "track.updateNowPlaying"), ("format", "xml")],
            "sec",
        );
        assert_eq!(a, b);
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
