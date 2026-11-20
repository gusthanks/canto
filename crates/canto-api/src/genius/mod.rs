//! Genius integration (search fallback + optional BYOK Client Access Token metadata).
//!
//! Never logs tokens or secrets.

pub mod lyrics;

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

/// A credit beyond producers and writers ("Mastering Engineer", "Label",
/// "Additional Vocals"...): Genius's role name and who did it.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusCredit {
    pub role: String,
    pub names: Vec<String>,
}

/// A translation of the lyrics made by Genius users: only its link.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusTranslation {
    /// Language code, like "pt"
    pub language: String,
    /// What the translation calls itself ("Tradução em Português"), or the code
    pub label: String,
    pub url: String,
}

/// How much the song's Genius page is read and worked on.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusSongStats {
    pub pageviews: Option<u64>,
    pub contributors: Option<u64>,
    /// Reviewed explanations of lines
    pub annotations: Option<u64>,
    pub unreviewed_annotations: Option<u64>,
}

/// An artist on Genius (`GET /artists/{id}`).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GeniusArtistInfo {
    pub id: u64,
    pub name: String,
    pub url: String,
    pub description: Option<String>,
    /// The description as Pango markup, like [`GeniusTrackInfo::description_markup`]
    pub description_markup: Option<String>,
    pub alternate_names: Vec<String>,
    /// (kind, url): "website", "instagram", "youtube"...
    pub links: Vec<(String, String)>,
    pub followers: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Whether Genius reviewed the description (the song bio).
    #[serde(default)]
    pub description_status: Option<GeniusBioStatus>,
    /// The description as Pango markup, links kept; paragraphs are
    /// separated by a blank line.
    #[serde(default)]
    pub description_markup: Option<String>,
    /// Every other credit, in Genius's order
    #[serde(default)]
    pub credits: Vec<GeniusCredit>,
    #[serde(default)]
    pub recording_location: Option<String>,
    #[serde(default)]
    pub translations: Vec<GeniusTranslation>,
    #[serde(default)]
    pub stats: GeniusSongStats,
    /// The main artist's Genius id, for [`fetch_genius_artist_info`]
    #[serde(default)]
    pub artist_id: Option<u64>,
}

/// Genius's HTML (a song bio) as Pango markup: links (absolute, http or
/// https), italics and bold are kept, paragraphs end with a blank line,
/// everything else becomes plain text.
pub fn genius_html_to_markup(html: &str) -> String {
    let mut out = String::new();
    let mut rest = html;
    let mut open_links = 0usize;
    while !rest.is_empty() {
        let Some(lt) = rest.find('<') else {
            out.push_str(&escape_markup(&decode_entities(rest)));
            break;
        };
        out.push_str(&escape_markup(&decode_entities(&rest[..lt])));
        let Some(gt) = rest[lt..].find('>') else {
            out.push_str(&escape_markup(&decode_entities(&rest[lt..])));
            break;
        };
        let tag = &rest[lt + 1..lt + gt];
        rest = &rest[lt + gt + 1..];
        let closing = tag.starts_with('/');
        let name: String = tag
            .trim_start_matches('/')
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric())
            .collect::<String>()
            .to_ascii_lowercase();
        match (name.as_str(), closing) {
            ("a", false) => {
                if let Some(href) = attribute(tag, "href").and_then(|h| absolute_link(&h)) {
                    out.push_str(&format!("<a href=\"{}\">", escape_markup(&href)));
                    open_links += 1;
                }
            }
            ("a", true) => {
                if open_links > 0 {
                    out.push_str("</a>");
                    open_links -= 1;
                }
            }
            ("em" | "i", false) => out.push_str("<i>"),
            ("em" | "i", true) => out.push_str("</i>"),
            ("strong" | "b", false) => out.push_str("<b>"),
            ("strong" | "b", true) => out.push_str("</b>"),
            ("br", _) => out.push('\n'),
            ("p" | "blockquote" | "ul" | "ol" | "h1" | "h2" | "h3" | "h4", true) => {
                out.push_str("\n\n")
            }
            ("li", false) => out.push_str("• "),
            ("li", true) => out.push('\n'),
            _ => {}
        }
    }
    while open_links > 0 {
        out.push_str("</a>");
        open_links -= 1;
    }
    // At most one blank line between paragraphs, none at the ends
    let mut tidy = String::new();
    for paragraph in out.split("\n\n").map(str::trim).filter(|p| !p.is_empty()) {
        if !tidy.is_empty() {
            tidy.push_str("\n\n");
        }
        tidy.push_str(paragraph);
    }
    tidy
}

fn attribute(tag: &str, name: &str) -> Option<String> {
    let at = tag.find(&format!("{name}="))? + name.len() + 1;
    let value = &tag[at..];
    let quote = value.chars().next()?;
    if quote == '"' || quote == '\'' {
        let end = value[1..].find(quote)?;
        Some(decode_entities(&value[1..1 + end]))
    } else {
        Some(value.split_whitespace().next()?.to_string())
    }
}

fn absolute_link(href: &str) -> Option<String> {
    let href = href.trim();
    if href.starts_with("https://") || href.starts_with("http://") {
        Some(href.to_string())
    } else if href.starts_with('/') && !href.starts_with("//") {
        Some(format!("https://genius.com{href}"))
    } else {
        None
    }
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let decoded = rest.find(';').filter(|end| *end <= 10).and_then(|end| {
            let entity = &rest[1..end];
            let ch = match entity {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some(' '),
                _ => entity.strip_prefix('#').and_then(|num| {
                    let code = match num.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok()?,
                        None => num.parse().ok()?,
                    };
                    char::from_u32(code)
                }),
            }?;
            Some((ch, end))
        });
        match decoded {
            Some((ch, end)) => {
                out.push(ch);
                rest = &rest[end + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn escape_markup(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// How far Genius vouches for a song bio, as genius.com shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeniusBioStatus {
    /// Written by the community, not reviewed yet ("This song bio is unreviewed")
    Unreviewed,
    /// Accepted by Genius editors
    Reviewed,
    /// Written or confirmed by the artist
    Verified,
}

/// The status of the bio's annotation in a `/songs/{id}` payload.
fn bio_status(song: &Value) -> Option<GeniusBioStatus> {
    let annotation = song
        .get("description_annotation")?
        .get("annotations")?
        .as_array()?
        .first()?;
    if annotation.get("verified").and_then(Value::as_bool) == Some(true) {
        return Some(GeniusBioStatus::Verified);
    }
    Some(match annotation.get("state").and_then(Value::as_str) {
        Some("accepted") => GeniusBioStatus::Reviewed,
        _ => GeniusBioStatus::Unreviewed,
    })
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
        format!("genius_song_v3_{}", track_id.trim())
    } else {
        format!(
            "genius_song_v3_{}_{}",
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

    let song_url = format!("{GENIUS_API_BASE}/songs/{song_id}?text_format=plain,html");
    let song_val = get_genius_json(client, &song_url, token).await?;
    let info = parse_genius_song_response(&song_val, song_id, fallback_url);
    write_cache(cache, &key, &info).await;
    Ok(info)
}

/// A lyric fragment and what Genius says about it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct GeniusAnnotation {
    pub fragment: String,
    pub body: String,
    /// Reviewed by Genius editors, not only a contributor's note
    pub verified: bool,
}

/// The explanations Genius has for lines of a song (`GET /referents`).
pub async fn fetch_genius_annotations(
    client: &reqwest::Client,
    cache: Option<&DiskCache>,
    song_id: u64,
    token: &str,
) -> Result<Vec<GeniusAnnotation>, DomainError> {
    let key = format!("genius_annotations_{song_id}");
    if let Some(disk) = cache {
        if let Some(entry) = disk.read(&key).await {
            if matches!(entry.state, EntryState::Fresh) {
                if let Ok(cached) = serde_json::from_slice::<Vec<GeniusAnnotation>>(&entry.data) {
                    return Ok(cached);
                }
            }
        }
    }
    let mut annotations = Vec::new();
    for page in 1..=4 {
        let url = format!(
            "{GENIUS_API_BASE}/referents?song_id={song_id}&text_format=plain&per_page=50&page={page}"
        );
        let value = get_genius_json(client, &url, token.trim()).await?;
        let found = parse_genius_referents(&value);
        let done = found.len() < 50;
        annotations.extend(found);
        if done {
            break;
        }
    }
    if let Some(disk) = cache {
        if let Ok(bytes) = serde_json::to_vec(&annotations) {
            disk.write(&key, &bytes, GENIUS_CACHE_TTL, None).await;
        }
    }
    Ok(annotations)
}

/// Parses `GET /referents` into fragment + explanation pairs.
pub fn parse_genius_referents(value: &Value) -> Vec<GeniusAnnotation> {
    let Some(referents) = value
        .pointer("/response/referents")
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    referents
        .iter()
        .filter_map(|referent| {
            let fragment = referent.get("fragment")?.as_str()?.trim().to_string();
            let annotation = referent.get("annotations")?.as_array()?.first()?;
            let body = annotation
                .pointer("/body/plain")?
                .as_str()?
                .trim()
                .to_string();
            if fragment.is_empty() || body.is_empty() || body == "?" {
                return None;
            }
            let verified = annotation.get("state").and_then(Value::as_str) == Some("accepted")
                || annotation.get("verified").and_then(Value::as_bool) == Some(true);
            Some(GeniusAnnotation {
                fragment,
                body,
                verified,
            })
        })
        .collect()
}

/// Lowercase letters and digits only, for matching a fragment to a line.
fn fold(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

/// For each lyric line, the explanation of the fragment that starts on it.
pub fn annotations_for_lines(
    lines: &[String],
    annotations: &[GeniusAnnotation],
) -> Vec<Option<String>> {
    let folded: Vec<String> = lines.iter().map(|l| fold(l)).collect();
    let mut out = vec![None; lines.len()];
    for annotation in annotations {
        let first = annotation
            .fragment
            .lines()
            .map(fold)
            .find(|l| l.chars().count() >= 4);
        let Some(first) = first else { continue };
        let hit = folded.iter().position(|line| {
            line.chars().count() >= 4 && (line.contains(&first) || first.contains(line.as_str()))
        });
        if let Some(index) = hit {
            if out[index].is_none() {
                out[index] = Some(annotation.body.clone());
            }
        }
    }
    out
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
        url: fallback_url,
        ..Default::default()
    }
}

async fn get_genius_json(
    client: &reqwest::Client,
    url: &str,
    token: &str,
) -> Result<Value, DomainError> {
    get_json(client, url, Some(token)).await
}

/// GETs a Genius JSON answer, with the user's token when one is given (the
/// public web search needs none).
async fn get_json(
    client: &reqwest::Client,
    url: &str,
    token: Option<&str>,
) -> Result<Value, DomainError> {
    let mut request = client
        .get(url)
        .header(reqwest::header::USER_AGENT, GENIUS_USER_AGENT)
        .timeout(Duration::from_secs(12));
    if let Some(token) = token {
        request = request.bearer_auth(token);
    }
    let response = request
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
    artist
        .split(',')
        .next()
        .map(str::trim)
        .unwrap_or(artist.trim())
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
pub fn pick_best_hit(
    search_json: &Value,
    expected_artist: &str,
    expected_title: &str,
) -> Option<u64> {
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

        let score = score_hit(
            &target_artist,
            &target_title,
            &hit_artist,
            &hit_artists,
            &hit_title,
        );
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
    let album = song.get("album").and_then(|a| non_empty_str(a.get("name")));
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
    let description_status = description.as_ref().and_then(|_| bio_status(song));
    let description_markup = description
        .as_ref()
        .and_then(|_| non_empty_str(song.get("description").and_then(|d| d.get("html"))))
        .map(|html| genius_html_to_markup(&html))
        .filter(|markup| !markup.is_empty());

    let credits = extract_credits(song.get("custom_performances"));
    let recording_location = non_empty_str(song.get("recording_location"));
    let translations = extract_translations(song.get("translation_songs"));
    let stats = song
        .get("stats")
        .map(|s| GeniusSongStats {
            pageviews: s.get("pageviews").and_then(Value::as_u64),
            contributors: s.get("contributors").and_then(Value::as_u64),
            annotations: s.get("accepted_annotations").and_then(Value::as_u64),
            unreviewed_annotations: s.get("unreviewed_annotations").and_then(Value::as_u64),
        })
        .unwrap_or_default();
    let artist_id = song
        .get("primary_artist")
        .and_then(|a| a.get("id"))
        .and_then(Value::as_u64);

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
        description_status,
        description_markup,
        credits,
        recording_location,
        translations,
        stats,
        artist_id,
    }
}

/// `custom_performances`: every credit Genius has beyond the main ones.
fn extract_credits(val: Option<&Value>) -> Vec<GeniusCredit> {
    let Some(arr) = val.and_then(Value::as_array) else {
        return Vec::new();
    };
    arr.iter()
        .filter_map(|item| {
            let role = non_empty_str(item.get("label"))?;
            let names = extract_artist_names(item.get("artists"));
            (!names.is_empty()).then_some(GeniusCredit { role, names })
        })
        .collect()
}

/// `translation_songs`: links to the lyrics translated by Genius users.
fn extract_translations(val: Option<&Value>) -> Vec<GeniusTranslation> {
    let Some(arr) = val.and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut out: Vec<GeniusTranslation> = Vec::new();
    for item in arr {
        let Some(url) = non_empty_str(item.get("url")).filter(|u| u.starts_with("https://")) else {
            continue;
        };
        if out.iter().any(|t| t.url == url) {
            continue;
        }
        let language = non_empty_str(item.get("language")).unwrap_or_default();
        // "Artist - Song (Tradução em Português)": the part in parentheses
        let label = non_empty_str(item.get("title"))
            .and_then(|title| {
                let title = title.trim_end();
                let inner = title.strip_suffix(')')?;
                let open = inner.rfind('(')?;
                let label = inner[open + 1..].trim();
                (!label.is_empty()).then(|| label.to_string())
            })
            .unwrap_or_else(|| language.to_uppercase());
        if label.is_empty() {
            continue;
        }
        out.push(GeniusTranslation {
            language,
            label,
            url,
        });
    }
    out
}

/// The order artist links are shown in; unknown kinds come after.
const LINK_ORDER: &[&str] = &[
    "website",
    "instagram",
    "twitter",
    "facebook",
    "youtube",
    "tiktok",
    "soundcloud",
    "spotify",
    "apple_music",
    "tidal",
    "deezer",
    "amazon_music",
    "shazam",
];

/// The link for a social account, which Genius sends as a name or a URL.
fn social_url(kind: &str, value: &str) -> Option<String> {
    let value = value.trim().trim_start_matches('@');
    if value.is_empty() {
        return None;
    }
    if value.starts_with("https://") || value.starts_with("http://") {
        return Some(value.to_string());
    }
    if value.contains('/') || value.contains(' ') {
        return None;
    }
    let base = match kind {
        "instagram" => "https://www.instagram.com/",
        "twitter" => "https://x.com/",
        "facebook" => "https://www.facebook.com/",
        _ => return None,
    };
    Some(format!("{base}{value}"))
}

/// Parse `GET /artists/{id}?text_format=plain,html`.
pub fn parse_genius_artist_response(value: &Value) -> Option<GeniusArtistInfo> {
    let artist = value.pointer("/response/artist")?;
    let id = artist.get("id").and_then(Value::as_u64)?;
    let name = non_empty_str(artist.get("name"))?;
    let url = non_empty_str(artist.get("url")).unwrap_or_default();
    let description = artist
        .get("description")
        .and_then(|d| non_empty_str(d.get("plain")))
        .filter(|text| text != "?");
    let description_markup = description
        .as_ref()
        .and_then(|_| non_empty_str(artist.get("description").and_then(|d| d.get("html"))))
        .map(|html| genius_html_to_markup(&html))
        .filter(|markup| !markup.is_empty());
    let mut alternate_names: Vec<String> = Vec::new();
    for alias in artist
        .get("alternate_names")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let Some(alias) = non_empty_str(Some(alias)) {
            if alias != name && !alternate_names.contains(&alias) {
                alternate_names.push(alias);
            }
        }
    }
    let mut links: Vec<(String, String)> = Vec::new();
    let mut add = |kind: &str, value: &str| {
        if links.iter().any(|(k, _)| k == kind) {
            return;
        }
        if let Some(url) = social_url(kind, value) {
            links.push((kind.to_string(), url));
        }
    };
    if let Some(social) = artist.get("social_links").and_then(Value::as_object) {
        for (kind, value) in social {
            if let Some(value) = value.as_str() {
                add(kind, value);
            }
        }
    }
    for (kind, key) in [
        ("instagram", "instagram_name"),
        ("twitter", "twitter_name"),
        ("facebook", "facebook_name"),
    ] {
        if let Some(value) = artist.get(key).and_then(Value::as_str) {
            add(kind, value);
        }
    }
    links.sort_by_key(|(kind, _)| {
        LINK_ORDER
            .iter()
            .position(|k| k == kind)
            .unwrap_or(LINK_ORDER.len())
    });
    Some(GeniusArtistInfo {
        id,
        name,
        url,
        description,
        description_markup,
        alternate_names,
        links,
        followers: artist.get("followers_count").and_then(Value::as_u64),
    })
}

/// The artist's Genius page: bio, other names and links. Cached on disk.
pub async fn fetch_genius_artist_info(
    client: &reqwest::Client,
    cache: Option<&DiskCache>,
    artist_id: u64,
    token: &str,
) -> Result<Option<GeniusArtistInfo>, DomainError> {
    let token = token.trim();
    if token.is_empty() {
        return Ok(None);
    }
    let key = format!("genius_artist_v1_{artist_id}");
    if let Some(disk) = cache {
        if let Some(entry) = disk.read(&key).await {
            if matches!(entry.state, EntryState::Fresh) {
                if let Ok(cached) = serde_json::from_slice::<Option<GeniusArtistInfo>>(&entry.data)
                {
                    return Ok(cached);
                }
            }
        }
    }
    let url = format!("{GENIUS_API_BASE}/artists/{artist_id}?text_format=plain,html");
    let value = get_genius_json(client, &url, token).await?;
    let info = parse_genius_artist_response(&value);
    if let Some(disk) = cache {
        if let Ok(bytes) = serde_json::to_vec(&info) {
            disk.write(&key, &bytes, GENIUS_CACHE_TTL, None).await;
        }
    }
    Ok(info)
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
        ("cover_of", "Cover of"),
        ("covered_by", "Covered by"),
        ("remix_of", "Remix of"),
        ("remixed_by", "Remixed by"),
        ("live_version_of", "Live version of"),
        ("performed_live_as", "Performed live as"),
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
            if let Some(title) =
                non_empty_str(s.get("full_title")).or_else(|| non_empty_str(s.get("title")))
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
    #[test]
    fn referents_parse_and_land_on_their_lines() {
        let value = serde_json::json!({"response": {"referents": [
            {"fragment": "30 no comando", "annotations": [{"body": {"plain": "A gravadora dele"}, "state": "pending"}]},
            {"fragment": "Ahn, se eu quiser\nela deixa", "annotations": [{"body": {"plain": "Outra"}, "state": "accepted"}]},
            {"fragment": "x", "annotations": [{"body": {"plain": "?"}}]}
        ]}});
        let annotations = super::parse_genius_referents(&value);
        assert_eq!(annotations.len(), 2);
        assert!(annotations[1].verified);
        let lines = vec![
            "Yeah, 30 no comando!".to_string(),
            String::new(),
            "Ahn, se eu quiser".to_string(),
        ];
        let marked = super::annotations_for_lines(&lines, &annotations);
        assert_eq!(marked[0].as_deref(), Some("A gravadora dele"));
        assert_eq!(marked[1], None);
        assert_eq!(marked[2].as_deref(), Some("Outra"));
    }

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
        assert_eq!(
            pick_best_hit(&json, "Pink Floyd", "Money - Remastered"),
            Some(200)
        );
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
                    "description": { "plain": "A critique of greed written in 7/4 time." },
                    "description_annotation": {
                        "annotations": [{ "state": "pending", "verified": false }]
                    }
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
        assert_eq!(info.description_status, Some(GeniusBioStatus::Unreviewed));
    }

    #[test]
    fn parses_full_credits_translations_stats_and_more_relationships() {
        let json = serde_json::json!({
            "response": {
                "song": {
                    "url": "https://genius.com/Frank-ocean-pink-white-lyrics",
                    "recording_location": "Abbey Road Studios",
                    "primary_artist": { "id": 1985, "name": "Frank Ocean" },
                    "stats": {
                        "pageviews": 2555469,
                        "contributors": 270,
                        "accepted_annotations": 13,
                        "unreviewed_annotations": 1,
                        "hot": false
                    },
                    "custom_performances": [
                        { "label": "Mastering Engineer", "artists": [{ "name": "MIKE DEAN" }] },
                        { "label": "Additional Vocals", "artists": [{ "name": "Beyonc\u{e9}" }] },
                        { "label": "Empty", "artists": [] }
                    ],
                    "song_relationships": [
                        { "relationship_type": "cover_of", "songs": [{ "full_title": "A by B" }] },
                        { "relationship_type": "remixed_by", "songs": [{ "title": "A (Remix)" }] },
                        { "relationship_type": "translations", "songs": [{ "title": "skip" }] }
                    ],
                    "translation_songs": [
                        {
                            "language": "pt",
                            "title": "Frank Ocean - Pink + White (Tradu\u{e7}\u{e3}o em Portugu\u{ea}s)",
                            "url": "https://genius.com/pt-pink-white"
                        },
                        { "language": "es", "title": "No parentheses", "url": "https://genius.com/es" },
                        { "language": "fr", "title": "x", "url": "javascript:alert(1)" }
                    ]
                }
            }
        });
        let info = parse_genius_song_response(&json, 2842078, "https://genius.com/search".into());
        assert_eq!(
            info.recording_location.as_deref(),
            Some("Abbey Road Studios")
        );
        assert_eq!(info.artist_id, Some(1985));
        assert_eq!(info.stats.pageviews, Some(2555469));
        assert_eq!(info.stats.contributors, Some(270));
        assert_eq!(info.stats.annotations, Some(13));
        assert_eq!(info.credits.len(), 2, "a role with nobody is dropped");
        assert_eq!(info.credits[0].role, "Mastering Engineer");
        assert_eq!(info.credits[1].names, vec!["Beyonc\u{e9}"]);
        let kinds: Vec<&str> = info.relationships.iter().map(|r| r.kind.as_str()).collect();
        assert_eq!(kinds, vec!["cover_of", "remixed_by"]);
        assert_eq!(info.translations.len(), 2, "only https links");
        assert_eq!(
            info.translations[0].label,
            "Tradu\u{e7}\u{e3}o em Portugu\u{ea}s"
        );
        assert_eq!(info.translations[1].label, "ES");
    }

    #[test]
    fn old_cached_song_info_still_loads() {
        let old = r#"{"song_id":1,"url":"u","is_exact_match":true,"full_title":null,
            "album":null,"release_date":null,"featured_artists":[],"producers":["P"],
            "writers":[],"relationships":[],"description":null}"#;
        let info: GeniusTrackInfo = serde_json::from_str(old).unwrap();
        assert_eq!(info.producers, vec!["P"]);
        assert!(info.credits.is_empty());
        assert_eq!(info.stats, GeniusSongStats::default());
    }

    #[test]
    fn parses_an_artist_with_bio_names_and_links() {
        let json = serde_json::json!({
            "response": {
                "artist": {
                    "id": 1985,
                    "name": "Frank Ocean",
                    "url": "https://genius.com/artists/Frank-ocean",
                    "description": { "plain": "Singer.", "html": "<p>Singer.</p>" },
                    "alternate_names": ["Lonny Breaux", "Frank Ocean", "Lonny Breaux", ""],
                    "instagram_name": "blonded",
                    "twitter_name": null,
                    "facebook_name": "",
                    "followers_count": 7606,
                    "social_links": {
                        "youtube": "https://www.youtube.com/channel/x",
                        "website": "https://blonded.co/",
                        "facebook": "",
                        "instagram": "blonded"
                    }
                }
            }
        });
        let artist = parse_genius_artist_response(&json).unwrap();
        assert_eq!(artist.id, 1985);
        assert_eq!(artist.description.as_deref(), Some("Singer."));
        assert!(artist.description_markup.is_some());
        assert_eq!(artist.alternate_names, vec!["Lonny Breaux"]);
        assert_eq!(artist.followers, Some(7606));
        let kinds: Vec<&str> = artist.links.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(kinds, vec!["website", "instagram", "youtube"]);
        assert_eq!(artist.links[1].1, "https://www.instagram.com/blonded");
        assert!(parse_genius_artist_response(&serde_json::json!({})).is_none());
    }

    #[test]
    fn turns_a_song_bio_into_markup_with_links() {
        let html = r#"<p>&#8220;Nikes&#8221; is a <a href="https://genius.com/x" rel="noopener">music <em>video</em></a> &amp; more.</p>

<p>See <a href="/albums/y">this</a> &lt;3<br>bye <img src="z.png"></p>"#;
        assert_eq!(
            genius_html_to_markup(html),
            "\u{201c}Nikes\u{201d} is a <a href=\"https://genius.com/x\">music <i>video</i></a> &amp; more.\n\nSee <a href=\"https://genius.com/albums/y\">this</a> &lt;3\nbye"
        );
        assert_eq!(
            genius_html_to_markup(r#"<a href="javascript:x">bad</a> ok"#),
            "bad ok"
        );
    }

    #[test]
    fn reads_whether_the_song_bio_was_reviewed() {
        let status = |annotation: serde_json::Value| {
            bio_status(&serde_json::json!({
                "description_annotation": { "annotations": [annotation] }
            }))
        };
        assert_eq!(
            status(serde_json::json!({ "state": "accepted", "verified": false })),
            Some(GeniusBioStatus::Reviewed)
        );
        assert_eq!(
            status(serde_json::json!({ "state": "accepted", "verified": true })),
            Some(GeniusBioStatus::Verified)
        );
        assert_eq!(
            status(serde_json::json!({ "state": "pending" })),
            Some(GeniusBioStatus::Unreviewed)
        );
        assert_eq!(bio_status(&serde_json::json!({})), None);
    }
}
