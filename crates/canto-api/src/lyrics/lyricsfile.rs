//! LRCLIB's Lyricsfile: YAML lyrics with the time of every sung word.
//!
//! Only the small part of YAML these files use is read (the `lines` list,
//! plain, single- and double-quoted values). Anything else makes the whole
//! file count as unreadable, and the lyrics fall back to the LRC lines.

use super::model::{LyricLine, LyricWord};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileLine {
    pub start_ms: u64,
    pub end_ms: Option<u64>,
    pub text: String,
    pub words: Vec<FileWord>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FileWord {
    pub start_ms: u64,
    pub text: String,
}

/// The `lines` of a Lyricsfile, or `None` if it can't be read.
pub fn parse(yaml: &str) -> Option<Vec<FileLine>> {
    let mut lines: Vec<(FileLine, bool, Vec<(FileWord, bool)>)> = Vec::new();
    let mut in_lines = false;
    let mut line_indent: Option<usize> = None;
    for raw in yaml.lines() {
        if raw.trim().is_empty() {
            continue;
        }
        let mut indent = raw.len() - raw.trim_start_matches(' ').len();
        let mut row = &raw[indent..];
        if indent == 0 && !row.starts_with("- ") {
            in_lines = row.trim_end() == "lines:";
            continue;
        }
        if !in_lines {
            continue;
        }
        let new_item = row.starts_with("- ");
        if new_item {
            indent += 2;
            row = &row[2..];
        }
        let (key, value) = split_key(row)?;
        let level = *line_indent.get_or_insert(indent);
        if indent == level {
            if new_item {
                lines.push(Default::default());
            }
            let (line, has_start, _) = lines.last_mut()?;
            match key {
                "text" => line.text = scalar(value)?,
                "start_ms" => {
                    line.start_ms = number(value)?;
                    *has_start = true;
                }
                "end_ms" => line.end_ms = optional_number(value)?,
                _ => {}
            }
        } else if indent > level {
            let (_, _, words) = lines.last_mut()?;
            if new_item {
                words.push(Default::default());
            }
            let (word, has_start) = words.last_mut()?;
            match key {
                "text" => word.text = scalar(value)?,
                "start_ms" => {
                    word.start_ms = number(value)?;
                    *has_start = true;
                }
                _ => {}
            }
        } else {
            return None;
        }
    }
    lines
        .into_iter()
        .map(|(mut line, has_start, words)| {
            if !has_start || words.iter().any(|(_, has)| !has) {
                return None;
            }
            line.words = words.into_iter().map(|(w, _)| w).collect();
            Some(line)
        })
        .collect()
}

fn split_key(row: &str) -> Option<(&str, &str)> {
    let colon = row.find(':')?;
    let key = &row[..colon];
    if key.is_empty() || !key.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    let rest = &row[colon + 1..];
    if rest.is_empty() {
        return Some((key, ""));
    }
    rest.strip_prefix(' ').map(|value| (key, value))
}

fn scalar(value: &str) -> Option<String> {
    let value = value.trim();
    if let Some(inner) = value.strip_prefix('\'') {
        let inner = inner.strip_suffix('\'')?;
        return Some(inner.replace("''", "'"));
    }
    if let Some(inner) = value.strip_prefix('"') {
        let inner = inner.strip_suffix('"')?;
        let mut out = String::with_capacity(inner.len());
        let mut chars = inner.chars();
        while let Some(c) = chars.next() {
            match c {
                '\\' => out.push(match chars.next()? {
                    '"' => '"',
                    '\\' => '\\',
                    '/' => '/',
                    ' ' => ' ',
                    'n' => '\n',
                    't' => '\t',
                    _ => return None,
                }),
                '"' => return None,
                c => out.push(c),
            }
        }
        return Some(out);
    }
    // Block scalars, anchors, tags, flow collections: not in these files
    if value.starts_with(['|', '>', '&', '*', '!', '%', '@', '`', '{', '[']) {
        return None;
    }
    Some(value.to_string())
}

fn number(value: &str) -> Option<u64> {
    let text = scalar(value)?;
    text.parse::<u64>().ok().or_else(|| {
        text.parse::<f64>()
            .ok()
            .filter(|n| *n >= 0.0)
            .map(|n| n as u64)
    })
}

fn optional_number(value: &str) -> Option<Option<u64>> {
    match scalar(value)?.as_str() {
        "" | "null" | "~" => Some(None),
        _ => number(value).map(Some),
    }
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Where each word ends in `text`, in bytes, finding them in order; empty
/// if a word isn't there or the times go backwards.
fn words_in(text: &str, words: &[FileWord]) -> Vec<LyricWord> {
    let mut out = Vec::with_capacity(words.len());
    let mut cursor = 0;
    let mut last_start = 0;
    for word in words {
        let piece = word.text.trim();
        if piece.is_empty() {
            continue;
        }
        let Some(found) = text[cursor..].find(piece) else {
            return Vec::new();
        };
        if word.start_ms < last_start {
            return Vec::new();
        }
        cursor += found + piece.len();
        last_start = word.start_ms;
        out.push(LyricWord {
            start_ms: word.start_ms,
            end_byte: cursor,
        });
    }
    out
}

/// Whether an LRC line is only a music note, the mark of a break.
pub fn is_break_mark(text: &str) -> bool {
    let text = text.trim();
    !text.is_empty()
        && text
            .chars()
            .all(|c| matches!(c, '♪' | '♫' | '♬' | '♩') || c.is_whitespace())
}

/// Lyric lines from a Lyricsfile, keeping the break marks of the LRC lines
/// (`♪`) in their places; `None` if no line has word timings.
pub fn to_lines(file: &[FileLine], lrc: &[LyricLine]) -> Option<Vec<LyricLine>> {
    let mut lines: Vec<LyricLine> = file
        .iter()
        .filter_map(|line| {
            let text = collapse_spaces(&line.text);
            if text.is_empty() {
                return None;
            }
            let words = words_in(&text, &line.words);
            Some(LyricLine {
                start_time_ms: line.start_ms,
                end_time_ms: line.end_ms.filter(|end| *end > line.start_ms),
                words,
                text,
            })
        })
        .collect();
    if lines.iter().all(|line| line.words.is_empty()) {
        return None;
    }
    lines.extend(lrc.iter().filter(|l| is_break_mark(&l.text)).cloned());
    lines.sort_by_key(|l| l.start_time_ms);
    Some(lines)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "version: '1.0'
metadata:
  title: Blinding Lights
  artist: The Weeknd
  duration_ms: 200000
lines:
- text: Yeah
  start_ms: 13120
  end_ms: 14551
  words:
  - text: Yeah
    start_ms: 13159
- text: I've been tryna call
  start_ms: 27170
  end_ms: 28268
  words:
  - text: 'I''ve '
    start_ms: 27209
  - text: 'been '
    start_ms: 27598
  - text: \"tryna \"
    start_ms: 27889
  - text: call
    start_ms: 28200
plain: |
  Yeah
  I've been tryna call
";

    #[test]
    fn reads_lines_and_words() {
        let lines = parse(SAMPLE).expect("readable");
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].end_ms, Some(14551));
        assert_eq!(lines[1].text, "I've been tryna call");
        let words: Vec<&str> = lines[1].words.iter().map(|w| w.text.as_str()).collect();
        assert_eq!(words, ["I've ", "been ", "tryna ", "call"]);
        assert_eq!(lines[1].words[3].start_ms, 28200);
    }

    #[test]
    fn indented_lists_read_the_same() {
        let indented = "lines:\n  - text: Oi\n    start_ms: 10\n    words:\n      - text: Oi\n        start_ms: 12\n";
        let lines = parse(indented).expect("readable");
        assert_eq!(lines[0].words[0].start_ms, 12);
    }

    #[test]
    fn unreadable_files_give_none() {
        assert_eq!(parse("lines:\n- text: |\n    Yeah\n  start_ms: 1\n"), None);
        assert_eq!(parse("lines:\n- text: 'open\n  start_ms: 1\n"), None);
        assert_eq!(
            parse("lines:\n- text: Yeah\n"),
            None,
            "a line without start"
        );
        assert_eq!(
            parse("lines:\n- text: a\n  start_ms: 1\n  words:\n  - text: a\n"),
            None
        );
        assert_eq!(
            parse("lines:\n- text: a\n  start_ms: 1\nsomething odd\n"),
            Some(vec![FileLine {
                start_ms: 1,
                text: "a".into(),
                ..Default::default()
            }])
        );
        assert_eq!(
            parse("lines:\n- text: a\n  start_ms: 1\n   continued here\n"),
            None
        );
    }

    #[test]
    fn words_map_to_byte_ends_and_breaks_stay() {
        let file = parse(SAMPLE).unwrap();
        let lrc = vec![
            LyricLine::new(13_100, "Yeah"),
            LyricLine::new(20_000, "♪"),
            LyricLine::new(27_100, "I've been tryna call"),
        ];
        let lines = to_lines(&file, &lrc).expect("has words");
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["Yeah", "♪", "I've been tryna call"]);
        let ends: Vec<usize> = lines[2].words.iter().map(|w| w.end_byte).collect();
        assert_eq!(ends, [4, 9, 15, 20]);
        assert_eq!(lines[2].end_time_ms, Some(28268));
        assert_eq!(lines[2].start_time_ms, 27170);
    }

    #[test]
    fn a_word_not_in_the_line_drops_only_that_lines_words() {
        let file = vec![
            FileLine {
                start_ms: 0,
                text: "um dois".into(),
                words: vec![
                    FileWord {
                        start_ms: 0,
                        text: "um ".into(),
                    },
                    FileWord {
                        start_ms: 300,
                        text: "tres".into(),
                    },
                ],
                ..Default::default()
            },
            FileLine {
                start_ms: 1000,
                text: "quatro".into(),
                words: vec![FileWord {
                    start_ms: 1000,
                    text: "quatro".into(),
                }],
                ..Default::default()
            },
        ];
        let lines = to_lines(&file, &[]).unwrap();
        assert!(lines[0].words.is_empty());
        assert_eq!(lines[1].words.len(), 1);
        // No word timings at all: not worth replacing the LRC lines
        let bare = vec![FileLine {
            start_ms: 0,
            text: "x".into(),
            ..Default::default()
        }];
        assert_eq!(to_lines(&bare, &[]), None);
    }
}
