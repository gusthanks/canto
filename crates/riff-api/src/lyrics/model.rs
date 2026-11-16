use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct LyricLine {
    pub start_time_ms: u64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lyrics {
    Synced(Vec<LyricLine>),
    Plain(Vec<String>),
}

/// Roughly how long singing one character takes, for the estimated highlight.
const MS_PER_CHAR: u64 = 70;
/// Added to every line's guessed length.
const LINE_TAIL_MS: u64 = 300;

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
        let chars = line.text.chars().filter(|c| !c.is_whitespace()).count() as u64;
        let guess = chars * MS_PER_CHAR + LINE_TAIL_MS;
        let length = match lines.get(index + 1) {
            Some(next) => guess.min(next.start_time_ms.saturating_sub(line.start_time_ms)),
            None => guess,
        }
        .max(1);
        let elapsed = position_ms.saturating_sub(line.start_time_ms);
        Some((index, (elapsed as f64 / length as f64).min(1.0)))
    }
}

#[cfg(test)]
mod karaoke_tests {
    use super::*;

    fn lyrics() -> Lyrics {
        Lyrics::Synced(vec![
            LyricLine { start_time_ms: 1000, text: "um dois tres".into() },
            LyricLine { start_time_ms: 20000, text: "quatro".into() },
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
        // 10 characters: 70 * 10 + 300 = 1000 ms, though the next line is 19 s away
        let (index, progress) = lyrics.line_progress(1500).unwrap();
        assert_eq!(index, 0);
        assert!((progress - 0.5).abs() < 1e-9);
        assert_eq!(lyrics.line_progress(5000), Some((0, 1.0)));
        assert_eq!(lyrics.line_progress(500), None);
    }
}
