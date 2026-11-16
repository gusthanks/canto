//! Lyrics translation through Google Translate, cached on disk per track and
//! language so a song is translated once.
//!
//! Without a key it uses the keyless endpoint browsers' translate features
//! use: free and instant, but unofficial, so it may stop working or throttle.
//! With a Google Cloud API key it uses the official Cloud Translation API
//! (500,000 characters a month free, a billing account required).

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The keyless endpoint Chrome's translate extension uses: one answer per
/// line sent, with the language it detected.
const KEYLESS_URL: &str = "https://clients5.google.com/translate_a/t";
const CLOUD_URL: &str = "https://translation.googleapis.com/language/translate/v2";
/// The keyless endpoint rejects long texts; lyrics are cut into chunks.
const MAX_CHUNK_CHARS: usize = 4000;

/// A translated song: one entry per lyric line (`None` for blank lines or
/// lines left as they were).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Translation {
    /// The lyrics are already in the target language: nothing to show.
    pub same_language: bool,
    pub lines: Vec<Option<String>>,
}

#[derive(Clone, Debug)]
pub struct LyricsTranslator {
    client: reqwest::Client,
    root: PathBuf,
}

impl LyricsTranslator {
    pub fn new(client: reqwest::Client) -> Self {
        Self {
            client,
            root: glib::user_cache_dir().join("canto").join("translations"),
        }
    }

    fn cache_path(&self, key: &str, target: &str) -> PathBuf {
        let safe = |s: &str| -> String {
            s.chars()
                .map(|c| if c.is_ascii_alphanumeric() || c == '-' { c } else { '_' })
                .collect()
        };
        let key = key.trim().strip_prefix("spotify:track:").unwrap_or(key.trim());
        self.root.join(format!("{}.{}.json", safe(key), safe(target)))
    }

    pub fn cached(&self, key: &str, target: &str) -> Option<Translation> {
        let text = fs::read_to_string(self.cache_path(key, target)).ok()?;
        serde_json::from_str(&text).ok()
    }

    fn store(&self, key: &str, target: &str, translation: &Translation) {
        if fs::create_dir_all(&self.root).is_ok() {
            if let Ok(text) = serde_json::to_string(translation) {
                let _ = fs::write(self.cache_path(key, target), text);
            }
        }
    }

    /// Translates `lines` into `target` (a code like "pt" or "en"), from the
    /// cache when it's there.
    pub async fn translate(
        &self,
        key: &str,
        lines: &[String],
        target: &str,
        api_key: Option<&str>,
    ) -> Result<Translation, String> {
        if let Some(cached) = self.cached(key, target) {
            if cached.lines.len() == lines.len() {
                return Ok(cached);
            }
        }
        // Only lines with words are sent; the rest come back as `None`
        let sent: Vec<(usize, &str)> = lines
            .iter()
            .enumerate()
            .map(|(i, l)| (i, l.trim()))
            .filter(|(_, l)| !l.is_empty())
            .collect();
        let texts: Vec<&str> = sent.iter().map(|(_, l)| *l).collect();
        let (translated, detected) = match api_key.filter(|k| !k.trim().is_empty()) {
            Some(key) => self.cloud(&texts, target, key.trim()).await?,
            None => self.keyless(&texts, target).await?,
        };
        let same_language = detected.as_deref().is_some_and(|d| same_language(d, target));
        let mut out = vec![None; lines.len()];
        for ((index, original), translation) in sent.iter().zip(translated) {
            let translation = translation.trim();
            if !translation.is_empty() && translation != *original {
                out[*index] = Some(translation.to_string());
            }
        }
        let translation = Translation {
            same_language,
            lines: out,
        };
        self.store(key, target, &translation);
        Ok(translation)
    }

    async fn keyless(
        &self,
        texts: &[&str],
        target: &str,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let mut translated = Vec::with_capacity(texts.len());
        let mut languages = Vec::new();
        for chunk in chunks(texts, MAX_CHUNK_CHARS) {
            let form: Vec<(&str, &str)> = chunk.iter().map(|line| ("q", *line)).collect();
            let response = self
                .client
                .post(KEYLESS_URL)
                .query(&[("client", "dict-chrome-ex"), ("sl", "auto"), ("tl", target)])
                .form(&form)
                .send()
                .await
                .map_err(|e| e.to_string())?;
            if !response.status().is_success() {
                return Err(format!("Google Translate answered {}", response.status()));
            }
            let body = response.text().await.map_err(|e| e.to_string())?;
            let (lines, chunk_languages) = parse_keyless(&body, chunk.len())?;
            translated.extend(lines);
            languages.extend(chunk_languages);
        }
        Ok((translated, most_common(languages)))
    }

    async fn cloud(
        &self,
        texts: &[&str],
        target: &str,
        api_key: &str,
    ) -> Result<(Vec<String>, Option<String>), String> {
        let body = serde_json::json!({ "q": texts, "target": target, "format": "text" });
        let response = self
            .client
            .post(CLOUD_URL)
            .query(&[("key", api_key)])
            .header(reqwest::header::CONTENT_TYPE, "application/json")
            .body(body.to_string())
            .send()
            .await
            .map_err(|e| e.to_string())?;
        let status = response.status();
        let text = response.text().await.map_err(|e| e.to_string())?;
        if !status.is_success() {
            return Err(format!("Google Cloud Translation answered {status}"));
        }
        parse_cloud(&text, texts.len())
    }
}

/// Lines grouped so each group's text stays under `max` characters.
fn chunks<'a>(texts: &[&'a str], max: usize) -> Vec<Vec<&'a str>> {
    let mut groups: Vec<Vec<&str>> = Vec::new();
    let mut size = 0;
    for text in texts {
        let len = text.chars().count() + 1;
        if groups.is_empty() || size + len > max {
            groups.push(Vec::new());
            size = 0;
        }
        groups.last_mut().unwrap().push(text);
        size += len;
    }
    groups
}

/// The keyless answer: `[["translated", "en"], ...]`, one per line sent
/// (plain strings when the source language was given).
fn parse_keyless(body: &str, expected: usize) -> Result<(Vec<String>, Vec<String>), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let items = value.as_array().ok_or("unexpected answer from Google Translate")?;
    if items.len() != expected {
        return Err(format!("{} translations for {expected} lines", items.len()));
    }
    let mut lines = Vec::with_capacity(items.len());
    let mut languages = Vec::new();
    for item in items {
        match item {
            serde_json::Value::String(text) => lines.push(text.clone()),
            serde_json::Value::Array(pair) => {
                lines.push(pair.first().and_then(|t| t.as_str()).unwrap_or("").to_string());
                if let Some(language) = pair.get(1).and_then(|l| l.as_str()) {
                    languages.push(language.to_string());
                }
            }
            _ => return Err("unexpected answer from Google Translate".to_string()),
        }
    }
    Ok((lines, languages))
}

/// The language most lines were detected in.
fn most_common(languages: Vec<String>) -> Option<String> {
    let mut counts: std::collections::HashMap<String, usize> = Default::default();
    for language in languages {
        *counts.entry(language).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(_, n)| *n).map(|(l, _)| l)
}

/// The official answer: `{"data": {"translations": [{"translatedText", "detectedSourceLanguage"}]}}`.
fn parse_cloud(body: &str, expected: usize) -> Result<(Vec<String>, Option<String>), String> {
    let value: serde_json::Value = serde_json::from_str(body).map_err(|e| e.to_string())?;
    let items = value
        .pointer("/data/translations")
        .and_then(|v| v.as_array())
        .ok_or("unexpected answer from Google Cloud Translation")?;
    if items.len() != expected {
        return Err(format!("{} translations for {expected} lines", items.len()));
    }
    let lines = items
        .iter()
        .map(|item| {
            item.get("translatedText")
                .and_then(|t| t.as_str())
                .map(decode_entities)
                .unwrap_or_default()
        })
        .collect();
    let detected = items
        .first()
        .and_then(|item| item.get("detectedSourceLanguage"))
        .and_then(|l| l.as_str())
        .map(str::to_owned);
    Ok((lines, detected))
}

/// The few HTML entities the official API can still return.
fn decode_entities(text: &str) -> String {
    text.replace("&#39;", "'")
        .replace("&quot;", "\"")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&")
}

/// "pt-BR" and "pt" are the same language here.
fn same_language(a: &str, b: &str) -> bool {
    let base = |s: &str| s.split(['-', '_']).next().unwrap_or("").to_ascii_lowercase();
    base(a) == base(b)
}

/// The translation target for an interface locale like "pt_BR.UTF-8".
pub fn target_for_locale(locale: &str) -> String {
    let base = locale
        .split(['.', '@'])
        .next()
        .unwrap_or("")
        .split(['_', '-'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    if base.is_empty() || base == "c" || base == "posix" {
        "en".to_string()
    } else {
        base
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keyless_answer_has_one_translation_per_line() {
        let body = r#"[["Eu estou apaixonado","en"],["Nós empurramos e puxamos","en"],["Tom e Jerry","fr"]]"#;
        let (lines, languages) = parse_keyless(body, 3).unwrap();
        assert_eq!(lines, ["Eu estou apaixonado", "Nós empurramos e puxamos", "Tom e Jerry"]);
        assert_eq!(most_common(languages).as_deref(), Some("en"));
        let (plain, none) = parse_keyless(r#"["Olá","Boa noite"]"#, 2).unwrap();
        assert_eq!(plain, ["Olá", "Boa noite"]);
        assert!(none.is_empty());
        assert!(parse_keyless(body, 2).is_err());
        assert!(parse_keyless("<html>Sorry</html>", 1).is_err());
    }

    /// The real keyless endpoint: `cargo test -p riff-api -- --ignored live_keyless`
    #[tokio::test]
    #[ignore]
    async fn live_keyless() {
        let translator = LyricsTranslator::new(reqwest::Client::new());
        let result = translator
            .keyless(&["I'm in love with the shape of you", "Good night"], "pt")
            .await;
        println!("{result:?}");
        assert!(result.is_ok());
    }

    #[test]
    fn cloud_answer_keeps_order_and_decodes_entities() {
        let body = r#"{"data":{"translations":[
            {"translatedText":"I&#39;m here","detectedSourceLanguage":"pt"},
            {"translatedText":"Tom &amp; Jerry","detectedSourceLanguage":"pt"}]}}"#;
        let (lines, detected) = parse_cloud(body, 2).unwrap();
        assert_eq!(lines, ["I'm here", "Tom & Jerry"]);
        assert_eq!(detected.as_deref(), Some("pt"));
        assert!(parse_cloud(body, 1).is_err());
    }

    #[test]
    fn long_lyrics_are_cut_into_chunks() {
        let texts = ["aaaa", "bbbb", "cccc"];
        let groups = chunks(&texts, 10);
        assert_eq!(groups, vec![vec!["aaaa", "bbbb"], vec!["cccc"]]);
        assert_eq!(chunks(&[], 10).len(), 0);
    }

    #[test]
    fn languages_and_locales() {
        assert!(same_language("pt-BR", "pt"));
        assert!(!same_language("en", "pt"));
        assert_eq!(target_for_locale("pt_BR.UTF-8"), "pt");
        assert_eq!(target_for_locale("en_US"), "en");
        assert_eq!(target_for_locale("C"), "en");
        assert_eq!(target_for_locale(""), "en");
    }
}
