use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub start_time_ms: u64,
    pub text: String,
    /// When the singing of the line ends, if the source says so.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_time_ms: Option<u64>,
    /// Real word timings, if the source has them; empty means guess.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub words: Vec<LyricWord>,
}

/// One sung word (or syllable): when it starts, and where it ends in the
/// line's text, in bytes. Lighting by byte keeps the cut on the word's edge.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricWord {
    pub start_ms: u64,
    pub end_byte: usize,
}

impl LyricLine {
    pub fn new(start_time_ms: u64, text: impl Into<String>) -> Self {
        Self {
            start_time_ms,
            text: text.into(),
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lyrics {
    Synced(Vec<LyricLine>),
    Plain(Vec<String>),
}

/// Roughly how long singing one character takes, for the estimated highlight
/// (measured on 610 lines with real word timings: median 97 ms, so 120 keeps
/// the guess from running ahead of the voice).
const MS_PER_CHAR: u64 = 120;
/// The singing takes at most this share of the time until the next line
/// (median 0.76 in the same lines): the rest is breath or a break.
const SUNG_SHARE_OF_GAP: f64 = 0.8;
/// A guessed line never lights faster than this.
const MIN_LINE_MS: u64 = 400;

/// Where the `n`th whitespace-separated word of `text` ends, in bytes
/// (0 for none, the whole text past the last).
pub fn word_end_byte(text: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut seen = 0;
    let mut in_word = false;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if in_word {
                seen += 1;
                if seen == n {
                    return i;
                }
            }
            in_word = false;
        } else {
            in_word = true;
        }
    }
    text.len()
}

/// How many of the line's words have been reached at `progress` (0 to 1),
/// weighing each word by its length.
pub fn words_reached(text: &str, progress: f64) -> usize {
    let words: Vec<usize> = text.split_whitespace().map(|w| w.chars().count()).collect();
    let total: usize = words.iter().sum();
    if total == 0 {
        return 0;
    }
    let reached = progress.clamp(0.0, 1.0) * total as f64;
    // The first word lights with the line, each next one once it's reached
    let mut start = 0usize;
    let mut count = 0;
    for len in words {
        if count > 0 && start as f64 >= reached {
            break;
        }
        count += 1;
        start += len;
    }
    count
}

impl Lyrics {
    pub fn is_synced(&self) -> bool {
        matches!(self, Self::Synced(_))
    }

    pub fn lines_count(&self) -> usize {
        match self {
            Self::Synced(lines) => lines.len(),
            Self::Plain(lines) => lines.len(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.lines_count() == 0
    }

    /// Finds the index of the active lyric line for a given playback position in milliseconds.
    pub fn active_line_index(&self, position_ms: u64) -> Option<usize> {
        match self {
            Self::Synced(lines) => {
                if lines.is_empty() || position_ms < lines[0].start_time_ms {
                    return None;
                }
                let idx = lines.partition_point(|l| l.start_time_ms <= position_ms);
                if idx > 0 {
                    Some(idx - 1)
                } else {
                    None
                }
            }
            Self::Plain(_) => None,
        }
    }

    /// How far into the active line `position_ms` is, from 0 to 1, for the
    /// estimated word-by-word highlight. The line's sung length is guessed
    /// from its length in characters, capped by the start of the next line,
    /// so a long instrumental break after it doesn't slow the words down.
    pub fn line_progress(&self, position_ms: u64) -> Option<(usize, f64)> {
        let Self::Synced(lines) = self else {
            return None;
        };
        let index = self.active_line_index(position_ms)?;
        let line = &lines[index];
        let length = sung_length(line, lines.get(index + 1).map(|n| n.start_time_ms));
        let elapsed = position_ms.saturating_sub(line.start_time_ms);
        Some((index, (elapsed as f64 / length as f64).min(1.0)))
    }

    /// The active line and how much of its text is sung at `position_ms`,
    /// in bytes: from the real word timings when the lyrics have them,
    /// otherwise guessed from the line's length.
    pub fn karaoke(&self, position_ms: u64) -> Option<(usize, usize)> {
        let Self::Synced(lines) = self else {
            return None;
        };
        let index = self.active_line_index(position_ms)?;
        let line = &lines[index];
        if !line.words.is_empty() {
            let reached = line.words.partition_point(|w| w.start_ms <= position_ms);
            let lit = match reached {
                0 => 0,
                k => line.words[k - 1].end_byte.min(line.text.len()),
            };
            return Some((index, lit));
        }
        let (_, progress) = self.line_progress(position_ms)?;
        let words = words_reached(&line.text, progress);
        Some((index, word_end_byte(&line.text, words)))
    }
}

/// How long the line takes to sing, in ms: the source's end when it has
/// one, otherwise its length at a singing pace, never more than most of the
/// gap to the next line (so a long break doesn't slow the words down).
fn sung_length(line: &LyricLine, next_start: Option<u64>) -> u64 {
    if let Some(end) = line.end_time_ms {
        return end.saturating_sub(line.start_time_ms).max(1);
    }
    let chars = line.text.chars().filter(|c| !c.is_whitespace()).count() as u64;
    let pace = chars * MS_PER_CHAR;
    let length = match next_start {
        Some(next) => {
            let gap = next.saturating_sub(line.start_time_ms);
            let sung = (gap as f64 * SUNG_SHARE_OF_GAP) as u64;
            pace.min(sung).max(MIN_LINE_MS.min(gap))
        }
        None => pace.max(MIN_LINE_MS),
    };
    length.max(1)
}

#[cfg(test)]
mod sung_length_tests {
    use super::*;

    fn line(start: u64, text: &str) -> LyricLine {
        LyricLine::new(start, text)
    }

    #[test]
    fn the_pace_caps_a_line_before_a_long_break() {
        // 17 characters at 120 ms, the next line 10 s away
        let l = line(0, "Is this the real life");
        assert_eq!(sung_length(&l, Some(10_000)), 2_040);
    }

    #[test]
    fn a_short_gap_caps_the_pace() {
        // 28 characters would take 3360 ms; 80% of 1840 is 1472
        let l = line(29_960, "I've been on my own for long enough");
        assert_eq!(sung_length(&l, Some(31_800)), 1_472);
    }

    #[test]
    fn short_lines_have_a_floor_but_never_pass_the_next_line() {
        assert_eq!(sung_length(&line(0, "Oh"), Some(5_000)), 400);
        assert_eq!(sung_length(&line(0, "Oh"), Some(300)), 300);
        assert_eq!(sung_length(&line(0, "Goodbye"), None), 840);
    }

    #[test]
    fn the_source_end_wins() {
        let mut l = line(13_120, "Yeah");
        l.end_time_ms = Some(14_551);
        assert_eq!(sung_length(&l, Some(27_170)), 1_431);
    }

    #[test]
    fn real_word_timings_light_word_by_word() {
        let mut l = line(27_170, "I've been tryna call");
        l.words = [(27_209, 4), (27_598, 9), (27_889, 15), (28_200, 20)]
            .into_iter()
            .map(|(start_ms, end_byte)| LyricWord { start_ms, end_byte })
            .collect();
        let lyrics = Lyrics::Synced(vec![l, line(29_960, "next")]);
        assert_eq!(lyrics.karaoke(27_180), Some((0, 0)));
        assert_eq!(lyrics.karaoke(27_209), Some((0, 4)));
        assert_eq!(lyrics.karaoke(27_597), Some((0, 4)));
        assert_eq!(lyrics.karaoke(27_598), Some((0, 9)));
        assert_eq!(lyrics.karaoke(28_199), Some((0, 15)));
        assert_eq!(lyrics.karaoke(28_268), Some((0, 20)));
        assert_eq!(lyrics.karaoke(100), None);
    }

    #[test]
    fn guessed_words_end_on_a_word_edge() {
        assert_eq!(word_end_byte("um dois tres", 0), 0);
        assert_eq!(word_end_byte("um dois tres", 1), 2);
        assert_eq!(word_end_byte("um  dois tres", 2), 8);
        assert_eq!(word_end_byte("um dois tres", 3), 12);
        assert_eq!(word_end_byte("ação é", 1), "ação".len());
        let lyrics = Lyrics::Synced(vec![line(0, "um dois tres"), line(20_000, "x")]);
        // The first word lights with the line
        assert_eq!(lyrics.karaoke(0), Some((0, 2)));
        assert_eq!(lyrics.karaoke(19_000), Some((0, 12)));
    }
}

#[cfg(test)]
mod karaoke_tests {
    use super::*;

    fn lyrics() -> Lyrics {
        Lyrics::Synced(vec![
            LyricLine {
                start_time_ms: 1000,
                text: "um dois tres".into(),
                ..Default::default()
            },
            LyricLine {
                start_time_ms: 20000,
                text: "quatro".into(),
                ..Default::default()
            },
        ])
    }

    #[test]
    fn the_first_word_lights_with_the_line() {
        assert_eq!(words_reached("um dois tres", 0.0), 1);
        assert_eq!(words_reached("um dois tres", 1.0), 3);
        assert_eq!(words_reached("", 0.5), 0);
    }

    #[test]
    fn words_light_in_order_by_length() {
        // "um" is 2 of 10 characters, "tres" starts at 6
        assert_eq!(words_reached("um dois tres", 0.2), 1);
        assert_eq!(words_reached("um dois tres", 0.3), 2);
        assert_eq!(words_reached("um dois tres", 0.7), 3);
    }

    #[test]
    fn a_long_break_doesnt_slow_the_words() {
        let lyrics = lyrics();
        // 10 characters: 120 * 10 = 1200 ms, though the next line is 19 s away
        let (index, progress) = lyrics.line_progress(1600).unwrap();
        assert_eq!(index, 0);
        assert!((progress - 0.5).abs() < 1e-9);
        assert_eq!(lyrics.line_progress(5000), Some((0, 1.0)));
        assert_eq!(lyrics.line_progress(500), None);
    }
}
