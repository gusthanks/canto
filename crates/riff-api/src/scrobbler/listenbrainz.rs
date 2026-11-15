//! ListenBrainz API client.
//!
//! User tokens are never logged.

use std::time::Duration;

use serde_json::{json, Value};

use super::ScrobbleTrack;
use crate::error::DomainError;

const LISTENBRAINZ_SUBMIT_URL: &str = "https://api.listenbrainz.org/1/submit-listens";
const LISTENBRAINZ_USER_AGENT: &str = "Canto/0.1 (https://github.com/gusthanks/canto)";

pub struct ListenBrainzClient;

impl ListenBrainzClient {
    pub fn build_playing_now_payload(track: &ScrobbleTrack) -> Value {
        json!({
            "listen_type": "playing_now",
            "payload": [{
                "track_metadata": build_metadata(track)
            }]
        })
    }

    pub fn build_scrobble_payload(track: &ScrobbleTrack) -> Value {
        json!({
            "listen_type": "single",
            "payload": [{
                "listened_at": track.started_at_unix,
                "track_metadata": build_metadata(track)
            }]
        })
    }

    pub async fn submit_playing_now(
        client: &reqwest::Client,
        token: &str,
        track: &ScrobbleTrack,
    ) -> Result<(), DomainError> {
        let payload = Self::build_playing_now_payload(track);
        post_listen(client, token, &payload).await
    }

    pub async fn submit_scrobble(
        client: &reqwest::Client,
        token: &str,
        track: &ScrobbleTrack,
    ) -> Result<(), DomainError> {
        let payload = Self::build_scrobble_payload(track);
        post_listen(client, token, &payload).await
    }
}

fn build_metadata(track: &ScrobbleTrack) -> Value {
    let mut additional = serde_json::Map::new();
    additional.insert("media_player".to_string(), json!("Canto"));
    if let Some(secs) = track.duration_secs {
        additional.insert("duration_ms".to_string(), json!(u64::from(secs) * 1000));
    }

    let mut meta = serde_json::Map::new();
    meta.insert("artist_name".to_string(), json!(track.artist));
    meta.insert("track_name".to_string(), json!(track.title));
    if let Some(album) = track.album.as_deref().filter(|s| !s.is_empty()) {
        meta.insert("release_name".to_string(), json!(album));
    }
    meta.insert("additional_info".to_string(), Value::Object(additional));
    Value::Object(meta)
}

async fn post_listen(
    client: &reqwest::Client,
    token: &str,
    payload: &Value,
) -> Result<(), DomainError> {
    let token = token.trim();
    if token.is_empty() {
        return Err(DomainError::NoToken);
    }

    let body = serde_json::to_vec(payload).map_err(|e| DomainError::Parse(e.to_string()))?;

    let response = client
        .post(LISTENBRAINZ_SUBMIT_URL)
        .header(reqwest::header::USER_AGENT, LISTENBRAINZ_USER_AGENT)
        .header(reqwest::header::AUTHORIZATION, format!("Token {token}"))
        .header(reqwest::header::CONTENT_TYPE, "application/json")
        .timeout(Duration::from_secs(12))
        .body(body)
        .send()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

    let status = response.status();
    if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
        return Err(DomainError::AuthExpired);
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Err(DomainError::RateLimited {
            retry_after_ms: None,
        });
    }
    if status.is_server_error() {
        return Err(DomainError::ServerError {
            status: status.as_u16(),
            message: "ListenBrainz server error".into(),
        });
    }
    if !status.is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(DomainError::ClientError {
            status: status.as_u16(),
            message: text,
        });
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_playing_now_and_single_payloads() {
        let track = ScrobbleTrack {
            title: "Wish You Were Here".into(),
            artist: "Pink Floyd".into(),
            album: Some("Wish You Were Here".into()),
            album_artist: None,
            duration_secs: Some(334),
            started_at_unix: 1_700_000_123,
        };

        let now = ListenBrainzClient::build_playing_now_payload(&track);
        assert_eq!(now["listen_type"], "playing_now");
        assert_eq!(
            now["payload"][0]["track_metadata"]["track_name"],
            "Wish You Were Here"
        );
        assert_eq!(
            now["payload"][0]["track_metadata"]["additional_info"]["duration_ms"],
            334_000
        );

        let single = ListenBrainzClient::build_scrobble_payload(&track);
        assert_eq!(single["listen_type"], "single");
        assert_eq!(single["payload"][0]["listened_at"], 1_700_000_123);
    }
}
