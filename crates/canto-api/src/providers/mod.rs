//! Music vendor implementations.
//!
//! Each submodule binds one streaming service and produces the provider-neutral
//! types from [`crate::models`].

mod provider;
pub use provider::{MusicProvider, ProviderCapabilities, ProviderKind};

pub mod local;
pub mod spotify;
pub mod subsonic;

use crate::models::{AlbumType, ContentRating, Image, ImageSet, ReleaseDate};

/// The library key ("playlist:ID", "album:ID", "artist:ID", "liked") of a
/// context URI like `spotify:playlist:ID` or `spotify:user:NAME:collection`.
pub fn recent_context_key(uri: &str) -> Option<String> {
    let parts: Vec<&str> = uri.split(':').collect();
    if parts.last() == Some(&"collection") || parts.contains(&"collection") {
        return Some("liked".to_string());
    }
    let kind_at = parts
        .iter()
        .rposition(|p| matches!(*p, "playlist" | "album" | "artist"))?;
    let id = parts.get(kind_at + 1).filter(|id| !id.is_empty())?;
    Some(format!("{}:{}", parts[kind_at], id))
}

#[cfg(test)]
mod recent_tests {
    use super::recent_context_key;

    #[test]
    fn context_uris_become_library_keys() {
        assert_eq!(
            recent_context_key("spotify:playlist:abc").as_deref(),
            Some("playlist:abc")
        );
        assert_eq!(
            recent_context_key("spotify:user:gus:playlist:xyz").as_deref(),
            Some("playlist:xyz")
        );
        assert_eq!(
            recent_context_key("spotify:album:1").as_deref(),
            Some("album:1")
        );
        assert_eq!(
            recent_context_key("spotify:artist:2").as_deref(),
            Some("artist:2")
        );
        assert_eq!(
            recent_context_key("spotify:user:gus:collection").as_deref(),
            Some("liked")
        );
        assert_eq!(recent_context_key("spotify:show:3"), None);
    }
}

/// Build an [`ImageSet`] from `(url, width, height)` tuples.
pub(crate) fn images_to_set<I>(images: I) -> Option<ImageSet>
where
    I: IntoIterator<Item = (String, Option<u32>, Option<u32>)>,
{
    let imgs: Vec<Image> = images
        .into_iter()
        .filter(|(url, _, _)| !url.is_empty())
        .map(|(url, width, height)| Image { url, width, height })
        .collect();
    if imgs.is_empty() {
        None
    } else {
        Some(ImageSet {
            images: imgs,
            template: None,
        })
    }
}

pub(crate) fn content_rating_from_explicit(explicit: Option<bool>) -> ContentRating {
    match explicit {
        Some(true) => ContentRating::Explicit,
        Some(false) => ContentRating::Clean,
        None => ContentRating::None,
    }
}

/// Parse "YYYY", "YYYY-MM", or "YYYY-MM-DD" into a [`ReleaseDate`].
pub(crate) fn parse_release_date(s: &str) -> Option<ReleaseDate> {
    let mut parts = s.split('-');
    let year = parts.next()?.parse::<i32>().ok()?;
    let month = parts.next().and_then(|m| m.parse::<u8>().ok());
    let day = parts.next().and_then(|d| d.parse::<u8>().ok());
    Some(ReleaseDate { year, month, day })
}

pub(crate) fn enum_to_string<T: serde::Serialize>(value: &T) -> Option<String> {
    match serde_json::to_value(value).ok()? {
        serde_json::Value::String(s) => Some(s),
        _ => None,
    }
}

pub(crate) fn album_type_from<T: serde::Serialize>(value: &T) -> Option<AlbumType> {
    enum_to_string(value).map(|s| match s.as_str() {
        "album" => AlbumType::Album,
        "single" => AlbumType::Single,
        "compilation" => AlbumType::Compilation,
        other => AlbumType::Other(other.to_string()),
    })
}

const EP_MIN_TRACKS: u32 = 4;

pub(crate) fn release_type_from<T: serde::Serialize>(value: &T, total_tracks: u32) -> AlbumType {
    match album_type_from(value).unwrap_or(AlbumType::Album) {
        AlbumType::Single if total_tracks >= EP_MIN_TRACKS => AlbumType::Ep,
        other => other,
    }
}
