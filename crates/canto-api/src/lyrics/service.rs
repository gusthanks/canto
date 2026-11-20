use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use log::debug;
use serde::Deserialize;

use super::cache::{GeniusLyricsEntry, LyricsCache};
use super::lyricsfile;
use super::model::{FoundLyrics, Lyrics};
use super::parser::parse_lyrics;
use crate::error::DomainError;
use crate::genius::lyrics::{fetch_genius_lyrics_from, GeniusEndpoints, GeniusLyrics};

const USER_AGENT: &str = "Canto/0.1 (https://github.com/gusthanks/canto)";
const LRCLIB_GET_URL: &str = "https://lrclib.net/api/get";
const LRCLIB_SEARCH_URL: &str = "https://lrclib.net/api/search";
/// Each LRCLIB request gives up after this, so a stalled LRCLIB never keeps
/// the lyrics (or Genius's in its place) from showing for minutes.
const LRCLIB_TIMEOUT: Duration = Duration::from_secs(10);
/// A track neither LRCLIB nor Genius has lyrics for isn't asked about again
/// for this long (the reload button still asks).
const MISS_SECS: u64 = 24 * 60 * 60;
/// After Genius refuses a search or says "too many requests", it isn't
/// asked again for this long (the reload button still asks).
const GENIUS_REST_SECS: u64 = 15 * 60;

#[derive(Clone, Debug)]
pub struct LyricsService {
    client: reqwest::Client,
    cache: LyricsCache,
    translator: super::translate::LyricsTranslator,
    endpoints: Endpoints,
    memory: Arc<Memory>,
}

/// Where the look-ups go: the real services, or a server in tests.
#[derive(Clone, Debug)]
struct Endpoints {
    lrclib_get: String,
    lrclib_search: String,
    genius: GeniusEndpoints,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            lrclib_get: LRCLIB_GET_URL.to_string(),
            lrclib_search: LRCLIB_SEARCH_URL.to_string(),
            genius: GeniusEndpoints::default(),
        }
    }
}

/// What the service keeps in memory between look-ups.
#[derive(Debug, Default)]
struct Memory {
    misses: Misses,
    /// One look-up per track at a time: the lyrics page and the Now Playing
    /// card asking together share the first one's answer
    turns: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    /// Genius isn't asked before this (Unix seconds)
    genius_rests_until: AtomicU64,
}

impl Memory {
    fn turn(&self, key: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut turns = self.turns.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(turns.entry(key.to_string()).or_default())
    }

    /// Forgets `key`'s turn once no other look-up waits for it.
    fn end_turn(&self, key: &str, turn: Arc<tokio::sync::Mutex<()>>) {
        let mut turns = self.turns.lock().unwrap_or_else(PoisonError::into_inner);
        // Only this one and the map hold it
        if Arc::strong_count(&turn) <= 2 {
            turns.remove(key);
        }
    }
}

/// Tracks neither source had lyrics for, so playing one again shows "No
/// Lyrics" at once instead of asking both again. In memory only.
#[derive(Debug, Default)]
struct Misses(Mutex<HashMap<String, Miss>>);

#[derive(Clone, Copy, Debug)]
struct Miss {
    at: u64,
    /// Whether Genius was asked too (with the fallback on)
    genius: bool,
}

impl Misses {
    /// Whether `key` was a miss lately, with Genius asked too if `genius_on`.
    fn remembers(&self, key: &str, genius_on: bool, now: u64) -> bool {
        let misses = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        misses
            .get(LyricsCache::normalize_key(key))
            .is_some_and(|miss| {
                (miss.genius || !genius_on) && now.saturating_sub(miss.at) < MISS_SECS
            })
    }

    fn remember(&self, key: &str, genius_on: bool, now: u64) {
        let mut misses = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        let miss = Miss {
            at: now,
            genius: genius_on,
        };
        misses.insert(LyricsCache::normalize_key(key).to_string(), miss);
    }

    fn forget(&self, key: &str) {
        let mut misses = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        misses.remove(LyricsCache::normalize_key(key));
    }
}

/// What LRCLIB said about a track.
#[derive(Debug, PartialEq)]
enum LrclibAnswer {
    Found(Lyrics),
    /// It answered every query, and has none
    NotFound,
    /// Some query got no answer or an error: it may have them
    Failed,
}

/// One LRCLIB request's outcome.
#[derive(Debug)]
enum Reply {
    Lyrics(Lyrics),
    /// Not found, or found without lyrics
    None,
    /// An error answer: the next query may still work
    Error,
    /// No answer at all, or "too many requests": the next query would fare
    /// no better
    Unreachable,
}

/// LRCLIB's replies along the chain of ever looser queries.
#[derive(Debug, Default)]
struct Chain {
    lyrics: Option<Lyrics>,
    failed: bool,
}

impl Chain {
    /// Takes one reply; true when the chain is over (lyrics, or LRCLIB
    /// can't be reached).
    fn take(&mut self, reply: Reply) -> bool {
        match reply {
            Reply::Lyrics(lyrics) => {
                self.lyrics = Some(lyrics);
                true
            }
            Reply::None => false,
            Reply::Error => {
                self.failed = true;
                false
            }
            Reply::Unreachable => {
                self.failed = true;
                true
            }
        }
    }

    fn answer(self) -> LrclibAnswer {
        match self.lyrics {
            Some(lyrics) => LrclibAnswer::Found(lyrics),
            None if self.failed => LrclibAnswer::Failed,
            None => LrclibAnswer::NotFound,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(rename_all = "camelCase")]
struct LrclibItem {
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
    pub instrumental: Option<bool>,
    pub duration: Option<f64>,
    /// Whether `lyricsfile` has the time of each word.
    pub has_word_sync: Option<bool>,
    /// The lyrics as a Lyricsfile (YAML), with word timings when synced.
    pub lyricsfile: Option<String>,
}

impl LrclibItem {
    /// The item's lyrics, with real word timings when it has them.
    fn lyrics(&self) -> Option<Lyrics> {
        let parsed = parse_lyrics(self.synced_lyrics.as_deref(), self.plain_lyrics.as_deref())?;
        if self.has_word_sync != Some(true) {
            return Some(parsed);
        }
        let Lyrics::Synced(lrc) = &parsed else {
            return Some(parsed);
        };
        let words = self
            .lyricsfile
            .as_deref()
            .and_then(lyricsfile::parse)
            .and_then(|file| lyricsfile::to_lines(&file, lrc));
        match words {
            Some(lines) => Some(Lyrics::Synced(lines)),
            None => {
                debug!("this Lyricsfile could not be read; using its LRC lines");
                Some(parsed)
            }
        }
    }

    fn has_words(&self) -> bool {
        self.has_word_sync == Some(true) && self.synced_lyrics.is_some()
    }
}

fn clean_track_title(title: &str) -> Option<String> {
    let mut current = title.trim().to_string();
    let lower_patterns = [
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
        " (remaster",
        " (live",
        " (ao vivo",
        " (radio edit",
        " (single version",
        " (album version",
        " (deluxe",
        " (acoustic",
        " (instrumental",
        " (explicit",
        " (feat.",
        " (ft.",
        " (with ",
        " [remaster",
        " [live",
        " [ao vivo",
        " [deluxe",
        " [explicit",
        " [feat.",
        " [ft.",
        " [with ",
    ];

    loop {
        let lower = current.to_ascii_lowercase();
        let mut cut_at: Option<usize> = None;

        for pattern in lower_patterns {
            if let Some(idx) = lower.find(pattern) {
                cut_at = Some(cut_at.map_or(idx, |prev| prev.min(idx)));
            }
        }

        // Also match year-prefixed dash suffixes such as " - 2011 Remaster" or " - 2023 Mix".
        if let Some(year_idx) = find_year_edition_dash(&lower) {
            cut_at = Some(cut_at.map_or(year_idx, |prev| prev.min(year_idx)));
        }
        // Match year-prefixed parenthetical editions like " (2011 Remaster)" or " (2009 Mix)".
        if let Some(year_paren_idx) = find_year_edition_paren(&lower) {
            cut_at = Some(cut_at.map_or(year_paren_idx, |prev| prev.min(year_paren_idx)));
        }

        let Some(idx) = cut_at else {
            break;
        };
        let next = current[..idx].trim();
        if next.is_empty() || next.len() == current.len() {
            break;
        }
        current = next.to_string();
    }

    (current != title.trim() && !current.is_empty()).then_some(current)
}

fn find_year_edition_dash(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let needle = b" - ";
    let mut i = 0;
    while i + 8 <= bytes.len() {
        if &bytes[i..i + 3] == needle && bytes[i + 3..i + 7].iter().all(|b| b.is_ascii_digit()) {
            let tail = &lower[i + 7..];
            if tail.contains("remaster")
                || tail.contains("mix")
                || tail.contains("version")
                || tail.contains("edit")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

fn find_year_edition_paren(lower: &str) -> Option<usize> {
    let bytes = lower.as_bytes();
    let needle = b" (";
    let mut i = 0;
    while i + 7 <= bytes.len() {
        if &bytes[i..i + 2] == needle && bytes[i + 2..i + 6].iter().all(|b| b.is_ascii_digit()) {
            let tail = &lower[i + 6..];
            if tail.contains("remaster")
                || tail.contains("mix")
                || tail.contains("version")
                || tail.contains("edition")
            {
                return Some(i);
            }
        }
        i += 1;
    }
    None
}

impl Default for LyricsService {
    fn default() -> Self {
        Self::new()
    }
}

impl LyricsService {
    pub fn new() -> Self {
        Self::with_cache(LyricsCache::new())
    }

    pub fn with_cache(cache: LyricsCache) -> Self {
        let client = reqwest::Client::builder()
            .user_agent(USER_AGENT)
            .build()
            .unwrap_or_else(|_| reqwest::Client::new());
        Self {
            translator: super::translate::LyricsTranslator::new(client.clone()),
            client,
            cache,
            endpoints: Endpoints::default(),
            memory: Arc::default(),
        }
    }

    /// A service asking LRCLIB at `lrclib` and Genius at `genius` (test
    /// servers' addresses).
    #[cfg(test)]
    fn for_tests(cache: LyricsCache, lrclib: &str, genius: &str) -> Self {
        let client = crate::test_server::client();
        Self {
            translator: super::translate::LyricsTranslator::new(client.clone()),
            client,
            cache,
            endpoints: Endpoints {
                lrclib_get: format!("{lrclib}/api/get"),
                lrclib_search: format!("{lrclib}/api/search"),
                genius: GeniusEndpoints {
                    public_search: format!("{genius}/api/search/song"),
                    api_search: format!("{genius}/official/search"),
                    songs: format!("{genius}/songs"),
                },
            },
            memory: Arc::default(),
        }
    }

    pub fn cache(&self) -> &LyricsCache {
        &self.cache
    }

    pub fn translator(&self) -> &super::translate::LyricsTranslator {
        &self.translator
    }

    /// Fetch lyrics for a track: the disk cache first (0ms), then LRCLIB.
    /// With `genius` set (the Genius fallback on; the user's token, which
    /// may be empty) the lyrics Genius has, without timing, when LRCLIB has
    /// none. `None` turns the fallback off.
    pub async fn fetch_lyrics(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
        genius: Option<&str>,
    ) -> Option<FoundLyrics> {
        self.fetch_lyrics_inner(
            key,
            track_name,
            artist_name,
            album_name,
            duration_secs,
            genius,
            false,
        )
        .await
    }

    /// Refetch lyrics bypassing the copies on disk (LRCLIB's and Genius's),
    /// which are only used when nothing can be fetched.
    pub async fn refetch_lyrics(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
        genius: Option<&str>,
    ) -> Option<FoundLyrics> {
        self.fetch_lyrics_inner(
            key,
            track_name,
            artist_name,
            album_name,
            duration_secs,
            genius,
            true,
        )
        .await
    }

    /// The lyrics already on disk, for showing them at once: LRCLIB's, or
    /// else Genius's while fresh when the Genius fallback is on.
    pub fn cached_lyrics(&self, key: &str, genius_fallback: bool) -> Option<FoundLyrics> {
        saved_lyrics(&self.cache, key, genius_fallback, unix_now())
    }

    #[allow(clippy::too_many_arguments)]
    async fn fetch_lyrics_inner(
        &self,
        key: &str,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
        genius: Option<&str>,
        force_refresh: bool,
    ) -> Option<FoundLyrics> {
        let track_name = track_name.trim();
        let artist_name = artist_name.trim();
        if track_name.is_empty() || artist_name.is_empty() {
            return None;
        }
        // One look-up per track at a time: a second one waits, then finds
        // the first one's answer on disk (or remembered) without asking
        let turn_key = LyricsCache::normalize_key(key);
        let turn = self.memory.turn(turn_key);
        let found = {
            let _turn = turn.lock().await;
            // Neither runs unless `resolve` gets to it
            let lrclib = self.query_lrclib(track_name, artist_name, album_name, duration_secs);
            let from_genius = self.genius_lyrics(artist_name, track_name, genius, force_refresh);
            resolve(
                &self.cache,
                &self.memory.misses,
                key,
                genius.is_some(),
                force_refresh,
                unix_now(),
                lrclib,
                from_genius,
            )
            .await
        };
        self.memory.end_turn(turn_key, turn);
        found
    }

    /// Genius's lyrics for the track (`token`: the user's, maybe empty;
    /// None with the fallback off). After Genius refuses a search or asks
    /// to slow down, it rests a while unless the user reloads.
    async fn genius_lyrics(
        &self,
        artist: &str,
        title: &str,
        token: Option<&str>,
        force_refresh: bool,
    ) -> Result<Option<GeniusLyrics>, DomainError> {
        let Some(token) = token else {
            return Ok(None);
        };
        let now = unix_now();
        if !force_refresh && now < self.memory.genius_rests_until.load(Ordering::Relaxed) {
            return Err(DomainError::RateLimited {
                retry_after_ms: None,
            });
        }
        let found =
            fetch_genius_lyrics_from(&self.endpoints.genius, &self.client, artist, title, token)
                .await;
        if matches!(
            found,
            Err(DomainError::RateLimited { .. }
                | DomainError::ClientError {
                    status: 401 | 403,
                    ..
                })
        ) {
            debug!("Genius refused the lyrics search; resting a while");
            self.memory
                .genius_rests_until
                .store(now + GENIUS_REST_SECS, Ordering::Relaxed);
        }
        found
    }

    /// What LRCLIB has for a track, from ever looser queries.
    async fn query_lrclib(
        &self,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
    ) -> LrclibAnswer {
        let mut chain = Chain::default();
        // The get endpoint with full parameters
        let reply = self
            .query_get(track_name, artist_name, album_name, duration_secs)
            .await;
        if chain.take(reply) {
            return chain.answer();
        }

        // Then with just track_name and artist_name
        if album_name.is_some() || duration_secs.is_some() {
            let reply = self.query_get(track_name, artist_name, None, None).await;
            if chain.take(reply) {
                return chain.answer();
            }
        }

        // Then the search endpoint, preferring the right duration
        let reply = self
            .query_search(track_name, artist_name, duration_secs)
            .await;
        if chain.take(reply) {
            return chain.answer();
        }

        // Then the clean track title (strip " - Remastered", " (Live)", etc.)
        if let Some(clean_title) = clean_track_title(track_name) {
            let reply = self
                .query_get(&clean_title, artist_name, None, duration_secs)
                .await;
            if !chain.take(reply) {
                let reply = self
                    .query_search(&clean_title, artist_name, duration_secs)
                    .await;
                chain.take(reply);
            }
        }
        chain.answer()
    }

    async fn query_get(
        &self,
        track_name: &str,
        artist_name: &str,
        album_name: Option<&str>,
        duration_secs: Option<u32>,
    ) -> Reply {
        let mut query: Vec<(&str, String)> = vec![
            ("track_name", track_name.to_string()),
            ("artist_name", artist_name.to_string()),
        ];
        if let Some(album) = album_name {
            if !album.is_empty() {
                query.push(("album_name", album.to_string()));
            }
        }
        if let Some(duration) = duration_secs {
            if duration > 0 {
                query.push(("duration", duration.to_string()));
            }
        }

        let resp = match self
            .client
            .get(&self.endpoints.lrclib_get)
            .query(&query)
            .timeout(LRCLIB_TIMEOUT)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return no_answer(e),
        };
        let status = resp.status();
        if status == reqwest::StatusCode::NOT_FOUND {
            return Reply::None;
        }
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Reply::Unreachable;
        }
        if !status.is_success() {
            return Reply::Error;
        }

        let Ok(item) = resp.json::<LrclibItem>().await else {
            return Reply::Error;
        };
        item.lyrics().map_or(Reply::None, Reply::Lyrics)
    }

    async fn query_search(
        &self,
        track_name: &str,
        artist_name: &str,
        duration_secs: Option<u32>,
    ) -> Reply {
        let query = [("track_name", track_name), ("artist_name", artist_name)];

        let resp = match self
            .client
            .get(&self.endpoints.lrclib_search)
            .query(&query)
            .timeout(LRCLIB_TIMEOUT)
            .send()
            .await
        {
            Ok(resp) => resp,
            Err(e) => return no_answer(e),
        };
        let status = resp.status();
        if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Reply::Unreachable;
        }
        if !status.is_success() {
            return Reply::Error;
        }

        let Ok(mut items) = resp.json::<Vec<LrclibItem>>().await else {
            return Reply::Error;
        };
        // Filter out non-lyric instrumental items
        items.retain(|item| item.instrumental != Some(true));

        // If target duration is known, sort items by proximity to target duration
        if let Some(target) = duration_secs {
            let target_f = target as f64;
            items.sort_by(|a, b| {
                let diff_a = a.duration.map_or(f64::MAX, |d| (d - target_f).abs());
                let diff_b = b.duration.map_or(f64::MAX, |d| (d - target_f).abs());
                diff_a
                    .partial_cmp(&diff_b)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }

        // Prefer word timings for the right length, then any synced lyrics
        let fits = |item: &LrclibItem| match (duration_secs, item.duration) {
            (Some(target), Some(d)) => (d - f64::from(target)).abs() <= 2.0,
            _ => true,
        };
        if let Some(lyrics) = items
            .iter()
            .filter(|item| item.has_words() && fits(item))
            .find_map(LrclibItem::lyrics)
        {
            return Reply::Lyrics(lyrics);
        }
        for item in &items {
            if item.synced_lyrics.is_some() {
                if let Some(lyrics @ Lyrics::Synced(_)) = item.lyrics() {
                    return Reply::Lyrics(lyrics);
                }
            }
        }
        for item in &items {
            if let Some(lyrics) = item.lyrics() {
                return Reply::Lyrics(lyrics);
            }
        }

        Reply::None
    }
}

/// A request LRCLIB never answered (offline, timed out).
fn no_answer(e: reqwest::Error) -> Reply {
    debug!("LRCLIB didn't answer: {}", e.without_url());
    Reply::Unreachable
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |since| since.as_secs())
}

/// The saved lyrics that answer without asking anyone: LRCLIB's, then
/// Genius's while fresh (with the fallback on).
fn saved_lyrics(cache: &LyricsCache, key: &str, genius_on: bool, now: u64) -> Option<FoundLyrics> {
    if let Some(lyrics) = cache.get(key) {
        return Some(FoundLyrics::lrclib(lyrics));
    }
    if !genius_on {
        return None;
    }
    cache
        .get_genius(key)
        .filter(|saved| saved.is_fresh(now))
        .map(GeniusLyricsEntry::into_found)
}

/// The lyrics to show, LRCLIB first: its copy on disk; Genius's while
/// fresh (so a song LRCLIB lacks doesn't ask LRCLIB at every play);
/// nothing for a track both lacked lately; LRCLIB on the web; Genius on the
/// web; Genius's old copy when Genius can't be reached; LRCLIB's copy from
/// before word timings. A reload (`force_refresh`) skips the first three.
/// When LRCLIB can't say whether it has them, its own copies come before
/// Genius, and Genius's lyrics aren't kept as fresh (LRCLIB is asked again
/// next time). With `genius_on` false nothing of Genius is used. The
/// sources are futures that only run (and reach the network) when their
/// turn comes.
#[allow(clippy::too_many_arguments)]
async fn resolve(
    cache: &LyricsCache,
    misses: &Misses,
    key: &str,
    genius_on: bool,
    force_refresh: bool,
    now: u64,
    lrclib: impl Future<Output = LrclibAnswer>,
    genius: impl Future<Output = Result<Option<GeniusLyrics>, DomainError>>,
) -> Option<FoundLyrics> {
    if !force_refresh {
        if let Some(saved) = saved_lyrics(cache, key, genius_on, now) {
            return Some(saved);
        }
        if misses.remembers(key, genius_on, now) {
            return None;
        }
    }
    let lrclib_failed = match lrclib.await {
        LrclibAnswer::Found(lyrics) => {
            let _ = cache.store(key, &lyrics);
            misses.forget(key);
            return Some(FoundLyrics::lrclib(lyrics));
        }
        LrclibAnswer::NotFound => false,
        LrclibAnswer::Failed => true,
    };
    if lrclib_failed {
        // LRCLIB may well have them: its lyrics on disk before Genius's (on
        // a reload, also the copy it just skipped)
        let saved = force_refresh.then(|| cache.get(key)).flatten();
        if let Some(lyrics) = saved.or_else(|| cache.get_previous(key)) {
            return Some(FoundLyrics::lrclib(lyrics));
        }
    }
    // Whether Genius said cleanly it has none (or wasn't to be asked)
    let mut genius_none = !genius_on;
    if genius_on {
        match genius.await {
            Ok(Some(found)) => {
                let entry = GeniusLyricsEntry {
                    lines: found.lines,
                    url: found.url,
                    // Kept for when nothing answers, but never fresh
                    fetched_at: if lrclib_failed { 0 } else { now },
                };
                let _ = cache.store_genius(key, &entry);
                misses.forget(key);
                return Some(entry.into_found());
            }
            Ok(None) => {
                debug!("Genius has no lyrics for this track either");
                genius_none = true;
            }
            Err(e) => {
                debug!("Genius lyrics failed: {e}");
                if let Some(saved) = cache.get_genius(key) {
                    return Some(saved.into_found());
                }
            }
        }
    }
    if lrclib_failed {
        return None;
    }
    // Nothing online: lyrics saved before word timings are still lyrics
    if let Some(lyrics) = cache.get_previous(key) {
        return Some(FoundLyrics::lrclib(lyrics));
    }
    if genius_none {
        // Both answered they have none
        misses.remember(key, genius_on, now);
    }
    None
}

#[cfg(test)]
mod order_tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::lyrics::model::{LyricLine, LyricsSource};

    const KEY: &str = "spotify:track:abc";
    const NOW: u64 = 1_800_000_000;
    const DAY: u64 = 24 * 60 * 60;

    fn temp_cache(name: &str) -> (LyricsCache, PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("canto_test_order_{name}_{nanos}"));
        (LyricsCache::with_dir(dir.clone()), dir)
    }

    fn lrclib_lyrics() -> Lyrics {
        Lyrics::Synced(vec![LyricLine::new(1000, "from LRCLIB")])
    }

    fn saved_genius(age_days: u64) -> GeniusLyricsEntry {
        GeniusLyricsEntry {
            lines: vec!["saved from Genius".into()],
            url: "https://genius.com/saved".into(),
            fetched_at: NOW - age_days * DAY,
        }
    }

    fn new_genius() -> GeniusLyrics {
        GeniusLyrics {
            song_id: 1,
            url: "https://genius.com/new".into(),
            lines: vec!["new from Genius".into(), String::new(), "end".into()],
        }
    }

    fn genius_url(found: &Option<FoundLyrics>) -> Option<&str> {
        found.as_ref()?.source.genius_url()
    }

    /// Sources that fail the test if they are asked.
    async fn no_lrclib() -> LrclibAnswer {
        panic!("LRCLIB must not be asked")
    }

    async fn no_genius() -> Result<Option<GeniusLyrics>, DomainError> {
        panic!("Genius must not be asked")
    }

    fn offline() -> Result<Option<GeniusLyrics>, DomainError> {
        Err(DomainError::Network("offline".into()))
    }

    /// `resolve` with nothing remembered.
    #[allow(clippy::too_many_arguments)]
    async fn resolve_fresh(
        cache: &LyricsCache,
        key: &str,
        genius_on: bool,
        force_refresh: bool,
        now: u64,
        lrclib: impl Future<Output = LrclibAnswer>,
        genius: impl Future<Output = Result<Option<GeniusLyrics>, DomainError>>,
    ) -> Option<FoundLyrics> {
        let misses = Misses::default();
        let found = resolve(
            cache,
            &misses,
            key,
            genius_on,
            force_refresh,
            now,
            lrclib,
            genius,
        )
        .await;
        found
    }

    #[tokio::test]
    async fn saved_lrclib_lyrics_win_over_saved_genius_lyrics() {
        let (cache, dir) = temp_cache("lrclib_wins");
        cache.store(KEY, &lrclib_lyrics()).unwrap();
        cache.store_genius(KEY, &saved_genius(1)).unwrap();
        let found = resolve_fresh(&cache, KEY, true, false, NOW, no_lrclib(), no_genius()).await;
        assert_eq!(found, Some(FoundLyrics::lrclib(lrclib_lyrics())));
        assert_eq!(
            saved_lyrics(&cache, KEY, true, NOW),
            Some(FoundLyrics::lrclib(lrclib_lyrics()))
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn fresh_genius_lyrics_spare_lrclib_the_trip() {
        let (cache, dir) = temp_cache("genius_fresh");
        cache.store_genius(KEY, &saved_genius(6)).unwrap();
        let found = resolve_fresh(&cache, KEY, true, false, NOW, no_lrclib(), no_genius()).await;
        assert_eq!(genius_url(&found), Some("https://genius.com/saved"));
        assert_eq!(
            found.unwrap().lyrics,
            Lyrics::Plain(vec!["saved from Genius".into()])
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn with_the_fallback_off_genius_is_never_used() {
        let (cache, dir) = temp_cache("genius_off");
        cache.store_genius(KEY, &saved_genius(1)).unwrap();
        assert_eq!(saved_lyrics(&cache, KEY, false, NOW), None);
        let not_found = async { LrclibAnswer::NotFound };
        let found = resolve_fresh(&cache, KEY, false, false, NOW, not_found, no_genius()).await;
        assert_eq!(found, None);
        // Not even its copy when LRCLIB can't be reached on a reload
        let failed = async { LrclibAnswer::Failed };
        let found = resolve_fresh(&cache, KEY, false, true, NOW, failed, no_genius()).await;
        assert_eq!(found, None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lyrics_from_lrclib_are_saved_as_before() {
        let (cache, dir) = temp_cache("lrclib_web");
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::Found(lrclib_lyrics()) },
            no_genius(),
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(lrclib_lyrics())));
        assert_eq!(cache.get(KEY), Some(lrclib_lyrics()));
        assert!(dir.join("abc.v2.json").exists());
        assert_eq!(cache.get_genius(KEY), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn genius_answers_when_lrclib_has_none_and_is_saved_apart() {
        let (cache, dir) = temp_cache("genius_web");
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::NotFound },
            async { Ok(Some(new_genius())) },
        )
        .await
        .unwrap();
        assert_eq!(
            found.source,
            LyricsSource::Genius {
                url: "https://genius.com/new".into()
            }
        );
        assert_eq!(
            found.lyrics,
            Lyrics::Plain(vec!["new from Genius".into(), String::new(), "end".into()])
        );
        let saved = cache.get_genius(KEY).unwrap();
        assert_eq!(saved.fetched_at, NOW);
        assert_eq!(saved.url, "https://genius.com/new");
        // LRCLIB's file stays untouched
        assert_eq!(cache.get(KEY), None);
        assert!(!dir.join("abc.v2.json").exists());
        // The next play shows it at once
        assert_eq!(saved_lyrics(&cache, KEY, true, NOW + DAY), Some(found));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn old_genius_lyrics_only_when_genius_cannot_be_reached() {
        let (cache, dir) = temp_cache("genius_stale");
        cache.store_genius(KEY, &saved_genius(8)).unwrap();
        // Stale: LRCLIB is asked again first, and the copy isn't shown at once
        assert_eq!(saved_lyrics(&cache, KEY, true, NOW), None);
        let not_found = || async { LrclibAnswer::NotFound };
        let found = resolve_fresh(&cache, KEY, true, false, NOW, not_found(), async {
            offline()
        })
        .await;
        assert_eq!(genius_url(&found), Some("https://genius.com/saved"));
        // Genius answered it has none: the old copy is not shown
        let found = resolve_fresh(&cache, KEY, true, false, NOW, not_found(), async {
            Ok(None)
        })
        .await;
        assert_eq!(found, None);
        // Genius answered with lyrics: they replace the old copy
        let found = resolve_fresh(&cache, KEY, true, false, NOW, not_found(), async {
            Ok(Some(new_genius()))
        })
        .await;
        assert_eq!(genius_url(&found), Some("https://genius.com/new"));
        assert_eq!(cache.get_genius(KEY).unwrap().fetched_at, NOW);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_reload_asks_again_and_falls_back_on_the_saved_genius_copy() {
        let (cache, dir) = temp_cache("reload");
        cache.store(KEY, &lrclib_lyrics()).unwrap();
        cache.store_genius(KEY, &saved_genius(1)).unwrap();
        let other = Lyrics::Plain(vec!["fixed on LRCLIB".into()]);
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            true,
            NOW,
            async { LrclibAnswer::Found(other.clone()) },
            no_genius(),
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(other.clone())));
        assert_eq!(cache.get(KEY), Some(other));
        // Nothing reachable: Genius's copy, whatever its age
        let (cache, dir2) = temp_cache("reload_offline");
        cache.store_genius(KEY, &saved_genius(30)).unwrap();
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            true,
            NOW,
            async { LrclibAnswer::Failed },
            async { offline() },
        )
        .await;
        assert_eq!(genius_url(&found), Some("https://genius.com/saved"));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir2);
    }

    #[tokio::test]
    async fn a_reload_that_cannot_reach_lrclib_keeps_its_lyrics() {
        let (cache, dir) = temp_cache("reload_keeps_lrclib");
        cache.store(KEY, &lrclib_lyrics()).unwrap();
        cache.store_genius(KEY, &saved_genius(1)).unwrap();
        // Both down: LRCLIB's copy, not Genius's
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            true,
            NOW,
            async { LrclibAnswer::Failed },
            async { offline() },
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(lrclib_lyrics())));
        // Genius up: still LRCLIB's copy, without asking Genius
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            true,
            NOW,
            async { LrclibAnswer::Failed },
            no_genius(),
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(lrclib_lyrics())));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn genius_after_an_lrclib_failure_is_shown_but_not_kept_fresh() {
        let (cache, dir) = temp_cache("lrclib_failed");
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::Failed },
            async { Ok(Some(new_genius())) },
        )
        .await;
        assert_eq!(genius_url(&found), Some("https://genius.com/new"));
        // Saved for when nothing answers, but LRCLIB is asked again next time
        let saved = cache.get_genius(KEY).unwrap();
        assert!(!saved.is_fresh(NOW));
        assert_eq!(saved_lyrics(&cache, KEY, true, NOW), None);
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::Found(lrclib_lyrics()) },
            no_genius(),
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(lrclib_lyrics())));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lyrics_saved_before_word_timings_come_last() {
        let (cache, dir) = temp_cache("previous");
        fs::create_dir_all(&dir).unwrap();
        let old = Lyrics::Plain(vec!["old".into()]);
        fs::write(dir.join("abc.json"), serde_json::to_string(&old).unwrap()).unwrap();
        let found = resolve_fresh(
            &cache,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::NotFound },
            async { offline() },
        )
        .await;
        assert_eq!(found, Some(FoundLyrics::lrclib(old.clone())));
        let not_found = async { LrclibAnswer::NotFound };
        let found = resolve_fresh(&cache, KEY, false, false, NOW, not_found, no_genius()).await;
        assert_eq!(found, Some(FoundLyrics::lrclib(old.clone())));
        // LRCLIB unreachable: they come before Genius, which isn't asked
        let failed = async { LrclibAnswer::Failed };
        let found = resolve_fresh(&cache, KEY, true, false, NOW, failed, no_genius()).await;
        assert_eq!(found, Some(FoundLyrics::lrclib(old)));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_track_neither_has_is_not_asked_about_for_a_day() {
        let (cache, dir) = temp_cache("misses");
        let misses = Misses::default();
        let none = || async { LrclibAnswer::NotFound };
        let found = resolve(&cache, &misses, KEY, true, false, NOW, none(), async {
            Ok(None)
        })
        .await;
        assert_eq!(found, None);
        // Played again: nobody is asked, with the fallback on or off
        for genius_on in [true, false] {
            let found = resolve(
                &cache,
                &misses,
                "abc",
                genius_on,
                false,
                NOW + 60,
                no_lrclib(),
                no_genius(),
            )
            .await;
            assert_eq!(found, None);
        }
        // A reload asks again, and so does any play a day later
        let found = resolve(&cache, &misses, KEY, true, true, NOW, none(), async {
            Ok(Some(new_genius()))
        })
        .await;
        assert_eq!(genius_url(&found), Some("https://genius.com/new"));
        assert!(!misses.remembers(KEY, true, NOW));
        misses.remember(KEY, true, NOW);
        assert!(!misses.remembers(KEY, true, NOW + DAY));
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn only_clean_answers_are_remembered_as_misses() {
        let (cache, dir) = temp_cache("misses_clean");
        let misses = Misses::default();
        // LRCLIB alone said none: turning the fallback on still asks Genius
        let found = resolve(
            &cache,
            &misses,
            KEY,
            false,
            false,
            NOW,
            async { LrclibAnswer::NotFound },
            no_genius(),
        )
        .await;
        assert_eq!(found, None);
        assert!(misses.remembers(KEY, false, NOW));
        assert!(!misses.remembers(KEY, true, NOW));
        misses.forget(KEY);
        // Either one failing: asked again next time
        let found = resolve(
            &cache,
            &misses,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::NotFound },
            async { offline() },
        )
        .await;
        assert_eq!(found, None);
        let found = resolve(
            &cache,
            &misses,
            KEY,
            true,
            false,
            NOW,
            async { LrclibAnswer::Failed },
            async { Ok(None) },
        )
        .await;
        assert_eq!(found, None);
        assert!(!misses.remembers(KEY, false, NOW));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn lrclib_has_none_only_when_every_query_says_so() {
        let lyrics = || Reply::Lyrics(lrclib_lyrics());
        let follow = |replies: Vec<Reply>| {
            let mut chain = Chain::default();
            let mut asked = 0;
            for reply in replies {
                asked += 1;
                if chain.take(reply) {
                    break;
                }
            }
            (chain.answer(), asked)
        };
        assert_eq!(
            follow(vec![Reply::None, Reply::None, Reply::None]),
            (LrclibAnswer::NotFound, 3)
        );
        assert_eq!(
            follow(vec![Reply::None, Reply::Error, Reply::None]),
            (LrclibAnswer::Failed, 3)
        );
        assert_eq!(
            follow(vec![Reply::Error, lyrics(), Reply::None]),
            (LrclibAnswer::Found(lrclib_lyrics()), 2)
        );
        // No answer at all: the other queries aren't tried
        assert_eq!(
            follow(vec![Reply::None, Reply::Unreachable, Reply::None]),
            (LrclibAnswer::Failed, 2)
        );
    }
}

/// The look-up against test servers standing in for LRCLIB and Genius.
#[cfg(test)]
mod network_tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;
    use crate::test_server::{closed_base, TestServer};

    const KEY: &str = "spotify:track:net";
    const NOT_FOUND: &str =
        r#"{"code":404,"name":"TrackNotFound","message":"Failed to find specified track"}"#;
    const SYNCED: &str = r#"{"trackName":"Lua de Papel","artistName":"Nora Vale","duration":180.0,"instrumental":false,"plainLyrics":"Linha um","syncedLyrics":"[00:01.00] Linha um\n"}"#;
    const EMBED: &str = r#"document.write(JSON.parse('\"<div class=\\\"rg_embed_body\\\">\\n  <p>[Intro]<br>\\nPaper boats on a copper tide<\/p>\\n<\/div>\\n<div class=\\\"rg_embed_footer\\\"><\/div>\"'))"#;

    fn temp_cache(name: &str) -> (LyricsCache, PathBuf) {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("canto_test_net_{name}_{nanos}"));
        (LyricsCache::with_dir(dir.clone()), dir)
    }

    fn genius_search() -> String {
        serde_json::json!({ "response": { "sections": [{ "type": "song", "hits": [{
            "type": "song",
            "result": {
                "id": 7,
                "title": "Lua de Papel",
                "artist_names": "Nora Vale",
                "primary_artist": { "name": "Nora Vale" },
                "url": "https://genius.com/Nora-vale-lua-de-papel-lyrics",
                "lyrics_state": "complete",
                "instrumental": false
            }
        }] }] } })
        .to_string()
    }

    fn genius_up() -> TestServer {
        let search = genius_search();
        TestServer::start(&[
            ("/api/search/song", 200, search.as_str()),
            ("/songs/7/embed.js", 200, EMBED),
        ])
    }

    async fn look_up(service: &LyricsService) -> Option<FoundLyrics> {
        service
            .fetch_lyrics(
                KEY,
                "Lua de Papel",
                "Nora Vale",
                Some("Papel"),
                Some(180),
                Some(""),
            )
            .await
    }

    #[tokio::test]
    async fn lrclib_answering_none_hands_over_to_genius_for_a_week() {
        let (cache, dir) = temp_cache("handover");
        let lrclib = TestServer::start(&[("/api/get", 404, NOT_FOUND), ("/api/search", 200, "[]")]);
        let genius = genius_up();
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        let found = look_up(&service).await.unwrap();
        assert_eq!(
            found.source.genius_url(),
            Some("https://genius.com/Nora-vale-lua-de-papel-lyrics")
        );
        assert_eq!(
            found.lyrics,
            Lyrics::Plain(vec!["Paper boats on a copper tide".into()])
        );
        // get with album and duration, get without, search
        assert_eq!(lrclib.requests("/api/").len(), 3);
        assert_eq!(service.cached_lyrics(KEY, true), Some(found.clone()));
        // Played again: from disk, nobody asked
        assert_eq!(look_up(&service).await, Some(found));
        assert_eq!(lrclib.requests("/api/").len(), 3);
        assert_eq!(genius.requests("/").len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lrclib_in_trouble_is_asked_again_next_time() {
        let (cache, dir) = temp_cache("trouble");
        let lrclib =
            TestServer::start(&[("/api/get", 502, "Bad Gateway"), ("/api/search", 200, "[]")]);
        let genius = genius_up();
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        let found = look_up(&service).await.unwrap();
        assert!(found.source.genius_url().is_some());
        // An error answer doesn't stop the other queries
        assert_eq!(lrclib.requests("/api/").len(), 3);
        // Shown, but not kept as fresh: LRCLIB is asked again
        assert_eq!(service.cached_lyrics(KEY, true), None);
        assert_eq!(look_up(&service).await, Some(found));
        assert_eq!(lrclib.requests("/api/").len(), 6);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lrclib_out_of_reach_is_asked_once_then_genius() {
        let (cache, dir) = temp_cache("unreachable");
        let genius = genius_up();
        let service = LyricsService::for_tests(cache, &closed_base(), &genius.base);
        let found = look_up(&service).await.unwrap();
        assert!(found.source.genius_url().is_some());
        assert_eq!(service.cached_lyrics(KEY, true), None);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn lrclib_lyrics_are_found_and_kept() {
        let (cache, dir) = temp_cache("found");
        let lrclib = TestServer::start(&[("/api/get", 200, SYNCED)]);
        let genius = TestServer::start(&[]);
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        let found = look_up(&service).await.unwrap();
        assert_eq!(found.source, crate::lyrics::model::LyricsSource::Lrclib);
        assert!(found.lyrics.is_synced());
        assert_eq!(lrclib.requests("/api/").len(), 1);
        assert!(genius.requests("/").is_empty());
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn the_page_and_the_card_asking_together_ask_once() {
        let (cache, dir) = temp_cache("together");
        let lrclib = TestServer::start(&[("/api/get", 404, NOT_FOUND), ("/api/search", 200, "[]")]);
        let genius = genius_up();
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        let (page, card) = futures::join!(look_up(&service), look_up(&service));
        assert!(page.is_some());
        assert_eq!(page, card);
        assert_eq!(lrclib.requests("/api/").len(), 3);
        assert_eq!(genius.requests("/").len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn a_track_neither_has_is_asked_about_once() {
        let (cache, dir) = temp_cache("neither");
        let lrclib = TestServer::start(&[("/api/get", 404, NOT_FOUND), ("/api/search", 200, "[]")]);
        let empty = r#"{"response":{"sections":[{"type":"song","hits":[]}]}}"#;
        let genius = TestServer::start(&[("/api/search/song", 200, empty)]);
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        assert_eq!(look_up(&service).await, None);
        // "artist title", then "title artist"
        assert_eq!(genius.requests("/").len(), 2);
        assert_eq!(look_up(&service).await, None);
        assert_eq!(lrclib.requests("/api/").len(), 3);
        assert_eq!(genius.requests("/").len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }

    #[tokio::test]
    async fn genius_rests_after_refusing_unless_the_user_reloads() {
        let (cache, dir) = temp_cache("rest");
        let lrclib = TestServer::start(&[("/api/get", 404, NOT_FOUND), ("/api/search", 200, "[]")]);
        let genius = TestServer::start(&[("/api/search/song", 429, "{}")]);
        let service = LyricsService::for_tests(cache, &lrclib.base, &genius.base);
        assert_eq!(look_up(&service).await, None);
        assert_eq!(genius.requests("/").len(), 1);
        // Not a clean answer: LRCLIB is asked again, Genius rests
        assert_eq!(look_up(&service).await, None);
        assert_eq!(lrclib.requests("/api/").len(), 6);
        assert_eq!(genius.requests("/").len(), 1);
        let reloaded = service
            .refetch_lyrics(
                KEY,
                "Lua de Papel",
                "Nora Vale",
                Some("Papel"),
                Some(180),
                Some(""),
            )
            .await;
        assert_eq!(reloaded, None);
        assert_eq!(genius.requests("/").len(), 2);
        let _ = fs::remove_dir_all(&dir);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_item_with_word_timings_lights_real_words() {
        let json = r#"{"trackName":"Blinding Lights","duration":200.0,"instrumental":false,
            "plainLyrics":"Yeah","syncedLyrics":"[00:13.12] Yeah\n[00:20.00] ♪\n",
            "hasWordSync":true,
            "lyricsfile":"lines:\n- text: Yeah\n  start_ms: 13120\n  end_ms: 14551\n  words:\n  - text: Yeah\n    start_ms: 13159\n"}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        let Some(Lyrics::Synced(lines)) = item.lyrics() else {
            panic!("synced lyrics expected");
        };
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].words.len(), 1);
        assert_eq!(lines[0].end_time_ms, Some(14551));
        assert_eq!(lines[1].text, "♪");
    }

    #[test]
    fn an_old_style_item_still_reads() {
        let json = r#"{"plainLyrics":"a","syncedLyrics":"[00:01.00] a\n","duration":10}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        assert!(matches!(item.lyrics(), Some(Lyrics::Synced(l)) if l[0].words.is_empty()));
        // Word sync promised but the file is unreadable: the LRC lines stay
        let json = r#"{"syncedLyrics":"[00:01.00] a\n","hasWordSync":true,"lyricsfile":"lines:\n- text: |\n"}"#;
        let item: LrclibItem = serde_json::from_str(json).unwrap();
        assert!(matches!(item.lyrics(), Some(Lyrics::Synced(l)) if l[0].text == "a"));
    }

    #[test]
    fn clean_track_title_handles_remasters_features_and_live_editions() {
        assert_eq!(
            clean_track_title("Bohemian Rhapsody - Remastered 2011").as_deref(),
            Some("Bohemian Rhapsody")
        );
        assert_eq!(
            clean_track_title("Yesterday - 2009 Remaster").as_deref(),
            Some("Yesterday")
        );
        assert_eq!(
            clean_track_title("Wish You Were Here (2011 Remaster)").as_deref(),
            Some("Wish You Were Here")
        );
        assert_eq!(
            clean_track_title("Evidências (Ao Vivo) [Deluxe]").as_deref(),
            Some("Evidências")
        );
        assert_eq!(
            clean_track_title("Stay (with Justin Bieber)").as_deref(),
            Some("Stay")
        );
        assert_eq!(
            clean_track_title("Track One - radio edit (feat. Guest)").as_deref(),
            Some("Track One")
        );
        assert_eq!(clean_track_title("Let It Be"), None);
    }
}
