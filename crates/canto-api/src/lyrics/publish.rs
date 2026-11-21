//! Sending a listener's lyrics to LRCLIB (https://lrclib.net), the free
//! public lyrics library, without an account. LRCLIB asks for a proof of
//! work first: a challenge (a prefix and a target) and a number that, put
//! after the prefix, hashes (SHA-256) at or under the target. The app asks
//! the listener to confirm each sending before calling this.

use serde::Deserialize;
use sha2::{Digest, Sha256};

/// The lyrics and the song they are for, as LRCLIB files them.
#[derive(Clone, Debug, PartialEq)]
pub struct LyricsToPublish {
    pub track_name: String,
    pub artist_name: String,
    pub album_name: String,
    pub duration_secs: u32,
    pub plain_lyrics: String,
    pub synced_lyrics: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Challenge {
    prefix: String,
    target: String,
}

#[derive(Debug, Deserialize)]
struct LrclibError {
    message: Option<String>,
}

/// Whether `hash` is at or under `target`, both read as big numbers.
fn meets_target(hash: &[u8], target: &[u8]) -> bool {
    hash.len() == target.len() && hash <= target
}

/// The first number that, after `prefix`, hashes at or under `target`
/// (hex); `None` when the target isn't hex of a SHA-256's length.
pub fn solve_challenge(prefix: &str, target: &str) -> Option<u64> {
    let target = hex::decode(target).ok()?;
    if target.len() != 32 {
        return None;
    }
    (0u64..).find(|nonce| {
        let hash = Sha256::digest(format!("{prefix}{nonce}").as_bytes());
        meets_target(hash.as_slice(), &target)
    })
}

/// What LRCLIB's publish takes.
pub fn publish_body(lyrics: &LyricsToPublish) -> serde_json::Value {
    serde_json::json!({
        "trackName": lyrics.track_name,
        "artistName": lyrics.artist_name,
        "albumName": lyrics.album_name,
        "duration": lyrics.duration_secs,
        "plainLyrics": lyrics.plain_lyrics,
        "syncedLyrics": lyrics.synced_lyrics.clone().unwrap_or_default(),
    })
}

/// Sends `lyrics` to LRCLIB at `base` ("https://lrclib.net"): asks for a
/// challenge, solves it on a thread of its own (it can take a minute on a
/// slow computer), then publishes. The error is a sentence to show.
pub async fn publish(
    client: &reqwest::Client,
    base: &str,
    lyrics: &LyricsToPublish,
) -> Result<(), String> {
    let challenge: Challenge = client
        .post(format!("{base}/api/request-challenge"))
        .send()
        .await
        .and_then(reqwest::Response::error_for_status)
        .map_err(|e| format!("LRCLIB didn't answer: {e}"))?
        .json()
        .await
        .map_err(|e| format!("LRCLIB's answer couldn't be read: {e}"))?;

    let (sender, receiver) = futures::channel::oneshot::channel();
    let prefix = challenge.prefix.clone();
    let target = challenge.target.clone();
    std::thread::spawn(move || {
        let _ = sender.send(solve_challenge(&prefix, &target));
    });
    let nonce = receiver
        .await
        .ok()
        .flatten()
        .ok_or_else(|| "LRCLIB's challenge couldn't be solved".to_string())?;

    let response = client
        .post(format!("{base}/api/publish"))
        .header("X-Publish-Token", format!("{}:{nonce}", challenge.prefix))
        .json(&publish_body(lyrics))
        .send()
        .await
        .map_err(|e| format!("LRCLIB didn't answer: {e}"))?;
    if response.status().is_success() {
        return Ok(());
    }
    let status = response.status();
    let message = response
        .json::<LrclibError>()
        .await
        .ok()
        .and_then(|e| e.message)
        .unwrap_or_else(|| format!("LRCLIB answered {status}"));
    Err(message)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_server::TestServer;

    fn song() -> LyricsToPublish {
        LyricsToPublish {
            track_name: "Disco Voador".into(),
            artist_name: "Niink".into(),
            album_name: "Flowra FM".into(),
            duration_secs: 225,
            plain_lyrics: "First sample line\nSecond sample line".into(),
            synced_lyrics: Some(
                "[00:12.40] First sample line\n[00:15.10] Second sample line".into(),
            ),
        }
    }

    /// A target a hash meets about once in 16 tries
    const EASY: &str = "0fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff";

    #[test]
    fn the_number_found_hashes_under_the_target() {
        let nonce = solve_challenge("abc", EASY).expect("a nonce");
        let hash = Sha256::digest(format!("abc{nonce}").as_bytes());
        assert!(hash[0] < 0x10);
        // The first one: none before it meets the target
        for earlier in 0..nonce {
            let hash = Sha256::digest(format!("abc{earlier}").as_bytes());
            assert!(hash[0] >= 0x10);
        }
        assert_eq!(solve_challenge("abc", "zz"), None);
        assert_eq!(solve_challenge("abc", "0f"), None);
    }

    #[test]
    fn the_body_names_the_song_and_both_lyrics() {
        let body = publish_body(&song());
        assert_eq!(body["trackName"], "Disco Voador");
        assert_eq!(body["artistName"], "Niink");
        assert_eq!(body["albumName"], "Flowra FM");
        assert_eq!(body["duration"], 225);
        assert_eq!(body["plainLyrics"], "First sample line\nSecond sample line");
        assert!(body["syncedLyrics"]
            .as_str()
            .unwrap()
            .starts_with("[00:12.40]"));
        let mut plain = song();
        plain.synced_lyrics = None;
        assert_eq!(publish_body(&plain)["syncedLyrics"], "");
    }

    #[tokio::test]
    async fn publishing_solves_the_challenge_and_sends_the_token() {
        let challenge = format!(r#"{{"prefix":"abc","target":"{EASY}"}}"#);
        let server = TestServer::start(&[
            ("/api/request-challenge", 200, &challenge),
            ("/api/publish", 201, ""),
        ]);
        let client = crate::test_server::client();
        publish(&client, &server.base, &song())
            .await
            .expect("published");
        let nonce = solve_challenge("abc", EASY).unwrap();
        let sent = server.requests("/api/publish");
        assert_eq!(sent.len(), 1);
        let request = sent[0].to_ascii_lowercase();
        assert!(request.starts_with("post /api/publish"));
        assert!(request.contains(&format!("x-publish-token: abc:{nonce}")));
        assert!(sent[0].contains("\"trackName\":\"Disco Voador\""));
    }

    #[tokio::test]
    async fn a_refused_token_says_why() {
        let challenge = format!(r#"{{"prefix":"abc","target":"{EASY}"}}"#);
        let server = TestServer::start(&[
            ("/api/request-challenge", 200, &challenge),
            (
                "/api/publish",
                400,
                r#"{"code":400,"name":"IncorrectPublishTokenError","message":"The provided publish token is incorrect"}"#,
            ),
        ]);
        let client = crate::test_server::client();
        let error = publish(&client, &server.base, &song()).await.unwrap_err();
        assert_eq!(error, "The provided publish token is incorrect");
    }
}
