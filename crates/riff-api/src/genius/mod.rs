//! Genius integration (search fallback + optional BYOK Client Access Token metadata).
//!
//! Never logs tokens or secrets.

use std::collections::HashSet;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::cache::disk::EntryState;
use crate::cache::DiskCache;
use crate::error::DomainError;

const GENIUS_API_BASE: &str = "https://api.genius.com";
const GENIUS_WEB_SEARCH: &str = "https://genius.com/search";
const GENIUS_USER_AGENT: &str = "Canto/0.1 (https://github.com/gusthanks/canto)";
pub const GENIUS_CACHE_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusRelationship {
    pub kind: String,
    pub label: String,
    pub tracks: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusTrackInfo {
    pub song_id: Option<u64>,
    pub url: String,
    pub is_exact_match: bool,
    pub full_title: Option<String>,
    pub album: Option<String>,
    pub release_date: Option<String>,
    pub featured_artists: Vec<String>,
    pub producers: Vec<String>,
    pub writers: Vec<String>,
    pub relationships: Vec<GeniusRelationship>,
    pub description: Option<String>,
}

/// Strip noisy edition/remaster/live/feature suffixes so Genius search matches cleanly.
pub fn normalize_genius_query_title(title: &str) -> String {
    let trimmed = title.trim();
    if trimmed.is_empty() {
        return String::new();
    }

    let mut current = strip_bracketed_editions(trimmed);

    // Iteratively strip trailing hyphenated edition descriptors like
    // " - 2011 Remaster", " - Live at Wembley", " - Radio Edit", etc.
    let dash_markers = [
        " - remaster",
        " - live",
        " - ao vivo",
        " - radio edit",
        " - single version",
        " - album version",
        " - deluxe",
        " - acoustic",
        " - instrumental",
        " - demo",
        " - explicit",
        " - clean",
        " - mono",
        " - stereo",
        " - bonus track",
        " - anniversary",
        " - special edition",
        " - extended",
        " - edit",
        " - version",
        " - mix",
        " - remix",
    ];

    loop {
        let lower = current.to_ascii_lowercase();
        let mut cut_at: Option<usize> = None;
        for marker in dash_markers {
            if let Some(idx) = lower.find(marker) {
                cut_at = Some(cut_at.map_or(idx, |prev| prev.min(idx)));
            }
        }
        if let Some(idx) = find_year_edition_dash(&lower) {
            cut_at = Some(cut_at.map_or(idx, |prev| prev.min(idx)));
        }

        let Some(idx) = cut_at else {
            break;
        };
        let prefix = current[..idx].trim();
        if prefix.is_empty() || prefix.len() == current.len() {
            break;
        }
        current = prefix.to_string();
    }

    let collapsed = current.split_whitespace().collect::<Vec<_>>().join(" ");
    if collapsed.is_empty() {
        trimmed.to_string()
    } else {
        collapsed
    }
}

fn strip_bracketed_editions(input: &str) -> String {
    let noise_keywords = [
        "remaster",
        "live",
        "ao vivo",
        "deluxe",
        "explicit",
        "clean",
        "feat.",
        "feat ",
        "ft.",
        "ft ",
        "with ",
        "radio edit",
        "single version",
        "album version",
        "acoustic",
        "instrumental",
        "demo",
        "bonus track",
        "anniversary",
        "special edition",
        "expanded edition",
        "mono",
        "stereo",
        "mix",
    ];

    let mut out = String::with_capacity(input.len());
    let chars: Vec<(usize, char)> = input.char_indices().collect();
    let mut i = 0;

    while i < chars.len() {
        let (byte_idx, ch) = chars[i];
        if ch == '(' || ch == '[' {
            let close = if ch == '(' { ')' } else { ']' };
            if let Some(end_pos) = chars[i + 1..].iter().position(|&(_, c)| c == close) {
                let end_idx_in_chars = i + 1 + end_pos;
                let inner_start = byte_idx + ch.len_utf8();
                let inner_end = chars[end_idx_in_chars].0;
                let inner_lower = input[inner_start..inner_end].to_ascii_lowercase();
                let is_edition_noise = noise_keywords.iter().any(|kw| inner_lower.contains(kw))
                    || starts_with_year(&inner_lower);
                if is_edition_noise {
                    i = end_idx_in_chars + 1;
                    continue;
                }
            }
        }
        out.push(ch);
        i += 1;
    }

    out.trim().to_string()
}

fn starts_with_year(s: &str) -> bool {
    let bytes = s.trim_start().as_bytes();
    bytes.len() >= 4
        && (bytes.starts_with(b"19") || bytes.starts_with(b"20"))
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
}

fn find_year_edition_dash(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let needle = b" - ";
    let mut pos = 0;
    while let Some(rel) = lower[pos..].find(" - ") {
        let idx = pos + rel;
        let after = idx + needle.len();
        if bytes.len() >= after + 4
            && (bytes[after..].starts_with(b"19") || bytes[after..].starts_with(b"20"))
            && bytes[after + 2].is_ascii_digit()
            && bytes[after + 3].is_ascii_digit()
        {
            return Some(idx);
        }
        pos = after;
    }
    None
}

/// Zero-key fallback URL opening Genius search directly with a clean query.
pub fn build_genius_search_url(artist: &str, title: &str) -> String {
    let clean_title = normalize_genius_query_title(title);
    let artist_clean = artist.trim();
    let query = match (artist_clean.is_empty(), clean_title.is_empty()) {
        (true, true) => String::new(),
        (false, true) => artist_clean.to_string(),
        (true, false) => clean_title,
        (false, false) => format!("{artist_clean} {clean_title}"),
    };
    if let Ok(url) = url::Url::parse_with_params(GENIUS_WEB_SEARCH, &[("q", query.as_str())]) {
        return url.to_string();
    }
    GENIUS_WEB_SEARCH.to_string()
}

pub fn genius_cache_key(track_id: &str, artist: &str, title: &str) -> String {
    if !track_id.trim().is_empty() {
        format!("genius_song_v1_{}", track_id.trim())
    } else {
        format!(
            "genius_song_v1_{}_{}",
            normalize_for_match(artist),
            normalize_for_match(title)
        )
    }
}

pub async fn fetch_genius_track_info(
    client: &reqwest::Client,
    cache: Option<&DiskCache>,
    track_id: &str,
    artist: &str,
    title: &str,
    token: &str,
) -> Result<GeniusTrackInfo, DomainError> {
    let token = token.trim();
    let fallback_url = build_genius_search_url(artist, title);
    if token.is_empty() {
        return Ok(empty_fallback(fallback_url));
    }

    let key = genius_cache_key(track_id, artist, title);
    if let Some(disk) = cache {
        if let Some(entry) = disk.read(&key).await {
            if matches!(entry.state, EntryState::Fresh) {
                if let Ok(cached) = serde_json::from_slice::<GeniusTrackInfo>(&entry.data) {
                    return Ok(cached);
                }
            }
        }
    }

    let clean_title = normalize_genius_query_title(title);
    let primary_artist = primary_artist_name(artist);
    let query = if primary_artist.is_empty() {
        clean_title.clone()
    } else {
        format!("{primary_artist} {clean_title}")
    };

    let search_url = url::Url::parse_with_params(
        &format!("{GENIUS_API_BASE}/search"),
        &[("q", query.as_str())],
    )
    .map_err(|e| DomainError::Parse(e.to_string()))?;

    let search_val = get_genius_json(client, search_url.as_str(), token).await?;
    let Some(song_id) = pick_best_hit(&search_val, primary_artist, &clean_title) else {
        let info = empty_fallback(fallback_url);
        write_cache(cache, &key, &info).await;
        return Ok(info);
    };

    let song_url = format!("{GENIUS_API_BASE}/songs/{song_id}?text_format=plain");
    let song_val = get_genius_json(client, &song_url, token).await?;
    let info = parse_genius_song_response(&song_val, song_id, fallback_url);
    write_cache(cache, &key, &info).await;
    Ok(info)
}

async fn write_cache(cache: Option<&DiskCache>, key: &str, info: &GeniusTrackInfo) {
    if let Some(disk) = cache {
        if let Ok(bytes) = serde_json::to_vec(info) {
            disk.write(key, &bytes, GENIUS_CACHE_TTL, None).await;
        }
    }
}

fn empty_fallback(fallback_url: String) -> GeniusTrackInfo {
    GeniusTrackInfo {
        song_id: None,
        url: fallback_url,
        is_exact_match: false,
        full_title: None,
        album: None,
        release_date: None,
        featured_artists: Vec::new(),
        producers: Vec::new(),
        writers: Vec::new(),
        relationships: Vec::new(),
        description: None,
    }
}

async fn get_genius_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<Value, DomainError> {
    let response = client
        .get(url)
        .header(reqwest::header::USER_AGENT, GENIUS_USER_AGENT)
        .bearer_auth(token)
        .timeout(Duration::from_secs(12))
        .send()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

    let status = response.status();
    let bytes = response
        .bytes()
        .await
        .map_err(|e| DomainError::Network(e.without_url().to_string()))?;

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
            message: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }
    if !status.is_success() {
        return Err(DomainError::ClientError {
            status: status.as_u16(),
            message: String::from_utf8_lossy(&bytes).into_owned(),
        });
    }

    serde_json::from_slice(&bytes).map_err(|e| DomainError::Parse(e.to_string()))
}

fn primary_artist_name(artist: &str) -> &str {
    artist.split(',').next().map(str::trim).unwrap_or(artist.trim())
}

fn normalize_for_match(input: &str) -> String {
    normalize_genius_query_title(input)
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c.is_whitespace() {
                c.to_ascii_lowercase()
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Rank Genius `/search` hits against expected artist & title and return best matching song id.
pub fn pick_best_hit(search_json: &Value, expected_artist: &str, expected_title: &str) -> Option<u64> {
    let hits = search_json
        .get("response")
        .and_then(|r| r.get("hits"))
        .and_then(Value::as_array)?;

    let target_artist = normalize_for_match(primary_artist_name(expected_artist));
    let target_title = normalize_for_match(expected_title);

    let mut best: Option<(i32, u64)> = None;
    for hit in hits {
        if hit.get("type").and_then(Value::as_str) != Some("song") {
            continue;
        }
        let Some(result) = hit.get("result") else {
            continue;
        };
        let Some(song_id) = result.get("id").and_then(Value::as_u64) else {
            continue;
        };

        let hit_title = result
            .get("title")
            .and_then(Value::as_str)
            .map(normalize_for_match)
            .unwrap_or_default();
        let hit_artist = result
            .get("primary_artist")
            .and_then(|a| a.get("name"))
            .and_then(Value::as_str)
            .map(normalize_for_match)
            .unwrap_or_default();
        let hit_artists = result
            .get("artist_names")
            .and_then(Value::as_str)
            .map(normalize_for_match)
            .unwrap_or_default();

        let score = score_hit(&target_artist, &target_title, &hit_artist, &hit_artists, &hit_title);
        if score < 40 {
            continue;
        }
        if best.is_none_or(|(best_score, _)| score > best_score) {
            best = Some((score, song_id));
        }
    }

    best.map(|(_, id)| id)
}

fn score_hit(
    target_artist: &str,
    target_title: &str,
    hit_artist: &str,
    hit_artists: &str,
    hit_title: &str,
) -> i32 {
    let mut score = 0;

    if !target_title.is_empty() {
        if hit_title == target_title {
            score += 60;
        } else if hit_title.contains(target_title) || target_title.contains(hit_title) {
            score += 35;
        } else {
            let target_words: HashSet<&str> = target_title.split_whitespace().collect();
            let hit_words: HashSet<&str> = hit_title.split_whitespace().collect();
            if !target_words.is_empty() {
                let overlap = target_words.intersection(&hit_words).count();
                score += ((overlap * 30) / target_words.len()) as i32;
            }
        }
    }

    if !target_artist.is_empty() {
        if hit_artist == target_artist {
            score += 50;
        } else if hit_artist.contains(target_artist)
            || target_artist.contains(hit_artist)
            || hit_artists.contains(target_artist)
        {
            score += 30;
        }
    }

    score
}

/// Parse `GET /songs/{id}?text_format=plain` payload cleanly.
pub fn parse_genius_song_response(
    song_json: &Value,
    song_id: u64,
    fallback_url: String,
) -> GeniusTrackInfo {
    let Some(song) = song_json.get("response").and_then(|r| r.get("song")) else {
        return empty_fallback(fallback_url);
    };

    let url = song
        .get("url")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
        .unwrap_or(fallback_url);

    let full_title = non_empty_str(song.get("full_title"));
    let album = song
        .get("album")
        .and_then(|a| non_empty_str(a.get("name")));
    let release_date = non_empty_str(song.get("release_date_for_display"))
        .or_else(|| non_empty_str(song.get("release_date")));

    let featured_artists = extract_artist_names(song.get("featured_artists"));
    let producers = extract_artist_names(song.get("producer_artists"));
    let writers = extract_artist_names(song.get("writer_artists"));
    let relationships = extract_relationships(song.get("song_relationships"));

    let description = song
        .get("description")
        .and_then(|d| non_empty_str(d.get("plain")))
        .filter(|text| text != "?");

    GeniusTrackInfo {
        song_id: Some(song_id),
        url,
        is_exact_match: true,
        full_title,
        album,
        release_date,
        featured_artists,
        producers,
        writers,
        relationships,
        description,
    }
}

fn non_empty_str(val: Option<&Value>) -> Option<String> {
    val.and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToString::to_string)
}

fn extract_artist_names(val: Option<&Value>) -> Vec<String> {
    let Some(arr) = val.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for item in arr {
        if let Some(name) = non_empty_str(item.get("name")) {
            if seen.insert(name.clone()) {
                out.push(name);
            }
        }
    }
    out
}

fn extract_relationships(val: Option<&Value>) -> Vec<GeniusRelationship> {
    let Some(arr) = val.and_then(Value::as_array) else {
        return Vec::new();
    };
    let tracked = [
        ("samples", "Samples"),
        ("sampled_in", "Sampled in"),
        ("interpolates", "Interpolates"),
        ("interpolated_by", "Interpolated in"),
        ("covered_by", "Covered by"),
        ("covers", "Cover of"),
    ];

    let mut out = Vec::new();
    for rel in arr {
        let Some(kind) = rel.get("relationship_type").and_then(Value::as_str) else {
            continue;
        };
        let Some((_, label)) = tracked.iter().find(|&&(k, _)| k == kind) else {
            continue;
        };
        let Some(songs) = rel.get("songs").and_then(Value::as_array) else {
            continue;
        };
        let mut tracks = Vec::new();
        for s in songs {
            if let Some(title) = non_empty_str(s.get("full_title"))
                .or_else(|| non_empty_str(s.get("title")))
            {
                if !tracks.contains(&title) {
                    tracks.push(title);
                }
            }
        }
        if !tracks.is_empty() {
            out.push(GeniusRelationship {
                kind: kind.to_string(),
                label: (*label).to_string(),
                tracks,
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_remastered_live_and_feat_suffixes() {
        assert_eq!(
            normalize_genius_query_title("Time - 2011 Remastered Version"),
            "Time"
        );
        assert_eq!(
            normalize_genius_query_title("Paranoid Android (Remastered)"),
            "Paranoid Android"
        );
        assert_eq!(
            normalize_genius_query_title("Numb [Live in Texas] (Explicit)"),
            "Numb"
        );
        assert_eq!(
            normalize_genius_query_title("Nights (feat. Frank Ocean)"),
            "Nights"
        );
        assert_eq!(
            normalize_genius_query_title("911 (feat. Tyler, The Creator)"),
            "911"
        );
    }

    #[test]
    fn preserves_meaningful_parentheses() {
        assert_eq!(
            normalize_genius_query_title("(Don't Fear) The Reaper"),
            "(Don't Fear) The Reaper"
        );
    }

    #[test]
    fn builds_clean_fallback_search_url() {
        let url = build_genius_search_url("Pink Floyd", "Time - 2011 Remastered");
        assert_eq!(url, "https://genius.com/search?q=Pink+Floyd+Time");
    }

    #[test]
    fn ranks_exact_search_hit_above_cover_or_remix() {
        let json = serde_json::json!({
            "response": {
                "hits": [
                    {
                        "type": "song",
                        "result": {
                            "id": 100,
                            "title": "Money (Acoustic Cover)",
                            "primary_artist": { "name": "Some Cover Band" },
                            "artist_names": "Some Cover Band"
                        }
                    },
                    {
                        "type": "song",
                        "result": {
                            "id": 200,
                            "title": "Money",
                            "primary_artist": { "name": "Pink Floyd" },
                            "artist_names": "Pink Floyd"
                        }
                    }
                ]
            }
        });
        assert_eq!(pick_best_hit(&json, "Pink Floyd", "Money - Remastered"), Some(200));
    }

    #[test]
    fn parses_song_credits_relationships_and_description() {
        let json = serde_json::json!({
            "response": {
                "song": {
                    "url": "https://genius.com/Pink-floyd-money-lyrics",
                    "full_title": "Money by Pink Floyd",
                    "release_date_for_display": "May 7, 1973",
                    "album": { "name": "The Dark Side of the Moon" },
                    "featured_artists": [{ "name": "Dick Parry" }],
                    "producer_artists": [{ "name": "Pink Floyd" }, { "name": "Alan Parsons" }],
                    "writer_artists": [{ "name": "Roger Waters" }],
                    "song_relationships": [
                        {
                            "relationship_type": "sampled_in",
                            "songs": [{ "full_title": "Cash In by Example" }]
                        },
                        {
                            "relationship_type": "samples",
                            "songs": []
                        }
                    ],
                    "description": { "plain": "A critique of greed written in 7/4 time." }
                }
            }
        });
        let info = parse_genius_song_response(&json, 200, "https://genius.com/search".into());
        assert!(info.is_exact_match);
        assert_eq!(info.url, "https://genius.com/Pink-floyd-money-lyrics");
        assert_eq!(info.album.as_deref(), Some("The Dark Side of the Moon"));
        assert_eq!(info.release_date.as_deref(), Some("May 7, 1973"));
        assert_eq!(info.producers, vec!["Pink Floyd", "Alan Parsons"]);
        assert_eq!(info.writers, vec!["Roger Waters"]);
        assert_eq!(info.featured_artists, vec!["Dick Parry"]);
        assert_eq!(info.relationships.len(), 1);
        assert_eq!(info.relationships[0].label, "Sampled in");
        assert_eq!(info.relationships[0].tracks, vec!["Cash In by Example"]);
        assert_eq!(
            info.description.as_deref(),
            Some("A critique of greed written in 7/4 time.")
        );
    }
}
