//! Extraction and cleaning of Genius `/songs/{id}/embed.js` lyrics payloads.

use crate::error::DomainError;
use crate::genius::decode_entities;

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
