use super::model::{LyricLine, Lyrics};

/// Parse a time tag content such as "01:23.45", "00:05.06", "02:15.500", or "01:23" into milliseconds.
pub fn parse_timestamp(tag: &str) -> Option<u64> {
    let mut parts = tag.split(':');
    let min_str = parts.next()?;
    let sec_str = parts.next()?;
    if parts.next().is_some() {
        return None;
    }

    let minutes = min_str.trim().parse::<u64>().ok()?;

    let (seconds, frac_ms) = if let Some((sec, frac)) = sec_str.trim().split_once('.') {
        let s = sec.parse::<u64>().ok()?;
        let f = match frac.len() {
            0 => 0,
            1 => frac.parse::<u64>().ok()? * 100,
            2 => frac.parse::<u64>().ok()? * 10,
            _ => frac[..3].parse::<u64>().ok()?,
        };
        (s, f)
    } else {
        (sec_str.trim().parse::<u64>().ok()?, 0)
    };

    Some((minutes * 60 + seconds) * 1000 + frac_ms)
}

/// Parse LRC formatted content into a list of timestamped `LyricLine`s.
pub fn parse_lrc(input: &str) -> Vec<LyricLine> {
    let mut lines = Vec::new();

    for raw_line in input.lines() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }

        let mut timestamps = Vec::new();
        let mut rest = line;

        while rest.starts_with('[') {
            if let Some(close_bracket) = rest.find(']') {
                let tag = &rest[1..close_bracket];
                if let Some(ms) = parse_timestamp(tag) {
                    timestamps.push(ms);
                    rest = rest[close_bracket + 1..].trim_start();
                } else {
                    let lower = tag.to_ascii_lowercase();
                    if lower.starts_with("ar:")
                        || lower.starts_with("ti:")
                        || lower.starts_with("al:")
                        || lower.starts_with("by:")
                        || lower.starts_with("offset:")
                        || lower.starts_with("length:")
                    {
                        rest = rest[close_bracket + 1..].trim_start();
                    } else {
                        break;
                    }
                }
            } else {
                break;
            }
        }

        let text = rest.trim().to_string();
        for ts in timestamps {
            lines.push(LyricLine {
                start_time_ms: ts,
                text: text.clone(),
            });
        }
    }

    lines.sort_by_key(|l| l.start_time_ms);
    lines
}

/// Fallback aware lyrics parser that takes optional synced LRC and plain lyrics strings.
pub fn parse_lyrics(synced: Option<&str>, plain: Option<&str>) -> Option<Lyrics> {
    if let Some(synced_str) = synced {
        let parsed = parse_lrc(synced_str);
        if !parsed.is_empty() {
            return Some(Lyrics::Synced(parsed));
        }
    }

    if let Some(plain_str) = plain {
        let lines: Vec<String> = plain_str
            .lines()
            .map(|l| l.trim().to_string())
            .filter(|l| !l.is_empty())
            .collect();
        if !lines.is_empty() {
            return Some(Lyrics::Plain(lines));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_timestamp_standard() {
        assert_eq!(parse_timestamp("00:05.06"), Some(5060));
        assert_eq!(parse_timestamp("01:07.87"), Some(67870));
        assert_eq!(parse_timestamp("00:00.00"), Some(0));
    }

    #[test]
    fn test_parse_timestamp_three_digits() {
        assert_eq!(parse_timestamp("02:15.500"), Some(135500));
        assert_eq!(parse_timestamp("00:04.620"), Some(4620));
    }

    #[test]
    fn test_parse_timestamp_one_digit() {
        assert_eq!(parse_timestamp("01:20.5"), Some(80500));
    }

    #[test]
    fn test_parse_timestamp_no_fraction() {
        assert_eq!(parse_timestamp("02:15"), Some(135000));
    }

    #[test]
    fn test_parse_timestamp_invalid() {
        assert_eq!(parse_timestamp(""), None);
        assert_eq!(parse_timestamp("invalid"), None);
        assert_eq!(parse_timestamp("ar: The Beatles"), None);
        assert_eq!(parse_timestamp("12:34:56"), None);
    }

    #[test]
    fn test_parse_lrc_basic() {
        let lrc = r#"
[00:05.06] Yesterday, all my troubles seemed so far away
[00:13.53] Now it looks as though they're here to stay
[00:17.24] Oh, I believe in yesterday
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].start_time_ms, 5060);
        assert_eq!(lines[0].text, "Yesterday, all my troubles seemed so far away");
        assert_eq!(lines[1].start_time_ms, 13530);
        assert_eq!(lines[1].text, "Now it looks as though they're here to stay");
        assert_eq!(lines[2].start_time_ms, 17240);
        assert_eq!(lines[2].text, "Oh, I believe in yesterday");
    }

    #[test]
    fn test_parse_lrc_with_metadata_tags() {
        let lrc = r#"
[ar: The Beatles]
[ti: Yesterday]
[al: Help!]
[length: 02:06]
[00:05.06] Yesterday, all my troubles seemed so far away
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 1);
        assert_eq!(lines[0].start_time_ms, 5060);
        assert_eq!(lines[0].text, "Yesterday, all my troubles seemed so far away");
    }

    #[test]
    fn test_parse_lrc_multiple_timestamps() {
        let lrc = "[00:05.00][00:15.00] Chorus line";
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].start_time_ms, 5000);
        assert_eq!(lines[0].text, "Chorus line");
        assert_eq!(lines[1].start_time_ms, 15000);
        assert_eq!(lines[1].text, "Chorus line");
    }

    #[test]
    fn test_parse_lrc_sorting() {
        let lrc = r#"
[00:20.00] Line two
[00:10.00] Line one
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].start_time_ms, 10000);
        assert_eq!(lines[0].text, "Line one");
        assert_eq!(lines[1].start_time_ms, 20000);
        assert_eq!(lines[1].text, "Line two");
    }

    #[test]
    fn test_parse_lyrics_synced_fallback_to_plain() {
        let plain = "Line 1\nLine 2\n\nLine 3";
        let lyrics = parse_lyrics(None, Some(plain)).expect("lyrics");
        match lyrics {
            Lyrics::Plain(lines) => {
                assert_eq!(lines, vec!["Line 1", "Line 2", "Line 3"]);
            }
            Lyrics::Synced(_) => panic!("Expected plain lyrics"),
        }
    }

    #[test]
    fn test_parse_timestamp_long_fraction() {
        assert_eq!(parse_timestamp("01:23.45678"), Some(83456));
    }

    #[test]
    fn test_parse_lrc_unicode() {
        let lrc = r#"
[00:01.00] こんにちは世界
[00:03.50] Canção em português com acentuação
[00:06.00] 🎵 Music notes and emoji ✨
"#;
        let lines = parse_lrc(lrc);
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].text, "こんにちは世界");
        assert_eq!(lines[1].text, "Canção em português com acentuação");
        assert_eq!(lines[2].text, "🎵 Music notes and emoji ✨");
    }

    #[test]
    fn test_parse_lrc_empty_and_whitespace() {
        assert_eq!(parse_lrc(""), vec![]);
        assert_eq!(parse_lrc("   \n\t  \n  "), vec![]);
    }

    #[test]
    fn test_parse_lyrics_all_none_or_empty() {
        assert_eq!(parse_lyrics(None, None), None);
        assert_eq!(parse_lyrics(Some(""), Some("")), None);
        assert_eq!(parse_lyrics(Some("   "), Some("   \n  ")), None);
    }

    #[test]
    fn test_active_line_index() {
        let lrc = r#"
[00:05.00] Line 1
[00:10.00] Line 2
[00:20.00] Line 3
"#;
        let lyrics = parse_lyrics(Some(lrc), None).expect("lyrics");
        assert_eq!(lyrics.active_line_index(0), None);
        assert_eq!(lyrics.active_line_index(4999), None);
        assert_eq!(lyrics.active_line_index(5000), Some(0));
        assert_eq!(lyrics.active_line_index(8000), Some(0));
        assert_eq!(lyrics.active_line_index(10000), Some(1));
        assert_eq!(lyrics.active_line_index(15000), Some(1));
        assert_eq!(lyrics.active_line_index(20000), Some(2));
        assert_eq!(lyrics.active_line_index(30000), Some(2));
    }
}
