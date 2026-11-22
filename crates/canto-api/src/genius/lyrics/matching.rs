//! Search result normalization and strict track-to-Genius matching.

use serde_json::Value;

use super::GeniusSongHit;
use crate::genius::normalize_genius_query_title;

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
pub(super) fn only_sections(key: &str) -> bool {
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
pub(super) fn match_key(text: &str) -> String {
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
