//! Track Radio / Similar Tracks builder with Spotify API restriction fallback.
//!
//! Spotify restricted `/v1/recommendations` and `/v1/artists/{id}/related-artists`
//! for many BYOK Client IDs created after Nov 2024. This module tries provider
//! recommendations when permitted and transparently falls back to artist top
//! tracks plus targeted genre/artist catalog search and deduplication.

use std::collections::{HashMap, HashSet};

use crate::genius::normalize_genius_query_title;
use crate::models::{SearchType, Track};
use crate::scheduler::Load;
use crate::service::ApiService;
use crate::DomainError;

pub const DEFAULT_RADIO_TARGET_COUNT: usize = 25;

/// Deduplicate candidates against the seed track and current queue, then mix
/// across artists so similar tracks flow smoothly without artist repetition.
pub fn build_radio_queue(
    seed: &Track,
    candidates: Vec<Track>,
    exclude_ids: &HashSet<String>,
    target_count: usize,
) -> Vec<Track> {
    if target_count == 0 {
        return Vec::new();
    }

    let mut seen_ids = exclude_ids.clone();
    seen_ids.insert(seed.rri.id.clone());

    let mut seen_titles: HashSet<(String, String)> = HashSet::new();
    seen_titles.insert(track_dedup_key(seed));

    let mut by_artist: HashMap<String, Vec<Track>> = HashMap::new();
    let mut artist_order: Vec<String> = Vec::new();

    for track in candidates {
        if !track.playable || track.rri.id.is_empty() {
            continue;
        }
        if !seen_ids.insert(track.rri.id.clone()) {
            continue;
        }
        let title_key = track_dedup_key(&track);
        if !seen_titles.insert(title_key) {
            continue;
        }
        let artist_key = track
            .artists
            .first()
            .map(|a| a.rri.id.clone())
            .filter(|id| !id.is_empty())
            .unwrap_or_else(|| {
                track
                    .artists
                    .first()
                    .map(|a| a.name.to_ascii_lowercase())
                    .unwrap_or_default()
            });

        if !by_artist.contains_key(&artist_key) {
            artist_order.push(artist_key.clone());
        }
        by_artist.entry(artist_key).or_default().push(track);
    }

    // Round-robin across artists so listeners get variety instead of one artist
    // dominating the first 10 songs.
    let mut result = Vec::with_capacity(target_count);
    let mut pass = 0usize;
    loop {
        if result.len() >= target_count {
            break;
        }
        let mut added_any = false;
        for artist_key in &artist_order {
            if result.len() >= target_count {
                break;
            }
            if let Some(bucket) = by_artist.get(artist_key) {
                if let Some(track) = bucket.get(pass) {
                    result.push(track.clone());
                    added_any = true;
                }
            }
        }
        if !added_any {
            break;
        }
        pass += 1;
    }

    result
}

fn track_dedup_key(track: &Track) -> (String, String) {
    let title = normalize_genius_query_title(&track.title).to_ascii_lowercase();
    let artist = track
        .artists
        .first()
        .map(|a| a.name.trim().to_ascii_lowercase())
        .unwrap_or_default();
    (title, artist)
}

/// Build search queries from genres and artist names for the fallback catalog search.
pub fn build_radio_search_queries(seed: &Track, genres: &[String]) -> Vec<String> {
    let mut queries = Vec::new();
    let mut seen = HashSet::new();

    for genre in genres.iter().take(2) {
        let clean = genre.trim();
        if !clean.is_empty() {
            let quoted = format!("genre:\"{clean}\"");
            if seen.insert(quoted.clone()) {
                queries.push(quoted);
            }
        }
    }

    if let Some(primary) = seed.artists.first() {
        let name = primary.name.trim();
        if !name.is_empty() && seen.insert(name.to_string()) {
            queries.push(name.to_string());
        }
    }

    if let Some(first_genre) = genres.first().map(|g| g.trim()).filter(|g| !g.is_empty()) {
        if seen.insert(first_genre.to_string()) {
            queries.push(first_genre.to_string());
        }
    }

    if queries.is_empty() && !seed.title.trim().is_empty() {
        queries.push(normalize_genius_query_title(&seed.title));
    }

    queries
}

/// Fetch similar tracks resiliently using recommendations when permitted and
/// automatic top-tracks + genre/artist search fallback on 403/404 restrictions.
pub async fn fetch_similar_tracks(
    api: &ApiService,
    seed: &Track,
    exclude_ids: &HashSet<String>,
    target_count: usize,
    load: Load,
) -> Result<Vec<Track>, DomainError> {
    let mut pool: Vec<Track> = Vec::new();
    let mut genres: Vec<String> = Vec::new();
    let mut fatal_auth: Option<DomainError> = None;

    // 1. Optional native recommendations endpoint (tolerates 403/404 gracefully).
    if api.provider_capabilities().supports_recommendations && !seed.rri.id.is_empty() {
        match api
            .provider()
            .get_recommendations_for_track(&seed.rri.id, target_count)
            .await
        {
            Ok(tracks) => pool.extend(tracks),
            Err(e @ (DomainError::NoToken | DomainError::AuthExpired)) => fatal_auth = Some(e),
            Err(e) => {
                debug!(
                    "radio: optional recommendations unavailable for {}: {e}",
                    seed.rri.id
                );
            }
        }
    }

    // 2. Fetch artist genres & top tracks for seed + collaborating artists.
    for (idx, artist) in seed.artists.iter().take(3).enumerate() {
        if artist.rri.id.is_empty() {
            continue;
        }
        if idx == 0 {
            match api.get_artist(&artist.rri.id, load).await {
                Ok(full_artist) => genres = full_artist.genres,
                Err(e @ (DomainError::NoToken | DomainError::AuthExpired)) => {
                    fatal_auth = Some(e);
                }
                Err(e) => debug!("radio: failed to load seed artist genres: {e}"),
            }
        }
        match api.get_artist_top_tracks(&artist.rri.id, load).await {
            Ok(top) => pool.extend(top),
            Err(e @ (DomainError::NoToken | DomainError::AuthExpired)) => {
                fatal_auth = Some(e);
            }
            Err(e) => debug!("radio: top tracks failed for {}: {e}", artist.rri.id),
        }

        // Optional related artists -> top tracks if supported & allowed.
        if idx == 0 && pool.len() < target_count {
            if let Ok(related) = api.provider().get_related_artists(&artist.rri.id).await {
                for rel in related.into_iter().take(3) {
                    if let Ok(rel_tracks) = api.get_artist_top_tracks(&rel.rri.id, load).await {
                        pool.extend(rel_tracks.into_iter().take(4));
                    }
                }
            }
        }
    }

    // 3. Targeted catalog searches (genre + artist fallback) whenever pool is small
    //    or to diversify beyond seed artists.
    let queries = build_radio_search_queries(seed, &genres);
    for query in queries {
        if pool.len() >= target_count * 2 {
            break;
        }
        match api
            .search_scoped(&query, SearchType::Track, 0, 20, load)
            .await
        {
            Ok(res) => pool.extend(res.tracks.items),
            Err(e @ (DomainError::NoToken | DomainError::AuthExpired)) => fatal_auth = Some(e),
            Err(e) => debug!("radio: search query '{query}' failed: {e}"),
        }
    }

    let built = build_radio_queue(seed, pool, exclude_ids, target_count);
    if built.is_empty() {
        if let Some(err) = fatal_auth {
            return Err(err);
        }
    }
    Ok(built)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::defaults;
    use crate::models::{ArtistRef, ContentRating, Provider, ResourceId};

    fn make_track(id: &str, title: &str, artist_id: &str, artist_name: &str) -> Track {
        Track {
            rri: ResourceId {
                provider: Provider::Spotify,
                id: id.to_string(),
                uri: Some(format!("spotify:track:{id}")),
            },
            title: title.to_string(),
            artists: vec![ArtistRef {
                rri: ResourceId {
                    provider: Provider::Spotify,
                    id: artist_id.to_string(),
                    uri: None,
                },
                name: artist_name.to_string(),
            }],
            album: None,
            duration_ms: 210_000,
            track_number: None,
            disc_number: None,
            content_rating: ContentRating::None,
            isrc: None,
            art: defaults::image_set_for(defaults::EntityKind::Track),
            playable: true,
            popularity: None,
            saved: None,
            preview_url: None,
            url: None,
        }
    }

    #[test]
    fn deduplicates_seed_queue_unplayable_and_edition_variants() {
        let seed = make_track("seed", "Breathe (In the Air)", "a1", "Pink Floyd");
        let mut unplayable = make_track("u1", "Unplayable", "a2", "Artist 2");
        unplayable.playable = false;

        let candidates = vec![
            make_track("seed", "Breathe (In the Air)", "a1", "Pink Floyd"),
            make_track(
                "dup_title",
                "Breathe (In the Air) - 2011 Remastered",
                "a1",
                "Pink Floyd",
            ),
            make_track("queued", "Already Queued", "a2", "Artist 2"),
            unplayable,
            make_track("t1", "Time", "a1", "Pink Floyd"),
            make_track("t2", "No Quarter", "a2", "Led Zeppelin"),
            make_track("t3", "Starless", "a3", "King Crimson"),
        ];

        let mut exclude = HashSet::new();
        exclude.insert("queued".to_string());

        let radio = build_radio_queue(&seed, candidates, &exclude, 10);
        let ids: Vec<_> = radio.iter().map(|t| t.rri.id.as_str()).collect();
        assert_eq!(ids, ["t1", "t2", "t3"]);
    }

    #[test]
    fn interleaves_artists_round_robin_for_variety() {
        let seed = make_track("seed", "Seed", "a0", "Seed Artist");
        let candidates = vec![
            make_track("a1_1", "A1 1", "a1", "Artist 1"),
            make_track("a1_2", "A1 2", "a1", "Artist 1"),
            make_track("a1_3", "A1 3", "a1", "Artist 1"),
            make_track("a2_1", "A2 1", "a2", "Artist 2"),
            make_track("a3_1", "A3 1", "a3", "Artist 3"),
        ];

        let radio = build_radio_queue(&seed, candidates, &HashSet::new(), 5);
        let ids: Vec<_> = radio.iter().map(|t| t.rri.id.as_str()).collect();
        assert_eq!(ids, ["a1_1", "a2_1", "a3_1", "a1_2", "a1_3"]);
    }

    #[test]
    fn generates_genre_and_artist_fallback_queries() {
        let seed = make_track("seed", "Echoes - 2016 Remaster", "a1", "Pink Floyd");
        let queries = build_radio_search_queries(
            &seed,
            &[
                "progressive rock".into(),
                "psychedelic rock".into(),
                "art rock".into(),
            ],
        );
        assert_eq!(
            queries,
            vec![
                "genre:\"progressive rock\"",
                "genre:\"psychedelic rock\"",
                "Pink Floyd",
                "progressive rock",
            ]
        );
    }
}
