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
}
