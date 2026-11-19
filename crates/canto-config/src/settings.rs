use gio::prelude::SettingsExt;
use log::error;
use std::collections::HashMap;

pub const SETTINGS: &str = "io.github.gusthanks.Canto";

/// Canto's app id before it had its own (inherited from Riff).
pub const OLD_SETTINGS: &str = "dev.diegovsky.Riff";
pub const MIGRATED_FROM_RIFF: &str = "migrated-from-riff";

/// Copies what the user changed under the old app id to the new one, once.
/// The old schema is only there on machines that ran the old build; without
/// it there is nothing to copy. Returns how many keys came over.
pub fn migrate_from_old_app_id() -> usize {
    use gio::prelude::*;
    let Some(source) = gio::SettingsSchemaSource::default() else {
        return 0;
    };
    let Some(new_schema) = source.lookup(SETTINGS, true) else {
        return 0;
    };
    let settings = gio::Settings::new(SETTINGS);
    if settings.boolean(MIGRATED_FROM_RIFF) {
        return 0;
    }
    let mut copied = 0;
    if let Some(old_schema) = source.lookup(OLD_SETTINGS, true) {
        let old = gio::Settings::new_full(&old_schema, None::<&gio::SettingsBackend>, None);
        settings.delay();
        for key in old_schema.list_keys() {
            if !new_schema.has_key(&key) || key.as_str() == MIGRATED_FROM_RIFF {
                continue;
            }
            if let Some(value) = old.user_value(&key) {
                if settings.set_value(&key, &value).is_ok() {
                    copied += 1;
                }
            }
        }
        // The flag goes in the same batch: once delay() is on, writes made
        // after apply() stay pending and are never saved.
        let _ = settings.set_boolean(MIGRATED_FROM_RIFF, true);
        settings.apply();
    } else {
        let _ = settings.set_boolean(MIGRATED_FROM_RIFF, true);
    }
    gio::Settings::sync();
    copied
}
pub const QUEUE_SIDE_PANEL: &str = "queue-side-panel";
pub const APP_LANGUAGE: &str = "app-language";
pub const APP_THEME: &str = "app-theme";
pub const ADAPTIVE_COVER_COLORS: &str = "adaptive-cover-colors";
pub const LYRICS_OFFSET_MS: &str = "lyrics-offset-ms";
/// "word" lights the words as they are sung, "line" the whole line at once.
pub const LYRICS_HIGHLIGHT: &str = "lyrics-highlight";
/// "follow" goes back to the sung line after a scroll by hand, "free" stays.
pub const LYRICS_SCROLL: &str = "lyrics-scroll";
pub const NAVIGATION_RAIL: &str = "navigation-rail";
/// Player bar buttons the user can hide: (settings key, button)
pub const PLAYER_BAR_LYRICS: &str = "player-bar-lyrics";
pub const PLAYER_BAR_QUEUE: &str = "player-bar-queue";
pub const PLAYER_BAR_DEVICE: &str = "player-bar-device";
pub const PLAYER_BAR_VOLUME: &str = "player-bar-volume";
pub const PLAYER_BAR_MINI_PLAYER: &str = "player-bar-mini-player";
pub const PLAYER_BAR_SIDE_PANEL: &str = "player-bar-side-panel";
/// Order of the buttons at the end of the player bar, by id
pub const PLAYER_BAR_ORDER: &str = "player-bar-order";
pub const PLAYER_BAR_DEFAULT_ORDER: [&str; 7] = [
    "lyrics",
    "queue",
    "device",
    "volume",
    "mini-player",
    "more",
    "side-panel",
];

/// The saved order, cleaned: unknown ids dropped, missing ones added where
/// the default order has them, each id once.
pub fn player_bar_order(saved: &[String]) -> Vec<&'static str> {
    let mut order: Vec<&'static str> = Vec::new();
    for id in saved {
        if let Some(known) = PLAYER_BAR_DEFAULT_ORDER.iter().find(|d| **d == id.as_str()) {
            if !order.contains(known) {
                order.push(known);
            }
        }
    }
    for (index, id) in PLAYER_BAR_DEFAULT_ORDER.iter().enumerate() {
        if !order.contains(id) {
            // Right after the id that comes before it by default
            let at = PLAYER_BAR_DEFAULT_ORDER[..index]
                .iter()
                .rev()
                .find_map(|prev| order.iter().position(|o| o == prev))
                .map_or(0, |p| p + 1);
            order.insert(at, id);
        }
    }
    order
}

#[cfg(test)]
mod player_bar_order_tests {
    use super::*;

    fn owned(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn keeps_a_full_saved_order() {
        let saved = owned(&[
            "more",
            "side-panel",
            "lyrics",
            "queue",
            "device",
            "volume",
            "mini-player",
        ]);
        assert_eq!(
            player_bar_order(&saved),
            vec![
                "more",
                "side-panel",
                "lyrics",
                "queue",
                "device",
                "volume",
                "mini-player"
            ]
        );
    }

    #[test]
    fn drops_unknown_and_repeated_ids_and_adds_missing_ones() {
        let saved = owned(&["volume", "bogus", "volume", "lyrics"]);
        assert_eq!(
            player_bar_order(&saved),
            // A missing button comes back next to its neighbor in the default order
            vec![
                "volume",
                "mini-player",
                "more",
                "side-panel",
                "lyrics",
                "queue",
                "device"
            ]
        );
        assert_eq!(player_bar_order(&[]), PLAYER_BAR_DEFAULT_ORDER.to_vec());
    }
}
pub const LYRICS_TRANSLATION: &str = "lyrics-translation";
pub const LIBRARY_SORT: &str = "library-sort";
pub const LIBRARY_VIEW: &str = "library-view";
/// The track open when Canto closed: id, context URI, position and when.
pub const LAST_PLAYED: &str = "last-played";
pub const LYRICS_TRANSLATION_LANGUAGE: &str = "lyrics-translation-language";
pub const GOOGLE_TRANSLATE_API_KEY: &str = "google-translate-api-key";
pub const GENIUS_ACCESS_TOKEN: &str = "genius-access-token";
pub const SCROBBLE_ENABLED: &str = "scrobble-enabled";
pub const SCROBBLE_RESPECT_REMOTE: &str = "scrobble-respect-remote";
pub const LASTFM_API_KEY: &str = "lastfm-api-key";
pub const SOLOIST_ENABLED: &str = "soloist-enabled";
pub const LASTFM_API_SECRET: &str = "lastfm-api-secret";
pub const LASTFM_SESSION_KEY: &str = "lastfm-session-key";
/// Last.fm refused the saved connection (revoked in its settings, or the
/// API key changed); cleared when it accepts a scrobble or on reconnecting.
pub const LASTFM_SESSION_REJECTED: &str = "lastfm-session-rejected";
pub const LASTFM_USERNAME: &str = "lastfm-username";
/// Liking a song in Canto loves it on Last.fm (and unliking unloves it).
pub const LASTFM_LOVE_ON_LIKE: &str = "lastfm-love-on-like";
pub const LISTENBRAINZ_TOKEN: &str = "listenbrainz-token";
pub const SMART_QUEUE_CONTINUATION: &str = "smart-queue-continuation";
pub const VISUAL_BRIDGE_ENABLED: &str = "visual-bridge-enabled";

/// Keys whose changes only apply after restarting the app.
pub const RESTART_REQUIRED_KEYS: &[&str] = &[
    // General
    APP_LANGUAGE,
    // Audio
    "audio-backend",
    "alsa-device",
    "player-bitrate",
    "audio-format",
    "gapless-playback",
    "volume-curve",
    // Normalisation
    "normalisation",
    "normalisation-type",
    "normalisation-method",
    "normalisation-pregain-db",
    "normalisation-threshold-dbfs",
    "normalisation-attack-ms",
    "normalisation-release-ms",
    "normalisation-knee-db",
    // Network
    "ap-port",
    // Caching
    "audio-cache-size-mb",
];

/// GSettings key holding the per-user pinned-object map.
const PINNED_OBJECTS_KEY: &str = "pinned-objects-by-user";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PinnedKind {
    Playlist,
    Album,
    Artist,
    Track,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct PinnedObject {
    pub id: String,
    pub kind: PinnedKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
}

impl PinnedObject {
    pub fn new(id: impl Into<String>, kind: PinnedKind) -> Self {
        Self {
            id: id.into(),
            kind,
            title: None,
        }
    }
}

type PinnedObjectsByUser = HashMap<String, Vec<PinnedObject>>;

fn load_pinned_map(settings: &gio::Settings) -> PinnedObjectsByUser {
    let json = settings.string(PINNED_OBJECTS_KEY);
    if json.is_empty() {
        return HashMap::new();
    }
    serde_json::from_str(&json).unwrap_or_else(|e| {
        error!("Failed to parse pinned objects: {e}");
        HashMap::new()
    })
}

fn save_pinned_map(settings: &gio::Settings, map: &PinnedObjectsByUser) -> bool {
    match serde_json::to_string(map) {
        Ok(json) => settings
            .set_string(PINNED_OBJECTS_KEY, &json)
            .map_err(|e| error!("Failed to save pinned objects: {e}"))
            .is_ok(),
        Err(e) => {
            error!("Failed to serialize pinned objects: {e}");
            false
        }
    }
}

fn pin_in_map(map: &mut PinnedObjectsByUser, user_id: &str, object: &PinnedObject) -> bool {
    let objects = map.entry(user_id.to_string()).or_default();
    if objects
        .iter()
        .any(|o| o.id == object.id && o.kind == object.kind)
    {
        return false;
    }
    objects.push(object.clone());
    true
}

fn unpin_from_map(
    map: &mut PinnedObjectsByUser,
    user_id: &str,
    id: &str,
    kind: PinnedKind,
) -> bool {
    let Some(objects) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = objects.len();
    objects.retain(|o| o.id != id || o.kind != kind);
    let changed = objects.len() != len_before;
    if objects.is_empty() {
        map.remove(user_id);
    }
    changed
}

fn prune_user_pins(
    map: &mut PinnedObjectsByUser,
    user_id: &str,
    kind: PinnedKind,
    valid_ids: &[String],
) -> bool {
    let Some(objects) = map.get_mut(user_id) else {
        return false;
    };
    let len_before = objects.len();
    objects.retain(|o| o.kind != kind || valid_ids.iter().any(|valid| valid == &o.id));
    let changed = objects.len() != len_before;
    if objects.is_empty() {
        map.remove(user_id);
    }
    changed
}

// Generic pinned-object API (any kind).

pub fn get_pinned_objects(user_id: &str) -> Vec<PinnedObject> {
    load_pinned_map(&gio::Settings::new(SETTINGS))
        .get(user_id)
        .cloned()
        .unwrap_or_default()
}

pub fn is_object_pinned(user_id: &str, id: &str, kind: PinnedKind) -> bool {
    get_pinned_objects(user_id)
        .iter()
        .any(|o| o.id == id && o.kind == kind)
}

pub fn pin_object(user_id: &str, kind: PinnedKind, id: &str, title: Option<String>) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    let object = PinnedObject {
        title,
        ..PinnedObject::new(id, kind)
    };
    if !pin_in_map(&mut map, user_id, &object) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn unpin_object(user_id: &str, kind: PinnedKind, id: &str) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !unpin_from_map(&mut map, user_id, id, kind) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn prune_pinned_objects(user_id: &str, kind: PinnedKind, valid_ids: &[String]) -> bool {
    let settings = gio::Settings::new(SETTINGS);
    let mut map = load_pinned_map(&settings);
    if !prune_user_pins(&mut map, user_id, kind, valid_ids) {
        return false;
    }
    save_pinned_map(&settings, &map)
}

pub fn drm_verified_user() -> String {
    gio::Settings::new(SETTINGS)
        .string("drm-verified-user")
        .to_string()
}

pub fn set_drm_verified_user(user_id: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", user_id);
}

#[cfg(debug_assertions)]
pub fn clear_drm_verified_user() {
    let _ = gio::Settings::new(SETTINGS).set_string("drm-verified-user", "");
}

pub fn audio_cache_size_limit_bytes() -> u64 {
    let mb = gio::Settings::new(SETTINGS).uint("audio-cache-size-mb") as u64;
    mb * 1024 * 1024
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CloseWindowBehavior {
    #[default]
    Ask,
    MinimizeToBackground,
    StopAndQuit,
}

impl CloseWindowBehavior {
    pub fn from_gsettings_enum(value: i32) -> Self {
        match value {
            1 => Self::MinimizeToBackground,
            2 => Self::StopAndQuit,
            _ => Self::Ask,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct WindowGeometry {
    pub width: i32,
    pub height: i32,
    pub is_maximized: bool,
}

impl WindowGeometry {
    pub fn new_from_gsettings() -> Self {
        let settings = gio::Settings::new(SETTINGS);
        Self {
            width: settings.int("window-width"),
            height: settings.int("window-height"),
            is_maximized: settings.boolean("window-is-maximized"),
        }
    }

    pub fn save(&self) -> Option<()> {
        let settings = gio::Settings::new(SETTINGS);
        settings.delay();
        settings.set_int("window-width", self.width).ok()?;
        settings.set_int("window-height", self.height).ok()?;
        settings
            .set_boolean("window-is-maximized", self.is_maximized)
            .ok()?;
        settings.apply();
        Some(())
    }
}

pub fn card_layout() -> String {
    gio::Settings::new(SETTINGS)
        .string("card-layout")
        .to_string()
}

pub fn set_card_layout(layout: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("card-layout", layout);
}

pub fn card_size() -> String {
    gio::Settings::new(SETTINGS).string("card-size").to_string()
}

pub fn set_card_size(size: &str) {
    let _ = gio::Settings::new(SETTINGS).set_string("card-size", size);
}

fn sort_key(settings: &gio::Settings, page: &str) -> Option<String> {
    let key = format!("sort-{page}");
    settings
        .settings_schema()
        .is_some_and(|s| s.has_key(&key))
        .then_some(key)
}

pub fn sort_order(page: &str) -> Option<String> {
    let settings = gio::Settings::new(SETTINGS);
    let key = sort_key(&settings, page)?;
    Some(settings.string(&key).to_string())
}

pub fn set_sort_order(page: &str, order: &str) {
    let settings = gio::Settings::new(SETTINGS);
    if let Some(key) = sort_key(&settings, page) {
        let _ = settings.set_string(&key, order);
    }
}

#[cfg(test)]
mod pinned_playlists_tests {
    use super::*;

    fn pl(id: &str) -> PinnedObject {
        PinnedObject::new(id, PinnedKind::Playlist)
    }

    #[test]
    fn pin_adds_id() {
        let mut map = PinnedObjectsByUser::new();
        assert!(pin_in_map(&mut map, "user1", &pl("pl1")));
        assert_eq!(map["user1"], vec![pl("pl1")]);
    }

    #[test]
    fn double_pin_is_idempotent() {
        let mut map = PinnedObjectsByUser::new();
        assert!(pin_in_map(&mut map, "user1", &pl("pl1")));
        assert!(!pin_in_map(&mut map, "user1", &pl("pl1")));
        assert_eq!(map["user1"].len(), 1);
    }

    #[test]
    fn unpin_removes_id() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        assert!(unpin_from_map(
            &mut map,
            "user1",
            "pl1",
            PinnedKind::Playlist
        ));
        assert!(!map.contains_key("user1"));
    }

    #[test]
    fn unpin_last_item_yields_empty_map() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        unpin_from_map(&mut map, "user1", "pl1", PinnedKind::Playlist);
        assert!(map.is_empty());
    }

    #[test]
    fn is_playlist_pinned_checks_user_scope() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        assert!(map
            .get("user1")
            .unwrap()
            .iter()
            .any(|o| o.id == "pl1" && o.kind == PinnedKind::Playlist));
        assert!(!map
            .get("user2")
            .is_some_and(|objects| objects.iter().any(|o| o.id == "pl1")));
    }

    #[test]
    fn prune_drops_orphan_ids() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        pin_in_map(&mut map, "user1", &pl("pl2"));
        assert!(prune_user_pins(
            &mut map,
            "user1",
            PinnedKind::Playlist,
            &["pl1".to_string()],
        ));
        assert_eq!(map["user1"], vec![pl("pl1")]);
    }

    #[test]
    fn kind_is_part_of_identity() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("id1", PinnedKind::Album),
        );
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("id1", PinnedKind::Artist),
        );
        assert_eq!(map["user1"].len(), 2);
        assert!(unpin_from_map(&mut map, "user1", "id1", PinnedKind::Album));
        assert_eq!(map["user1"][0].kind, PinnedKind::Artist);
    }

    #[test]
    fn prune_is_kind_scoped() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(&mut map, "user1", &pl("pl1"));
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("alb1", PinnedKind::Album),
        );
        assert!(prune_user_pins(
            &mut map,
            "user1",
            PinnedKind::Album,
            &["alb2".to_string()],
        ));
        assert_eq!(map["user1"].len(), 1);
        assert_eq!(map["user1"][0].kind, PinnedKind::Playlist);
    }

    #[test]
    fn modern_serialization_includes_kind() {
        let mut map = PinnedObjectsByUser::new();
        pin_in_map(
            &mut map,
            "user1",
            &PinnedObject::new("alb1", PinnedKind::Album),
        );
        let json = serde_json::to_string(&map).unwrap();
        assert!(json.contains(r#""kind":"album""#));
        assert!(!json.contains("title"));
    }

    #[test]
    fn title_round_trips_and_is_optional() {
        let with_title: PinnedObject =
            serde_json::from_str(r#"{"id":"alb1","kind":"album","title":"Blue"}"#).unwrap();
        assert_eq!(with_title.title.as_deref(), Some("Blue"));
        let without_title: PinnedObject =
            serde_json::from_str(r#"{"id":"alb1","kind":"album"}"#).unwrap();
        assert_eq!(without_title.title, None);
    }
}
