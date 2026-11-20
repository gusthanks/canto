//! Lyrics from Genius, for the songs LRCLIB has none for.
//!
//! The song is found by search, strictly (wrong lyrics are worse than none),
//! and its text is read from the embed Genius offers websites
//! (`/songs/{id}/embed.js`), never by scraping the song page. With the
//! user's token the official API searches; without one, the search
//! genius.com itself uses. Never logs tokens.

use std::time::Duration;

use log::debug;
use serde_json::Value;

use super::{
    decode_entities, get_genius_json, get_json, normalize_genius_query_title, primary_artist_name,
    GENIUS_API_BASE, GENIUS_USER_AGENT,
};
use crate::error::DomainError;

/// The search genius.com uses itself: no token needed.
const PUBLIC_SEARCH_URL: &str = "https://genius.com/api/search/song";
const SONGS_URL: &str = "https://genius.com/songs";
const TIMEOUT: Duration = Duration::from_secs(12);
/// The biggest embed seen is about 53 KB; far past that it isn't lyrics.
const MAX_EMBED_BYTES: usize = 2 * 1024 * 1024;

/// Words naming another version of a song, whose lyrics differ or are
/// missing: a hit with one is taken only when the track has it too.
const VARIANTS: &[&str] = &[
    "remix",
    "mix",
    "live",
    "ao vivo",
    "en vivo",
    "demo",
    "instrumental",
    "karaoke",
    "cover",
    "sped up",
    "slowed",
    "acapella",
    "a cappella",
    "acoustic",
    "acustico",
    "unplugged",
    "stripped",
    "piano",
    "orchestral",
    "extended",
    "edit",
    "version",
    "versao",
    "dub",
    "vip",
];
/// Words naming other content than the song itself: a piece of an album
/// ("Intro", "Skit") or a new take on it ("Reprise", "Freestyle"). A title
/// with one never matches a title without it, either way.
const SECTIONS: &[&str] = &[
    "intro",
    "outro",
    "interlude",
    "interludio",
    "prelude",
    "preludio",
    "skit",
    "reprise",
    "freestyle",
    "snippet",
];
/// Words before the number of a part ("Pt. 2", "Parte II", "Vol. 53"):
/// another part is another song.
const PART_WORDS: &[&str] = &[
    "pt", "part", "parte", "chapter", "capitulo", "vol", "volume",
];
/// What translation and romanization pages say in the parentheses closing
/// their title, accents folded ("Tradução em Português").
const TRANSLATION_WORDS: &[&str] = &[
    "translation",
    "tradu",
    "ubersetzung",
    "ceviri",
    "перевод",
    "romaniz",
    "tlumacz",
];
/// Notices Genius may show where lyrics would be.
const PLACEHOLDERS: &[&str] = &[
    "yet to be released",
    "yet to be transcribed",
    "please check back",
    "instrumental",
    "no lyrics",
    "sem letra",
    "letra não dispon",
    "letra nao dispon",
];

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

/// The songs in a search answer, in Genius's order: `response.hits[]` from
/// the official API, `response.sections[].hits[]` from the public search.
pub fn search_hits(value: &Value) -> Vec<GeniusSongHit> {
    let Some(response) = value.get("response") else {
        return Vec::new();
    };
    let sections = response
        .get("sections")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|section| section.get("hits").and_then(Value::as_array));
    let lists = response
        .get("hits")
        .and_then(Value::as_array)
        .into_iter()
        .chain(sections);
    let mut songs: Vec<GeniusSongHit> = Vec::new();
    for hit in lists.flatten() {
        let kind = hit.get("type").and_then(Value::as_str);
        if kind.is_some_and(|kind| kind != "song") {
            continue;
        }
        if let Some(song) = hit.get("result").and_then(song_hit) {
            if songs.iter().all(|seen| seen.id != song.id) {
                songs.push(song);
            }
        }
    }
    songs
}

fn song_hit(result: &Value) -> Option<GeniusSongHit> {
    let text = |value: Option<&Value>| {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let id = result.get("id")?.as_u64()?;
    let title = text(result.get("title"));
    if title.is_empty() {
        return None;
    }
    let primary_artist = text(result.pointer("/primary_artist/name"));
    let mut artist_names = text(result.get("artist_names"));
    if artist_names.is_empty() {
        artist_names = primary_artist.clone();
    }
    // The official search may leave these out: complete, not instrumental
    let lyrics_state = result
        .get("lyrics_state")
        .and_then(Value::as_str)
        .unwrap_or("complete")
        .to_string();
    Some(GeniusSongHit {
        id,
        title,
        primary_artist,
        artist_names,
        url: text(result.get("url")),
        lyrics_state,
        instrumental: result
            .get("instrumental")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    })
}

/// The hit whose lyrics are the track's, when one clearly is: its title and
/// one of its artists must match the track's (`artist`: the track's
/// artists, comma-separated, main one first), and translations, unreleased
/// songs, instrumentals and other versions or parts never count. The best
/// match wins; two different songs matching equally well are none (two
/// "Intro"s by one artist can't be told apart). Wrong lyrics are worse than
/// none.
pub fn pick_lyrics_hit<'a>(
    hits: &'a [GeniusSongHit],
    artist: &str,
    title: &str,
) -> Option<&'a GeniusSongHit> {
    let artists: Vec<String> = artist
        .split(',')
        .map(match_key)
        .filter(|a| !a.is_empty())
        .collect();
    let clean = match_key(&normalize_genius_query_title(title));
    if artists.is_empty() || clean.is_empty() || only_sections(&clean) {
        return None;
    }
    let raw = match_key(title);
    let mut names = title_names(title);
    names.push(clean.clone());
    let mut best: Option<(i32, &GeniusSongHit)> = None;
    let mut tied = false;
    for hit in hits {
        if hit.instrumental
            || hit.lyrics_state.eq_ignore_ascii_case("unreleased")
            || is_translation(hit)
        {
            continue;
        }
        let Some(by_title) = title_score(&hit.title, &names, &clean, &raw) else {
            continue;
        };
        let Some(by_artist) = artist_score(hit, &artists) else {
            continue;
        };
        // Someone else's song the artist is only credited on: only under
        // the very same title
        if by_artist <= ARTIST_CREDITED && by_title < TITLE_SAME {
            continue;
        }
        let score = by_title + by_artist;
        match best {
            Some((top, _)) if score < top => {}
            Some((top, _)) if score == top => tied = true,
            _ => {
                best = Some((score, hit));
                tied = false;
            }
        }
    }
    best.filter(|_| !tied).map(|(_, hit)| hit)
}

/// The title score of the same title, cleaned of "(feat. X)" or
/// " - Remastered" (as it came scores higher).
const TITLE_SAME: i32 = 95;
/// The artist score of a song the track's artist is only credited on.
const ARTIST_CREDITED: i32 = 20;

/// How surely a Genius title names the track (`names`: the track's names,
/// see [`title_names`]; `clean`: its title without "(feat. X)" or
/// " - Remastered"; `raw`: as it came): None when it doesn't.
fn title_score(hit_title: &str, names: &[String], clean: &str, raw: &str) -> Option<i32> {
    let hit = match_key(hit_title);
    if hit.is_empty() {
        return None;
    }
    // As it came beats as cleaned: a live track takes the live page over
    // the studio one
    if hit == raw {
        return Some(100);
    }
    if hit == clean {
        return Some(TITLE_SAME);
    }
    // Another version ("(Remix)", "(Live)"), other content ("(Reprise)",
    // "Intro") or another part than the track
    let has = |key: &str, word: &str| contains_words(key, word);
    if VARIANTS
        .iter()
        .chain(SECTIONS)
        .any(|v| has(&hit, v) && !has(raw, v))
        || SECTIONS.iter().any(|s| has(raw, s) && !has(&hit, s))
        || part_number(&hit) != part_number(raw)
    {
        return None;
    }
    // Another number is another song: "ALEXA 2", "Tema II"
    let numbers = numerals(&hit);
    if numbers != numerals(raw) && numbers != numerals(clean) {
        return None;
    }
    // A name of the one is a name of the other: "Song - From the Movie",
    // "夜に駆ける (Yoru ni Kakeru)", "봄날 (Spring Day)"
    if title_names(hit_title)
        .iter()
        .any(|name| names.contains(name))
    {
        return Some(90);
    }
    // One inside the other as whole words, at most twice as long and never
    // around a single word: "Vol. 53" takes "Shakira: Vol. 53", "Love"
    // never takes "Love Me Do", "Solo" never "Solo Dolo"
    let hit_words = hit.split(' ').count();
    let track_words = clean.split(' ').count();
    let (short, long) = if hit_words <= track_words {
        (hit.as_str(), clean)
    } else {
        (clean, hit.as_str())
    };
    let (fewer, more) = (hit_words.min(track_words), hit_words.max(track_words));
    (fewer >= 2 && contains_words(long, short) && fewer * 2 >= more)
        .then(|| 50 - (more - fewer) as i32)
}

/// Whether a title (a match key) is only "Intro", "Outro", "Skit"... with
/// maybe a number: one artist has one on many albums, so the name alone
/// never says which.
fn only_sections(key: &str) -> bool {
    let words: Vec<&str> = key.split(' ').filter(|w| !w.is_empty()).collect();
    words.iter().any(|w| SECTIONS.contains(w))
        && words.iter().all(|w| SECTIONS.contains(w) || is_numeral(w))
}

/// The numbers in a match key ("2", "ii"), sorted.
fn numerals(key: &str) -> Vec<&str> {
    let mut numbers: Vec<&str> = key.split(' ').filter(|w| is_numeral(w)).collect();
    numbers.sort_unstable();
    numbers
}

/// A number in digits, or in roman numerals from II on (a lone "I", "V"
/// or "X" is too often a word or a letter).
fn is_numeral(word: &str) -> bool {
    (!word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()))
        || matches!(word, "ii" | "iii" | "iv" | "vi" | "vii" | "viii" | "ix")
}

/// The names a title goes by, as match keys: all of it, the part before
/// " - ", and the parts before and inside a closing "(...)".
fn title_names(title: &str) -> Vec<String> {
    let mut names = vec![match_key(title)];
    if let Some((before, _)) = title.split_once(" - ") {
        names.push(match_key(before));
    }
    if let Some((name, alias)) = split_alias(title) {
        names.push(match_key(name));
        names.push(match_key(alias));
    }
    names.retain(|name| !name.is_empty());
    names
}

/// "Name (Alias)" as its two parts.
fn split_alias(title: &str) -> Option<(&str, &str)> {
    let inner = title.trim().strip_suffix(')')?;
    let open = inner.rfind('(')?;
    Some((inner[..open].trim(), inner[open + 1..].trim()))
}

/// The number after "Pt.", "Part", "Vol."... in a match key, if any.
fn part_number(key: &str) -> Option<&str> {
    let words: Vec<&str> = key.split(' ').collect();
    words
        .windows(2)
        .find(|pair| PART_WORDS.contains(&pair[0]) && is_number(pair[1]))
        .map(|pair| pair[1])
}

fn is_number(word: &str) -> bool {
    (!word.is_empty() && word.bytes().all(|b| b.is_ascii_digit()))
        || matches!(
            word,
            "i" | "ii" | "iii" | "iv" | "v" | "vi" | "vii" | "viii" | "ix" | "x"
        )
}

/// How surely the hit is by one of the track's artists (`artists`: match
/// keys, the main one first): None when it isn't.
fn artist_score(hit: &GeniusSongHit, artists: &[String]) -> Option<i32> {
    let main = artists.first()?;
    let primary = match_key(&hit.primary_artist);
    if !primary.is_empty() {
        if &primary == main {
            return Some(50);
        }
        if artists.contains(&primary) {
            return Some(40);
        }
        // "BTS" and "BTS (방탄소년단)", "Revelação" and "Grupo Revelação"
        if artists
            .iter()
            .any(|a| contains_words(&primary, a) || contains_words(a, &primary))
        {
            return Some(30);
        }
    }
    // Someone else's song with the track's main artist on it
    contains_words(&match_key(&hit.artist_names), main).then_some(ARTIST_CREDITED)
}

/// Pages of translations (and romanizations) by Genius's users, which look
/// like the song itself: "Nora Vale - Lua de Papel (Tradução em Português)"
/// by "Genius Brasil Traduções".
fn is_translation(hit: &GeniusSongHit) -> bool {
    let artist = match_key(&hit.primary_artist);
    if artist.split(' ').next() == Some("genius") || artist.rsplit(' ').next() == Some("genius") {
        return true;
    }
    split_alias(&hit.title).is_some_and(|(_, label)| {
        let label = match_key(label);
        TRANSLATION_WORDS.iter().any(|word| label.contains(word))
    })
}

/// Whether `words` appear in `text` as whole words, in order.
fn contains_words(text: &str, words: &str) -> bool {
    !words.is_empty() && format!(" {text} ").contains(&format!(" {words} "))
}

/// A name reduced for comparing: lowercase, common accents folded
/// ("ALGO LÚCIDO" = "algo lucido"), apostrophes dropped ("MC’s" = "MC's"),
/// "&" read as "and" ("Me & U" = "Me and U"), any other symbol a space.
fn match_key(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match c {
            '\'' | '\u{2018}' | '\u{2019}' | '\u{02bc}' | '`' | '\u{00b4}' => {}
            '&' => out.push_str(" and "),
            // Accents written as marks of their own
            '\u{0300}'..='\u{036f}' => {}
            c if c.is_alphanumeric() => match unaccent(c) {
                Some(plain) => out.push_str(plain),
                None => out.push(c),
            },
            _ => out.push(' '),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Common Latin letters with accents (lowercase) as plain letters.
fn unaccent(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ă' | 'ą' => "a",
        'æ' => "ae",
        'ç' | 'ć' | 'ĉ' | 'ċ' | 'č' => "c",
        'ď' | 'đ' | 'ð' => "d",
        'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ĕ' | 'ė' | 'ę' | 'ě' => "e",
        'ĝ' | 'ğ' | 'ġ' | 'ģ' => "g",
        'ĥ' | 'ħ' => "h",
        'ì' | 'í' | 'î' | 'ï' | 'ĩ' | 'ī' | 'ĭ' | 'į' | 'ı' => "i",
        'ĵ' => "j",
        'ķ' => "k",
        'ĺ' | 'ļ' | 'ľ' | 'ŀ' | 'ł' => "l",
        'ñ' | 'ń' | 'ņ' | 'ň' => "n",
        'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ō' | 'ŏ' | 'ő' => "o",
        'œ' => "oe",
        'ŕ' | 'ŗ' | 'ř' => "r",
        'ś' | 'ŝ' | 'ş' | 'š' | 'ș' => "s",
        'ß' => "ss",
        'ţ' | 'ť' | 'ŧ' | 'ț' => "t",
        'þ' => "th",
        'ù' | 'ú' | 'û' | 'ü' | 'ũ' | 'ū' | 'ŭ' | 'ů' | 'ű' | 'ų' => "u",
        'ŵ' => "w",
        'ý' | 'ÿ' | 'ŷ' => "y",
        'ź' | 'ż' | 'ž' => "z",
        _ => return None,
    })
}

/// The lyrics in an embed.js: `Ok(None)` when Genius has none (unreleased,
/// instrumental), an error when the file isn't the embed this knows.
///
/// The file is JavaScript: `document.write(JSON.parse('<literal>'))`, where
/// the literal is a JS string holding a JSON string holding the HTML.
pub fn parse_embed(js: &str) -> Result<Option<Vec<String>>, DomainError> {
    let unknown = |what: &str| DomainError::Parse(format!("Genius embed: {what}"));
    let literal = embed_literal(js).ok_or_else(|| unknown("no lyrics data"))?;
    let html: String =
        serde_json::from_str(&unescape_js(literal)).map_err(|e| unknown(&e.to_string()))?;
    let body = embed_body(&html).ok_or_else(|| unknown("no lyrics body"))?;
    Ok(clean_lyrics(body))
}

/// The JavaScript string inside `JSON.parse('...')`, still escaped.
fn embed_literal(js: &str) -> Option<&str> {
    const START: &str = "JSON.parse('";
    let start = js.find(START)? + START.len();
    let bytes = js.as_bytes();
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'\'' => return Some(&js[start..i]),
            _ => i += 1,
        }
    }
    None
}

/// Undoes the escapes of a JavaScript string literal (the text between its
/// quotes): `\n` and the other letters, `\xHH`, `\uXXXX` (surrogate pairs
/// joined) and `\u{...}`; any other `\X` is X (`\'`, `\"`, `\/`, `\$`).
pub fn unescape_js(literal: &str) -> String {
    let mut out = String::with_capacity(literal.len());
    let mut rest = literal;
    while let Some(at) = rest.find('\\') {
        out.push_str(&rest[..at]);
        rest = &rest[at + 1..];
        let Some(c) = rest.chars().next() else {
            break;
        };
        let mut used = c.len_utf8();
        match c {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '0' => out.push('\0'),
            'x' => match hex(&rest[1..], 2) {
                Some(code) => {
                    out.push(char::from(code as u8));
                    used = 3;
                }
                None => out.push('x'),
            },
            'u' => match unicode_escape(&rest[1..]) {
                Some((ch, len)) => {
                    out.push(ch);
                    used = 1 + len;
                }
                None => out.push('u'),
            },
            // A backslash before a line break only continues the line
            '\n' | '\u{2028}' | '\u{2029}' => {}
            '\r' => {
                if rest[1..].starts_with('\n') {
                    used = 2;
                }
            }
            other => out.push(other),
        }
        rest = &rest[used..];
    }
    out.push_str(rest);
    out
}

/// The value of the `len` hex digits `text` starts with.
fn hex(text: &str, len: usize) -> Option<u32> {
    let digits = text.get(..len)?;
    if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

/// The character of a `\u` escape (`text`: what follows the `u`) and how
/// many bytes of `text` it takes. A lone surrogate becomes U+FFFD.
fn unicode_escape(text: &str) -> Option<(char, usize)> {
    if let Some(braced) = text.strip_prefix('{') {
        let end = braced.find('}')?;
        let digits = &braced[..end];
        if digits.is_empty() || digits.len() > 6 || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let code = u32::from_str_radix(digits, 16).ok()?;
        return Some((char::from_u32(code).unwrap_or('\u{fffd}'), end + 2));
    }
    let high = hex(text, 4)?;
    if (0xd800..0xdc00).contains(&high) {
        let low = text[4..]
            .strip_prefix("\\u")
            .and_then(|next| hex(next, 4))
            .filter(|low| (0xdc00..0xe000).contains(low));
        if let Some(low) = low {
            let code = 0x10000 + ((high - 0xd800) << 10) + (low - 0xdc00);
            return Some((char::from_u32(code)?, 10));
        }
    }
    Some((char::from_u32(high).unwrap_or('\u{fffd}'), 4))
}

/// The inside of `<div class="rg_embed_body">`, which ends where the
/// footer (the song's title and artist, not lyrics) starts.
fn embed_body(html: &str) -> Option<&str> {
    const BODY: &str = r#"<div class="rg_embed_body">"#;
    const FOOTER: &str = r#"<div class="rg_embed_footer">"#;
    let start = html.find(BODY)? + BODY.len();
    let rest = &html[start..];
    let body = &rest[..rest.find(FOOTER)?];
    Some(match body.rfind("</div>") {
        Some(close) => &body[..close],
        None => body,
    })
}

/// The lines of the embed's body HTML: `<br>` ends a line, `</p>` a stanza,
/// other tags go (their text stays), section headers ("[Verse 1: Nora]")
/// become stanza breaks, blank lines never pile up. None when nothing is
/// left, or only a notice.
fn clean_lyrics(body: &str) -> Option<Vec<String>> {
    let text = decode_entities(&strip_tags(body));
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        // Zero-width spaces hide in front of headers and in empty links
        let line: String = line
            .chars()
            .filter(|c| !matches!(c, '\u{200b}' | '\u{feff}'))
            .collect();
        let line = line.trim();
        // "[Verse 1: Nora]", also with a colon after it ("[Refren]:")
        let header = line.starts_with('[') && line.trim_end_matches([':', '.']).ends_with(']');
        let line = if header { "" } else { line };
        if line.is_empty() && lines.last().is_none_or(|last| last.is_empty()) {
            continue;
        }
        lines.push(line.to_string());
    }
    while lines.last().is_some_and(|last| last.is_empty()) {
        lines.pop();
    }
    (!is_placeholder(&lines)).then_some(lines)
}

/// The text of HTML with line breaks where `<br>` and `</p>` were. Raw
/// line breaks in the HTML are only spaces; a `<` that starts no tag
/// ("I <3 you") stays.
fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut rest = html;
    while let Some(lt) = rest.find('<') {
        push_text(&mut out, &rest[..lt]);
        let after = &rest[lt + 1..];
        let starts_tag = after
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '!');
        let Some(end) = after.find('>').filter(|_| starts_tag) else {
            out.push('<');
            rest = after;
            continue;
        };
        let tag = &after[..end];
        let name = tag
            .trim_start_matches('/')
            .split(|c: char| !c.is_ascii_alphanumeric())
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        match name.as_str() {
            "br" => out.push('\n'),
            "p" if tag.starts_with('/') => out.push_str("\n\n"),
            _ => {}
        }
        rest = &after[end + 1..];
    }
    push_text(&mut out, rest);
    out
}

fn push_text(out: &mut String, text: &str) {
    for c in text.chars() {
        match c {
            '\n' | '\t' => out.push(' '),
            '\r' => {}
            c => out.push(c),
        }
    }
}

/// Whether the lines are only a notice ("Lyrics for this song have yet to
/// be released") or nothing. Only short texts count, so a song that sings
/// "instrumental" keeps its lyrics.
fn is_placeholder(lines: &[String]) -> bool {
    let filled: Vec<String> = lines
        .iter()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_lowercase())
        .collect();
    filled.is_empty()
        || (filled.len() <= 2
            && filled.iter().any(|line| {
                PLACEHOLDERS.iter().any(|notice| line.contains(notice))
                    || (line.contains("lyric") && line.contains("not available"))
            }))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Byte for byte as Genius encodes embeds (made by the research encoder
    /// that reproduces 105 real embeds); every lyric here is invented.
    const PT_BR_HEADER: &str = r#"if (document.getElementById('rg_embed_link_90000001')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000001\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_header\\\">\\n  <a target=\\\"_blank\\\" class=\\\"rg_embed_logo\\\" href=\\\"https://genius.com/Nora-vale-lua-de-papel-lyrics\\\">Powered by Genius<\/a>\\n<\/div>\\n\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>[Letra de \\\"Lua de Papel]<br>\\n<br>\\n[Intro]<br>\\nAcordei com o mundo de ponta-cabeça<br>\\nE avisou que o dia \'tava lento<br>\\n(Ei, ei, ei—)<br>\\n<br>\\n[Verso 1: Nora Vale]<br>\\nComprei um sonho em \\\"promoção\\\", sem troco<br>\\nDobrei a lua em papel e guardei no bolso<br>\\nD\'água, d\'ouro, d’ali: tudo cabe no meu som<br>\\nRock &amp; Roll de esquina, dia/noite sem horário<br>\\nNinguém sabe o que é ser de lá — nem eu<br>\\n<br>\\n[Refrão]<br>\\nLua de papel, lua de papel<br>\\nAmassa mas não rasga<br>\\n<br>\\n[Verso 2: Nora Vale &amp; Dani Rocha]<br>\\nCoração de lata, açúcar no café<br>\\nFé, fé, fé<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <a target=\\\"_blank\\\" href=\\\"https://genius.com/Nora-vale-lua-de-papel-lyrics\\\">\\n    <div class=\\\"song_title\\\">Lua de Papel<\/div>\\n    Nora Vale\\n<\/a><\/div>\\n\\n  <\/div>\\n<iframe src=\\\"//genius.com/songs/90000001/embed_analytics\\\"\\n              class=\\\"rg_embed_analytics\\\"\\n              id=\\\"rg_embed_analytics_90000001\\\"><\/iframe>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;
    const JS_ESCAPES: &str = r#"if (document.getElementById('rg_embed_link_90000014')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000014\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_header\\\">\\n  <a target=\\\"_blank\\\" class=\\\"rg_embed_logo\\\" href=\\\"https://genius.com/Nora-vale-dollar-sign-lyrics\\\">Powered by Genius<\/a>\\n<\/div>\\n\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>Ty Dolla \$ign said it\'s 5\$ a ticket<br>\\nBack\`tick and \\\"double\\\" and \'single\' quotes<br>\\nA backslash \\\\ in the middle<br>\\nClose tag lookalike: &lt;/p&gt; stays text<br>\\nPath-ish: a/b/c and https://example.invalid/x<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <a target=\\\"_blank\\\" href=\\\"https://genius.com/Nora-vale-dollar-sign-lyrics\\\">\\n    <div class=\\\"song_title\\\">\$ign of the Times<\/div>\\n    Nora Vale\\n<\/a><\/div>\\n\\n  <\/div>\\n<iframe src=\\\"//genius.com/songs/90000014/embed_analytics\\\"\\n              class=\\\"rg_embed_analytics\\\"\\n              id=\\\"rg_embed_analytics_90000014\\\"><\/iframe>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;
    const JS_UNICODE_ESCAPES: &str = r#"if (document.getElementById('rg_embed_link_90000016')) {
  document.write('<link rel="stylesheet" media="screen" href="https://assets.genius.com/stylesheets/compiled/embedded_song-e8cb29d87f65b77ac44550874d60bddd.css" />')
  document.write(JSON.parse('\"\\n  <div class=\\\"rg_embed music\\\" id=\\\"rg_embed_90000016\\\" data-domain=\\\"genius.com\\\">\\n    <div class=\\\"rg_embed_body\\\">\\n  <p>Caf\u00e9 caf\\u00e9 \\ud83c\\udf19<br>\\n\ud83c\udf19 \x41\x42 slash\/ok<br>\\nend \\\"q\\\"<\/p>\\n\\n\\n<\/div>\\n\\n    <div class=\\\"rg_embed_footer\\\">\\n  <\/div>\"'))
  document.write('<script src="https://assets.genius.com/javascripts/compiled/embedded_song-f6614cf11067947616d840277d246cdd.js" async="async" crossorigin="true"></script>')
}
"#;

    /// An embed.js written the way Genius writes one around `body` (the
    /// HTML inside `rg_embed_body`): JSON-encoded, then escaped like Rails'
    /// `escape_javascript`.
    fn embed_js(body: &str) -> String {
        let html = format!(
            concat!(
                "\n  <div class=\"rg_embed music\" id=\"rg_embed_90000099\" data-domain=\"genius.com\">\n",
                "    <div class=\"rg_embed_header\">\n",
                "  <a target=\"_blank\" class=\"rg_embed_logo\" href=\"https://genius.com/Nora-vale-test-lyrics\">Powered by Genius</a>\n",
                "</div>\n\n",
                "    <div class=\"rg_embed_body\">{}</div>\n\n",
                "    <div class=\"rg_embed_footer\">\n",
                "  <a target=\"_blank\" href=\"https://genius.com/Nora-vale-test-lyrics\">\n",
                "    <div class=\"song_title\">Test Song</div>\n",
                "    Nora Vale\n",
                "</a></div>\n\n",
                "  </div>\n",
                "<iframe src=\"//genius.com/songs/90000099/embed_analytics\"\n",
                "              class=\"rg_embed_analytics\"\n",
                "              id=\"rg_embed_analytics_90000099\"></iframe>"
            ),
            body
        );
        let json = serde_json::to_string(&html).unwrap();
        let mut literal = String::new();
        let mut chars = json.chars().peekable();
        while let Some(c) = chars.next() {
            match c {
                '\\' | '"' | '\'' | '`' | '$' => {
                    literal.push('\\');
                    literal.push(c);
                }
                '<' if chars.peek() == Some(&'/') => literal.push_str("<\\"),
                c => literal.push(c),
            }
        }
        format!(
            concat!(
                "if (document.getElementById('rg_embed_link_90000099')) {{\n",
                "  document.write('<link rel=\"stylesheet\" media=\"screen\" href=\"https://assets.genius.com/stylesheets/compiled/embedded_song-0.css\" />')\n",
                "  document.write(JSON.parse('{}'))\n",
                "  document.write('<script src=\"https://assets.genius.com/javascripts/compiled/embedded_song-0.js\" async=\"async\" crossorigin=\"true\"></script>')\n",
                "}}\n"
            ),
            literal
        )
    }

    /// The lines of an embed whose single `<p>` holds `inner`.
    fn parse_p(inner: &str) -> Option<Vec<String>> {
        parse_body(&format!("\n  <p>{inner}</p>\n\n\n"))
    }

    fn parse_body(body: &str) -> Option<Vec<String>> {
        parse_embed(&embed_js(body)).expect("a readable embed")
    }

    fn lines(expected: &[&str]) -> Option<Vec<String>> {
        Some(expected.iter().map(|line| line.to_string()).collect())
    }

    /// A referent link (an explained fragment) as Genius writes it.
    fn anchor(id: u32) -> String {
        format!(
            r#"<a href="https://genius.com/{id}/Nora-vale-fixture/Invented-referent-{id}" data-id="{id}" class="has_comments" data-editorial-state="accepted" data-classification="accepted" target="_blank" onclick="return false" data-genius-referent-id="{id}">"#
        )
    }

    #[test]
    fn unescapes_javascript_strings() {
        assert_eq!(unescape_js(r#"a\'b\"c\\d\/e\$f\`g"#), "a'b\"c\\d/e$f`g");
        assert_eq!(unescape_js(r"1\n2\t3\r"), "1\n2\t3\r");
        assert_eq!(unescape_js(r"\x41\x42 \xZZ"), "AB xZZ");
        assert_eq!(unescape_js(r"café é"), "café é");
        assert_eq!(unescape_js(r"🌙 \u{1F319}"), "🌙 🌙");
        assert_eq!(unescape_js(r"lone \ud83c!"), "lone \u{fffd}!");
        assert_eq!(unescape_js(r"short \u12"), "short u12");
        assert_eq!(unescape_js("\\é and raw é"), "é and raw é");
        assert_eq!(unescape_js("line \\\ncontinued"), "line continued");
        assert_eq!(unescape_js("ends with \\"), "ends with ");
        assert_eq!(unescape_js(""), "");
    }

    #[test]
    fn reads_an_embed_byte_for_byte_as_genius_writes_it() {
        // Header "[Letra de "...]" (its quote left open, as Genius keeps
        // it), section headers, quotes, apostrophes, "&amp;", a slash
        assert_eq!(
            parse_embed(PT_BR_HEADER).unwrap(),
            lines(&[
                "Acordei com o mundo de ponta-cabeça",
                "E avisou que o dia 'tava lento",
                "(Ei, ei, ei—)",
                "",
                "Comprei um sonho em \"promoção\", sem troco",
                "Dobrei a lua em papel e guardei no bolso",
                "D'água, d'ouro, d’ali: tudo cabe no meu som",
                "Rock & Roll de esquina, dia/noite sem horário",
                "Ninguém sabe o que é ser de lá — nem eu",
                "",
                "Lua de papel, lua de papel",
                "Amassa mas não rasga",
                "",
                "Coração de lata, açúcar no café",
                "Fé, fé, fé",
            ])
        );
        // The helper below writes embeds the same way
        let rebuilt = embed_js("\n  <p>Fé, \"fé\" &amp; \'fé\'<br>\nfim</p>\n\n\n");
        assert_eq!(
            parse_embed(&rebuilt).unwrap(),
            lines(&["Fé, \"fé\" & 'fé'", "fim"])
        );
    }

    #[test]
    fn javascript_and_json_escapes_both_unescape() {
        assert_eq!(
            parse_embed(JS_ESCAPES).unwrap(),
            lines(&[
                "Ty Dolla $ign said it's 5$ a ticket",
                "Back`tick and \"double\" and 'single' quotes",
                "A backslash \\ in the middle",
                "Close tag lookalike: </p> stays text",
                "Path-ish: a/b/c and https://example.invalid/x",
            ])
        );
        // JS-level and JSON-level \u escapes, surrogate pairs, \x, \/
        assert_eq!(
            parse_embed(JS_UNICODE_ESCAPES).unwrap(),
            lines(&["Café café 🌙", "🌙 AB slash/ok", "end \"q\""])
        );
    }

    #[test]
    fn explained_fragments_keep_their_text_wherever_they_start_and_end() {
        let html = format!(
            concat!(
                "{a1}[Intro]<br>\nPaper boats on a copper tide<br>\nNobody counts them as they leave<br>\n",
                "Hold up (Hold up, hold up)</a><br>\n<br>\n",
                "{a2}[Verse 1]<br>\nI painted every window shut<br>\nKept the light inside a jar</a><br>\n",
                "{a3}Gave the neighbours half my name</a><br>\nLeft the other half to the stars<br>\n",
                "and a {a4}quiet harbor, glass and rain</a>, just like that<br>\n",
                "{a5}Tide goes out, the ledger stays<br>\nMorning smells like diesel and salt</a>, oh<br>\n",
                "{a6}\u{200b}</a><br>\n<br>\n",
                "[Chorus]<br>\n{a7}Harbor, harbor, hold me slow<br>\nHarbor, harbor, let me go</a><br>\n<br>\n",
                "{a8}[Outro]<br>\nLights out on the water</a>"
            ),
            a1 = anchor(11000001),
            a2 = anchor(11000002),
            a3 = anchor(11000003),
            a4 = anchor(11000004),
            a5 = anchor(11000005),
            a6 = anchor(11000006),
            a7 = anchor(11000007),
            a8 = anchor(11000008),
        );
        assert_eq!(
            parse_p(&html),
            lines(&[
                "Paper boats on a copper tide",
                "Nobody counts them as they leave",
                "Hold up (Hold up, hold up)",
                "",
                "I painted every window shut",
                "Kept the light inside a jar",
                "Gave the neighbours half my name",
                "Left the other half to the stars",
                "and a quiet harbor, glass and rain, just like that",
                "Tide goes out, the ledger stays",
                "Morning smells like diesel and salt, oh",
                "",
                "Harbor, harbor, hold me slow",
                "Harbor, harbor, let me go",
                "",
                "Lights out on the water",
            ])
        );
    }

    #[test]
    fn italics_and_bold_go_even_across_lines_and_inside_headers() {
        let html = format!(
            concat!(
                "[Verse 1: Nora Vale <i>&amp;</i> <b>Kit</b>]<br>\n",
                "I said (<i>wait</i>) and you said (<i>no</i>)<br>\n",
                "Everything <b>loud</b> sounds like home<br>\n",
                "<i>Whispered across two lines<br>\nand finished in italics</i><br>\n",
                "Plain again, then <b>bold that wraps<br>\nonto the next line</b> and out<br>\n<br>\n",
                "[Chorus: <i>Both</i>]<br>\n",
                "{a1}Say it <i>slow</i>, say it <b>twice</b></a><br>\n",
                "{a2}(<i>Say it</i>)</a>, say it again"
            ),
            a1 = anchor(11100001),
            a2 = anchor(11100002),
        );
        assert_eq!(
            parse_p(&html),
            lines(&[
                "I said (wait) and you said (no)",
                "Everything loud sounds like home",
                "Whispered across two lines",
                "and finished in italics",
                "Plain again, then bold that wraps",
                "onto the next line and out",
                "",
                "Say it slow, say it twice",
                "(Say it), say it again",
            ])
        );
    }

    #[test]
    fn entities_are_decoded_once_after_the_tags_go() {
        let html = concat!(
            "Salt &amp; vinegar, rock &amp; roll<br>\n",
            "I &lt;3 you, 2 &gt; 1<br>\n",
            "She said &quot;go&quot; and I said &#39;no&#39;<br>\n",
            "Don&#x27;t wait, don&#8217;t stay<br>\n",
            "Non&nbsp;breaking&nbsp;space<br>\n",
            "&lt;b&gt;not bold&lt;/b&gt; is just text<br>\n",
            "AT&T, R&D and an unknown &foo; entity<br>\n",
            "&amp;lt; stays one level only<br>\n",
            "A raw I <3 you stays too"
        );
        assert_eq!(
            parse_p(html),
            lines(&[
                "Salt & vinegar, rock & roll",
                "I <3 you, 2 > 1",
                "She said \"go\" and I said 'no'",
                "Don't wait, don’t stay",
                "Non breaking space",
                "<b>not bold</b> is just text",
                "AT&T, R&D and an unknown &foo; entity",
                "&lt; stays one level only",
                "A raw I <3 you stays too",
            ])
        );
    }

    #[test]
    fn section_headers_become_one_stanza_break() {
        let html = concat!(
            "[Intro]<br>\nfirst line right after a header<br>\nsecond line<br>\n",
            "[Verse 2]<br>\nheader glued to the previous line, no blank before it<br>\n<br>\n<br>\n",
            "[Pre-Chorus]<br>\n[Chorus]<br>\ntwo headers in a row and extra blanks<br>\n<br>\n",
            "[Wstęp: Nora, Kit, Razem/Chór]:<br>\n",
            "And if the lights go out before the bridge— [Gunshots]<br>\n",
            "Say it twice [x2]<br>\n(Verse 1)<br>\n[Verse 3: Nora [live]]<br>\n[]<br>\n",
            "  indented line with spaces  <br>\nends here<br>\n<br>\n[Refren: Razem/Chór]: <br>\n",
            "after a header with a colon<br>\n[Outro]."
        );
        assert_eq!(
            parse_p(html),
            lines(&[
                "first line right after a header",
                "second line",
                "",
                "header glued to the previous line, no blank before it",
                "",
                "two headers in a row and extra blanks",
                "",
                "And if the lights go out before the bridge— [Gunshots]",
                "Say it twice [x2]",
                "(Verse 1)",
                "",
                "indented line with spaces",
                "ends here",
                "",
                "after a header with a colon",
            ])
        );
    }

    #[test]
    fn other_scripts_and_hidden_zero_width_spaces() {
        // Japanese, the localised header behind a zero-width space
        assert_eq!(
            parse_p(concat!(
                "\u{200b}[Nora Vale「月の紙」歌詞]<br>\n<br>\n[Verse 1]<br>\n",
                "朝の窓に月が残ってた<br>\n紙みたいに薄い光で<br>\nポケットに入れて歩いた<br>\n<br>\n",
                "[Chorus]<br>\nねえ、まだ眠れないの？<br>\nひらひら、ひらひら"
            )),
            lines(&[
                "朝の窓に月が残ってた",
                "紙みたいに薄い光で",
                "ポケットに入れて歩いた",
                "",
                "ねえ、まだ眠れないの？",
                "ひらひら、ひらひら",
            ])
        );
        assert_eq!(
            parse_p(concat!(
                "[노라 베일 \"종이 달\" 가사]<br>\n<br>\n[Verse 1: 노라 <i>&amp;</i> 케이]<br>\n",
                "밤하늘에 종이 달을 접어<br>\n주머니에 넣고 걸어가<br>\n(<i>오, 오, 오</i>)<br>\n<br>\n",
                "[Chorus]<br>\n아무도 몰라 내 이름"
            )),
            lines(&[
                "밤하늘에 종이 달을 접어",
                "주머니에 넣고 걸어가",
                "(오, 오, 오)",
                "",
                "아무도 몰라 내 이름",
            ])
        );
        assert_eq!(
            parse_p(concat!(
                "[المقطع الأول]<br>\nالقمر فوق البحر<br>\nوالليل هادئ<br>\n<br>\n",
                "[اللازمة]<br>\nأغني لك وحدي"
            )),
            lines(&["القمر فوق البحر", "والليل هادئ", "", "أغني لك وحدي"])
        );
    }

    #[test]
    fn other_markup_still_reads() {
        assert_eq!(
            parse_body(concat!(
                "\n  <p>First paragraph, line one<br/>\nline two<br />\nline three<BR>\nline four</p>\n\n",
                "  <p>Second paragraph <span class=\"x\">with a span</span>, <em>em</em> and ",
                "<strong>strong</strong>.<br>\nraw newline inside\na line, joins with a space</p>\n\n\n"
            )),
            lines(&[
                "First paragraph, line one",
                "line two",
                "line three",
                "line four",
                "",
                "Second paragraph with a span, em and strong.",
                "raw newline inside a line, joins with a space",
            ])
        );
        // A translation page reads like any other (the search never picks it)
        assert_eq!(
            parse_p(concat!(
                "[Tradução de \"Lua de Papel\", de Nora Vale]<br>\n<br>\n[Introdução]<br>\n",
                "Eu acordei com o mundo de cabeça para baixo<br>\n<br>\n[Verso 1]<br>\n",
                "Comprei um sonho em promoção"
            )),
            lines(&[
                "Eu acordei com o mundo de cabeça para baixo",
                "",
                "Comprei um sonho em promoção",
            ])
        );
    }

    #[test]
    fn songs_without_lyrics_give_none() {
        // Instrumental, also inside an explained fragment
        assert_eq!(parse_p("[Instrumental]"), None);
        assert_eq!(
            parse_p(&format!("{}[Instrumental]</a>", anchor(11200001))),
            None
        );
        // Unreleased: an empty body
        assert_eq!(parse_body("\n  \n"), None);
        assert_eq!(
            parse_p("Lyrics for this song have yet to be released"),
            None
        );
        assert_eq!(
            parse_p("Lyrics for this song have yet to be transcribed<br>\nPlease check back once the song has been released."),
            None
        );
        // A song that sings the word keeps its lyrics
        assert_eq!(
            parse_p("This part is instrumental<br>\nthen I come back in<br>\nand sing again"),
            lines(&[
                "This part is instrumental",
                "then I come back in",
                "and sing again"
            ])
        );
    }

    #[test]
    fn an_embed_unlike_genius_is_an_error_not_an_empty_song() {
        assert!(parse_embed("").is_err());
        assert!(parse_embed("<html>Sorry</html>").is_err());
        // The literal never closes
        assert!(parse_embed("document.write(JSON.parse('\"abc").is_err());
        // Not JSON inside
        assert!(parse_embed("document.write(JSON.parse('<p>raw</p>'))").is_err());
        // No lyrics body
        assert!(parse_embed(
            r#"document.write(JSON.parse('\"<div class=\\\"other\\\">x<\/div>\"'))"#
        )
        .is_err());
    }

    #[test]
    fn long_explained_lyrics_come_whole() {
        let mut html = String::new();
        let mut expected: Vec<String> = Vec::new();
        for section in 1..=30u32 {
            if section > 1 {
                html.push_str("<br>\n<br>\n");
                expected.push(String::new());
            }
            html.push_str(&format!("[Verse {section}: Nora Vale]<br>\n"));
            for line in 1..=8u32 {
                if line > 1 {
                    html.push_str("<br>\n");
                }
                let text =
                    format!("Section {section}, line {line}: the copper harbor paints the orchard");
                // Every other line explained
                if line % 2 == 1 {
                    html.push_str(&format!(
                        "{}{text}</a>",
                        anchor(12_000_000 + section * 100 + line)
                    ));
                } else {
                    html.push_str(&text);
                }
                expected.push(text);
            }
        }
        let js = embed_js(&format!("\n  <p>{html}</p>\n\n\n"));
        assert!(js.len() > 30_000, "{} bytes", js.len());
        assert_eq!(parse_embed(&js).unwrap(), Some(expected));
    }

    /// The same eight songs a search for "Nora Vale Lua de Papel" could
    /// give: translations and other versions before the song itself.
    fn hits_json(with_instrumental: bool) -> Vec<Value> {
        let hit = |id: u64,
                   title: &str,
                   artist_names: &str,
                   primary: &str,
                   state: &str,
                   instrumental: bool| {
            let mut result = serde_json::json!({
                "_type": "song",
                "id": id,
                "title": title,
                "artist_names": artist_names,
                "primary_artist": { "name": primary },
                "lyrics_state": state,
                "url": format!("https://genius.com/fixture-{id}-lyrics"),
            });
            if with_instrumental {
                result["instrumental"] = Value::Bool(instrumental);
            }
            serde_json::json!({ "index": "song", "type": "song", "result": result })
        };
        vec![
            hit(
                90000101,
                "Nora Vale - Lua de Papel (Tradução em Português)",
                "Genius Brasil Traduções",
                "Genius Brasil Traduções",
                "complete",
                false,
            ),
            hit(
                90000102,
                "Lua de Papel",
                "Beto Cardoso",
                "Beto Cardoso",
                "complete",
                false,
            ),
            hit(
                90000001,
                "Lua de Papel",
                "Nora Vale (Ft. Dani Rocha)",
                "Nora Vale",
                "complete",
                false,
            ),
            hit(
                90000103,
                "Lua de Papel (Demo)",
                "Nora Vale",
                "Nora Vale",
                "unreleased",
                false,
            ),
            hit(
                90000104,
                "Lua de Papel (Instrumental)",
                "Nora Vale",
                "Nora Vale",
                "complete",
                true,
            ),
            hit(
                90000105,
                "Nora Vale - Lua de Papel (English Translation)",
                "Genius English Translations",
                "Genius English Translations",
                "complete",
                false,
            ),
            hit(
                90000106,
                "CÉU DE GIZ",
                "Lua Nova MC's",
                "Lua Nova MC\u{2019}s",
                "complete",
                false,
            ),
            hit(
                90000107,
                "Hustlin\u{2019} Hard",
                "Os Vale & Cia",
                "Os Vale",
                "complete",
                false,
            ),
        ]
    }

    fn public_search(hits: Vec<Value>) -> Value {
        serde_json::json!({
            "meta": { "status": 200 },
            "response": {
                "sections": [
                    // A top hit section repeats a song and holds an artist
                    { "type": "top_hit", "hits": [
                        { "index": "artist", "type": "artist", "result": { "id": 7, "name": "Nora Vale" } },
                        hits[2].clone()
                    ] },
                    { "type": "song", "hits": hits }
                ],
                "next_page": 2
            }
        })
    }

    fn official_search(hits: Vec<Value>) -> Value {
        serde_json::json!({ "meta": { "status": 200 }, "response": { "hits": hits } })
    }

    fn pick(hits: &[GeniusSongHit], artist: &str, title: &str) -> Option<u64> {
        pick_lyrics_hit(hits, artist, title).map(|hit| hit.id)
    }

    #[test]
    fn both_search_answers_read_the_same() {
        let public = search_hits(&public_search(hits_json(true)));
        let official = search_hits(&official_search(hits_json(false)));
        let ids = |hits: &[GeniusSongHit]| hits.iter().map(|h| h.id).collect::<Vec<_>>();
        // The top hit comes first, once; the artist is no song
        assert_eq!(
            ids(&public),
            [90000001, 90000101, 90000102, 90000103, 90000104, 90000105, 90000106, 90000107]
        );
        assert_eq!(
            ids(&official),
            [90000101, 90000102, 90000001, 90000103, 90000104, 90000105, 90000106, 90000107]
        );
        let song = &official[2];
        assert_eq!(song.title, "Lua de Papel");
        assert_eq!(song.primary_artist, "Nora Vale");
        assert_eq!(song.artist_names, "Nora Vale (Ft. Dani Rocha)");
        assert_eq!(song.url, "https://genius.com/fixture-90000001-lyrics");
        assert_eq!(song.lyrics_state, "complete");
        assert!(public.iter().any(|h| h.instrumental));
        assert!(!official.iter().any(|h| h.instrumental), "absent means no");

        let empty_public = r#"{"meta":{"status":200},"response":{"sections":[{"type":"song","hits":[]}],"next_page":null}}"#;
        let empty_official = r#"{"meta":{"status":200},"response":{"hits":[]}}"#;
        let refused = r#"{"meta":{"status":401,"message":"This call requires an access_token. Please see: https://genius.com/developers"}}"#;
        for body in [empty_public, empty_official, refused] {
            assert!(search_hits(&serde_json::from_str(body).unwrap()).is_empty());
        }
    }

    #[test]
    fn the_song_itself_is_picked_from_either_search() {
        for hits in [
            search_hits(&public_search(hits_json(true))),
            search_hits(&official_search(hits_json(false))),
        ] {
            assert_eq!(pick(&hits, "Nora Vale", "Lua de Papel"), Some(90000001));
            assert_eq!(
                pick(
                    &hits,
                    "Nora Vale, Dani Rocha",
                    "Lua de Papel (feat. Dani Rocha)"
                ),
                Some(90000001)
            );
            assert_eq!(pick(&hits, "Beto Cardoso", "Lua de Papel"), Some(90000102));
            // Case, accents and a curly apostrophe in Genius's names
            assert_eq!(pick(&hits, "Lua Nova MC's", "Ceu de Giz"), Some(90000106));
            assert_eq!(pick(&hits, "Os Vale", "Hustlin' Hard"), Some(90000107));
            // Same title, someone else
            assert_eq!(pick(&hits, "Maria Souza", "Lua de Papel"), None);
            // Without the song, nothing else of the title is taken:
            // translations, the unreleased demo, the instrumental
            let others: Vec<GeniusSongHit> =
                hits.iter().filter(|h| h.id != 90000001).cloned().collect();
            assert_eq!(pick(&others, "Nora Vale", "Lua de Papel"), None);
        }
    }

    fn song(id: u64, title: &str, primary: &str, artist_names: &str) -> GeniusSongHit {
        GeniusSongHit {
            id,
            title: title.into(),
            primary_artist: primary.into(),
            artist_names: artist_names.into(),
            url: format!("https://genius.com/fixture-{id}-lyrics"),
            lyrics_state: "complete".into(),
            instrumental: false,
        }
    }

    #[test]
    fn names_match_across_case_accents_and_apostrophes() {
        let hits = [song(14154298, "ALGO LÚCIDO", "FROID", "FROID")];
        assert_eq!(pick(&hits, "Froid", "Algo Lucido"), Some(14154298));
        assert_eq!(
            pick(&hits, "Froid", "Algo Lúcido - Ao Vivo"),
            Some(14154298)
        );
        assert_eq!(
            match_key("Straße Ñandú Łódź Æon"),
            "strasse nandu lodz aeon"
        );
        assert_eq!(match_key("MC’s  &  MCʼs"), "mcs and mcs");
        // "&" and "and" are one word
        let hits = [song(21, "Me and U", "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", "Me & U"), Some(21));
        // Accents as marks of their own
        assert_eq!(match_key("Lu\u{0301}cido"), "lucido");
    }

    #[test]
    fn the_names_a_song_goes_by_match() {
        // Japanese and Korean pages carry both names
        let hits = [song(
            5638888,
            "夜に駆ける (Yoru ni Kakeru)",
            "YOASOBI",
            "YOASOBI",
        )];
        assert_eq!(pick(&hits, "YOASOBI", "夜に駆ける"), Some(5638888));
        assert_eq!(pick(&hits, "YOASOBI", "Yoru ni Kakeru"), Some(5638888));
        let hits = [song(
            764417,
            "봄날 (Spring Day)",
            "BTS (방탄소년단)",
            "BTS (방탄소년단)",
        )];
        assert_eq!(pick(&hits, "BTS", "Spring Day"), Some(764417));
        // A soundtrack edition on Spotify, the song on Genius
        let hits = [song(1, "Let It Go", "Idina Menzel", "Idina Menzel")];
        assert_eq!(
            pick(
                &hits,
                "Idina Menzel",
                "Let It Go - From \"Frozen\"/Soundtrack Version"
            ),
            Some(1)
        );
        // A feature written "part." (Portuguese) is not a part number
        let hits = [song(2, "Coração", "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", "Coração (part. Froid)"), Some(2));
        // A longer Genius title around the track's
        let hits = [song(
            3,
            "Shakira: Bzrp Music Sessions, Vol. 53",
            "Bizarrap",
            "Bizarrap & Shakira",
        )];
        assert_eq!(
            pick(&hits, "Bizarrap, Shakira", "Bzrp Music Sessions, Vol. 53"),
            Some(3)
        );
        assert_eq!(
            pick(&hits, "Shakira, Bizarrap", "Bzrp Music Sessions, Vol. 53"),
            Some(3)
        );
        // Someone else's song with the track's main artist on it
        let hits = [song(
            4,
            "Lua de Papel",
            "Nora Vale",
            "Nora Vale (Ft. Dani Rocha)",
        )];
        assert_eq!(pick(&hits, "Dani Rocha", "Lua de Papel"), Some(4));
    }

    #[test]
    fn other_songs_versions_and_parts_are_never_taken() {
        // A remix with the artist on it
        let remix = song(
            9,
            "HUMBLE. (SKRILLEX REMIX)",
            "Skrillex",
            "Skrillex & Kendrick Lamar",
        );
        assert_eq!(pick(&[remix.clone()], "Kendrick Lamar", "HUMBLE."), None);
        let original = song(3039923, "HUMBLE.", "Kendrick Lamar", "Kendrick Lamar");
        assert_eq!(
            pick(&[remix, original], "Kendrick Lamar", "HUMBLE."),
            Some(3039923)
        );
        // A live page only for a live track
        let live = [song(
            10,
            "Numb (Live in Texas)",
            "Linkin Park",
            "Linkin Park",
        )];
        assert_eq!(pick(&live, "Linkin Park", "Numb"), None);
        assert_eq!(pick(&live, "Linkin Park", "Numb (Live in Texas)"), Some(10));
        // A title inside a much longer one is another song
        let love_me_do = [song(11, "Love Me Do", "The Beatles", "The Beatles")];
        assert_eq!(pick(&love_me_do, "The Beatles", "Love"), None);
        let love = [song(12, "Love", "The Beatles", "The Beatles")];
        assert_eq!(pick(&love, "The Beatles", "Love Me Do"), None);
        // Another part, either way
        let part_two = [song(13, "Algo Lúcido, Pt. 2", "Froid", "Froid")];
        assert_eq!(pick(&part_two, "Froid", "Algo Lúcido"), None);
        let part_one = [song(14, "Algo Lúcido", "Froid", "Froid")];
        assert_eq!(pick(&part_one, "Froid", "Algo Lúcido (Parte 2)"), None);
        let session = [song(
            15,
            "Bzrp Music Sessions, Vol. 52",
            "Bizarrap",
            "Bizarrap",
        )];
        assert_eq!(
            pick(&session, "Bizarrap", "Bzrp Music Sessions, Vol. 53"),
            None
        );
        // Translation pages, whatever they are called
        let romanized = [
            song(
                16,
                "YOASOBI - 夜に駆ける (Yoru ni Kakeru) (Romanized)",
                "Genius Romanizations",
                "Genius Romanizations",
            ),
            song(17, "夜に駆ける (English Translation)", "YOASOBI", "YOASOBI"),
            song(
                18,
                "夜に駆ける",
                "Polskie tłumaczenia Genius",
                "Polskie tłumaczenia Genius",
            ),
        ];
        assert_eq!(pick(&romanized, "YOASOBI", "夜に駆ける"), None);
        // Unreleased or instrumental, even with the right name
        let mut unreleased = song(19, "Algo Lúcido", "Froid", "Froid");
        unreleased.lyrics_state = "unreleased".into();
        let mut instrumental = song(20, "Algo Lúcido", "Froid", "Froid");
        instrumental.instrumental = true;
        assert_eq!(
            pick(&[unreleased, instrumental], "Froid", "Algo Lúcido"),
            None
        );
        // Nothing to compare
        assert_eq!(pick(&part_one, "", "Algo Lúcido"), None);
        assert_eq!(pick(&part_one, "Froid", ""), None);
    }

    #[test]
    fn without_the_song_its_siblings_are_never_taken() {
        // Another number is another song, either way
        for (track, hit) in [
            ("Alexa", "ALEXA 2"),
            ("Song 2", "Song"),
            ("Tema", "Tema II"),
            ("Fique Rico ou Moralismo", "Fique Rico ou Moralismo 2"),
            ("Lua de Papel", "Lua de Papel (2)"),
        ] {
            let hits = [song(30, hit, "Nora Vale", "Nora Vale")];
            assert_eq!(pick(&hits, "Nora Vale", track), None, "{track} took {hit}");
        }
        let hits = [song(31, "Fique Rico ou Moralismo 2", "Froid", "Froid")];
        assert_eq!(
            pick(&hits, "Froid, Major RD", "Fique Rico ou Moralismo 2"),
            Some(31)
        );
        // Other takes and pieces of an album, either way
        for hit in [
            "Solo (Reprise)",
            "Solo (Skit)",
            "Solo (Acoustic)",
            "Solo (Extended)",
            "Solo (Radio Edit)",
            "Solo (Club Mix)",
            "Solo (Edit)",
            "Solo (Version)",
            "Solo (Piano)",
            "Solo (VIP)",
            "Solo (Interlúdio)",
        ] {
            let hits = [song(32, hit, "Nora Vale", "Nora Vale")];
            assert_eq!(pick(&hits, "Nora Vale", "Solo"), None, "Solo took {hit}");
        }
        let hits = [song(33, "Solo", "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", "Solo (Reprise)"), None);
        assert_eq!(pick(&hits, "Nora Vale", "Solo - Skit"), None);
        // One word is never found inside a longer title
        let hits = [song(34, "Solo Dolo", "Nora Vale", "Nora Vale")];
        assert_eq!(pick(&hits, "Nora Vale", "Solo"), None);
        // "Intro" and the like: too common to tell apart by name
        let hits = [
            song(35, "Humor Negro (Intro)", "Nora Vale", "Nora Vale"),
            song(36, "Intro", "Nora Vale", "Nora Vale"),
        ];
        assert_eq!(pick(&hits, "Nora Vale", "Intro"), None);
        assert_eq!(pick(&hits, "Nora Vale", "Outro 2"), None);
        assert_eq!(pick(&hits, "Nora Vale", "Humor Negro"), None);
        // Someone else's song the artist is credited on: only by the very
        // same title ("A Língua dos Anjos" is another song)
        let other = [song(
            37,
            "A Língua dos Anjos",
            "Orquestra Novo Traço",
            "Orquestra Novo Traço (Ft. Froid)",
        )];
        assert_eq!(pick(&other, "Froid", "Língua dos Anjos"), None);
        let credited = [song(
            38,
            "Dispiei",
            "Pirâmide Perdida",
            "Pirâmide Perdida (Ft. BK', Bril, Nectar Gang & Sain)",
        )];
        assert_eq!(pick(&credited, "Sain", "Dispiei"), Some(38));
    }

    #[test]
    fn two_songs_matching_alike_are_none_and_the_same_version_wins() {
        // Two pages of one title by one artist can't be told apart
        let twins = [
            song(40, "Lua de Papel", "Nora Vale", "Nora Vale"),
            song(41, "Lua de Papel", "Nora Vale", "Nora Vale"),
        ];
        assert_eq!(pick(&twins, "Nora Vale", "Lua de Papel"), None);
        // A better match still wins over them
        let mut hits = twins.to_vec();
        hits.push(song(42, "Lua de Papel (Ao Vivo)", "Nora Vale", "Nora Vale"));
        assert_eq!(pick(&hits, "Nora Vale", "Lua de Papel (Ao Vivo)"), Some(42));
        // The live track takes the live page, whatever Genius's order; the
        // studio track never takes it
        let studio = song(43, "Colômbia", "Froid", "Froid");
        let live = song(44, "Colômbia (Ao Vivo)", "Froid", "Froid");
        for hits in [
            [studio.clone(), live.clone()],
            [live.clone(), studio.clone()],
        ] {
            assert_eq!(pick(&hits, "Froid", "Colômbia (Ao Vivo)"), Some(44));
            assert_eq!(pick(&hits, "Froid", "Colômbia - Ao Vivo"), Some(44));
            assert_eq!(pick(&hits, "Froid", "Colômbia"), Some(43));
        }
    }

    /// A Genius as `server`, for the look-up to ask.
    fn endpoints(server: &crate::test_server::TestServer) -> GeniusEndpoints {
        GeniusEndpoints {
            public_search: format!("{}/api/search/song", server.base),
            api_search: format!("{}/official/search", server.base),
            songs: format!("{}/songs", server.base),
        }
    }

    const ONE_LINE_EMBED: &str = "\n  <p>[Intro]<br>\nPaper boats on a copper tide</p>\n\n\n";

    async fn look_up(
        server: &crate::test_server::TestServer,
        token: &str,
    ) -> Result<Option<GeniusLyrics>, DomainError> {
        let client = crate::test_server::client();
        let at = endpoints(server);
        let found =
            fetch_genius_lyrics_from(&at, &client, "Nora Vale", "Lua de Papel", token).await;
        found
    }

    #[tokio::test]
    async fn a_refused_token_searches_without_it() {
        use crate::test_server::TestServer;
        let public = public_search(hits_json(true)).to_string();
        let embed = embed_js(ONE_LINE_EMBED);
        let server = TestServer::start(&[
            ("/official/search", 401, r#"{"meta":{"status":401}}"#),
            ("/api/search/song", 200, public.as_str()),
            ("/songs/90000001/embed.js", 200, embed.as_str()),
        ]);
        let found = look_up(&server, "test-token").await.unwrap().unwrap();
        assert_eq!(found.song_id, 90000001);
        assert_eq!(found.lines, ["Paper boats on a copper tide"]);
        // The token went to the official API only
        let official = server.requests("/official/search");
        assert_eq!(official.len(), 1);
        assert!(official[0]
            .to_ascii_lowercase()
            .contains("authorization: bearer"));
        let searches = server.requests("/api/search/song");
        assert_eq!(searches.len(), 1);
        assert!(!searches[0].to_ascii_lowercase().contains("authorization"));
        assert!(searches[0].contains("q=Nora+Vale+Lua+de+Papel"));
    }

    #[tokio::test]
    async fn genius_refusing_never_reads_as_an_expired_login() {
        use crate::test_server::TestServer;
        let server = TestServer::start(&[("/api/search/song", 403, "<html>blocked</html>")]);
        let error = look_up(&server, "").await.unwrap_err();
        assert!(
            matches!(error, DomainError::ClientError { status: 403, .. }),
            "{error:?}"
        );
        // Neither with a token both searches refuse
        let server = TestServer::start(&[
            ("/official/search", 401, "{}"),
            ("/api/search/song", 401, "{}"),
        ]);
        let error = look_up(&server, "test-token").await.unwrap_err();
        assert!(!matches!(error, DomainError::AuthExpired), "{error:?}");
    }

    #[tokio::test]
    async fn the_second_query_finds_what_the_first_missed() {
        use crate::test_server::TestServer;
        // Only translation pages come back for "artist title"
        let only_translations = serde_json::json!({ "response": { "sections": [
            { "type": "song", "hits": [hits_json(true)[0].clone(), hits_json(true)[5].clone()] }
        ] } })
        .to_string();
        let public = public_search(hits_json(true)).to_string();
        let embed = embed_js(ONE_LINE_EMBED);
        let server = TestServer::start(&[
            ("q=Nora+Vale+Lua", 200, only_translations.as_str()),
            ("q=Lua+de+Papel+Nora", 200, public.as_str()),
            ("/songs/90000001/embed.js", 200, embed.as_str()),
        ]);
        let found = look_up(&server, "").await.unwrap().unwrap();
        assert_eq!(found.song_id, 90000001);
        assert_eq!(server.requests("/api/search/song").len(), 2);
    }

    #[tokio::test]
    async fn the_embed_answer_decides_none_or_an_error() {
        use crate::test_server::TestServer;
        let public = public_search(hits_json(true)).to_string();
        let answer = |status| {
            let server = TestServer::start(&[
                ("/api/search/song", 200, public.as_str()),
                ("/songs/90000001/embed.js", status, "{}"),
            ]);
            async move { look_up(&server, "").await }
        };
        // No such song: no lyrics, not an error
        assert_eq!(answer(404).await.unwrap(), None);
        assert!(matches!(
            answer(429).await,
            Err(DomainError::RateLimited { .. })
        ));
        assert!(matches!(
            answer(503).await,
            Err(DomainError::ServerError { status: 503, .. })
        ));
        // Not an embed this knows
        assert!(matches!(answer(200).await, Err(DomainError::Parse(_))));
    }

    #[tokio::test]
    async fn an_intro_is_never_searched_for() {
        use crate::test_server::TestServer;
        let server = TestServer::start(&[]);
        let client = crate::test_server::client();
        let found =
            fetch_genius_lyrics_from(&endpoints(&server), &client, "Nora Vale", "Intro", "")
                .await
                .unwrap();
        assert_eq!(found, None);
        assert!(server.requests("/").is_empty());
    }

    #[test]
    fn song_pages_stay_on_genius() {
        let mut hit = song(14154298, "Algo Lúcido", "Froid", "Froid");
        hit.url = "https://genius.com/Froid-algo-lucido-lyrics".into();
        assert_eq!(
            song_page(&hit),
            "https://genius.com/Froid-algo-lucido-lyrics"
        );
        hit.url = "javascript:alert(1)".into();
        assert_eq!(song_page(&hit), "https://genius.com/songs/14154298");
        hit.url = String::new();
        assert_eq!(song_page(&hit), "https://genius.com/songs/14154298");
    }

    /// The real Genius, without a token:
    /// `cargo test -p canto-api --lib -- --ignored live_genius_lyrics`.
    /// Prints nothing: lyrics are copyrighted, and none are kept here.
    #[tokio::test]
    #[ignore]
    async fn live_genius_lyrics() {
        let client = reqwest::Client::new();
        let found = fetch_genius_lyrics(&client, "Froid", "Algo Lúcido", "")
            .await
            .expect("Genius answers")
            .expect("Genius has the lyrics");
        assert_eq!(found.song_id, 14154298);
        assert_eq!(found.url, "https://genius.com/Froid-algo-lucido-lyrics");
        assert!(found.lines.len() > 20, "{} lines", found.lines.len());
        // The title is sung in it
        assert!(found
            .lines
            .iter()
            .any(|line| match_key(line).contains("algo lucido")));
        assert!(!found
            .lines
            .iter()
            .any(|line| line.starts_with('[') && line.ends_with(']')));
    }
}
