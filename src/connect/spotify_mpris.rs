//! Local control of the Spotify app on this computer through MPRIS, the D-Bus
//! interface media keys use.
//!
//! Driving that app through Spotify's Web API goes out to Spotify's servers
//! and back for every press, and they often answer "device not found" or a
//! server error while the app plays fine. Over MPRIS, play, pause, seek,
//! repeat, shuffle and the playback state are local and immediate. Starting a
//! given album or playlist at a given track still needs the Web API (MPRIS can
//! only open a URI from its start).

use gio::prelude::*;
use std::time::Duration;

use crate::app::models::RepeatMode;

const BUS_NAME: &str = "org.mpris.MediaPlayer2.spotify";
const OBJECT_PATH: &str = "/org/mpris/MediaPlayer2";
const PLAYER: &str = "org.mpris.MediaPlayer2.Player";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
const TIMEOUT: Duration = Duration::from_secs(2);

/// What the Spotify app is doing, as it reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    pub playing: bool,
    /// Spotify id of the track, `None` for ads, episodes or nothing loaded.
    pub track_id: Option<String>,
    /// MPRIS object path of the item, needed to seek in it.
    pub item_path: Option<String>,
    pub position_ms: u32,
    pub repeat: RepeatMode,
    pub shuffle: bool,
    pub volume: Option<f64>,
}

fn connection() -> Option<gio::DBusConnection> {
    gio::bus_get_sync(gio::BusType::Session, None::<&gio::Cancellable>).ok()
}

fn call(
    interface: &str,
    method: &str,
    args: Option<&glib::Variant>,
) -> Result<glib::Variant, String> {
    let connection = connection().ok_or("no session bus")?;
    connection
        .call_sync(
            Some(BUS_NAME),
            OBJECT_PATH,
            interface,
            method,
            args,
            None,
            gio::DBusCallFlags::NO_AUTO_START,
            TIMEOUT.as_millis() as i32,
            None::<&gio::Cancellable>,
        )
        .map_err(|e| e.to_string())
}

/// Whether the Spotify app is running (it owns its MPRIS name).
pub fn is_running() -> bool {
    let Some(connection) = connection() else {
        return false;
    };
    connection
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(BUS_NAME,).to_variant()),
            Some(glib::VariantTy::new("(b)").unwrap()),
            gio::DBusCallFlags::NONE,
            TIMEOUT.as_millis() as i32,
            None::<&gio::Cancellable>,
        )
        .ok()
        .and_then(|reply| reply.child_value(0).get::<bool>())
        .unwrap_or(false)
}

pub fn play() -> Result<(), String> {
    call(PLAYER, "Play", None).map(drop)
}

pub fn pause() -> Result<(), String> {
    call(PLAYER, "Pause", None).map(drop)
}

/// Starts `uri` (e.g. `spotify:track:<id>`) from its beginning.
pub fn open_uri(uri: &str) -> Result<(), String> {
    call(PLAYER, "OpenUri", Some(&(uri,).to_variant())).map(drop)
}

/// Moves to `position_ms` in the item at `item_path`.
pub fn set_position(item_path: &str, position_ms: u32) -> Result<(), String> {
    if !glib::Variant::is_object_path(item_path) {
        return Err(format!("not an object path: {item_path}"));
    }
    let micros = i64::from(position_ms) * 1000;
    // An object path can't hold quotes, so this text form is safe
    let args = glib::Variant::parse(
        Some(glib::VariantTy::new("(ox)").unwrap()),
        &format!("(objectpath '{item_path}', int64 {micros})"),
    )
    .map_err(|e| e.to_string())?;
    call(PLAYER, "SetPosition", Some(&args)).map(drop)
}

fn set_property(name: &str, value: glib::Variant) -> Result<(), String> {
    call(PROPERTIES, "Set", Some(&(PLAYER, name, value).to_variant())).map(drop)
}

pub fn set_repeat(mode: RepeatMode) -> Result<(), String> {
    set_property("LoopStatus", loop_status(mode).to_variant())
}

pub fn set_shuffle(shuffle: bool) -> Result<(), String> {
    set_property("Shuffle", shuffle.to_variant())
}

pub fn set_volume(volume: f64) -> Result<(), String> {
    set_property("Volume", volume.clamp(0.0, 1.0).to_variant())
}

/// The app's current state, or `None` when it isn't reachable.
pub fn snapshot() -> Option<Snapshot> {
    let reply = call(PROPERTIES, "GetAll", Some(&(PLAYER,).to_variant())).ok()?;
    let props = reply.child_value(0);
    let dict = glib::VariantDict::new(Some(&props));
    let status: String = dict.lookup("PlaybackStatus").ok().flatten()?;
    let item_path = dict
        .lookup_value("Metadata", None)
        .map(|metadata| glib::VariantDict::new(Some(&metadata)))
        .and_then(|metadata| metadata.lookup_value("mpris:trackid", None))
        .and_then(|value| value.str().map(str::to_owned));
    let position_us: i64 = dict.lookup("Position").ok().flatten().unwrap_or(0);
    let loop_status: String = dict
        .lookup("LoopStatus")
        .ok()
        .flatten()
        .unwrap_or_else(|| "None".to_string());
    Some(Snapshot {
        playing: status == "Playing",
        track_id: item_path.as_deref().and_then(track_id_from_path),
        item_path,
        position_ms: (position_us.max(0) / 1000).min(i64::from(u32::MAX)) as u32,
        repeat: repeat_mode(&loop_status),
        shuffle: dict.lookup("Shuffle").ok().flatten().unwrap_or(false),
        volume: dict.lookup("Volume").ok().flatten(),
    })
}

/// `/com/spotify/track/<id>` (or a `spotify:track:<id>` URI) to `<id>`.
pub fn track_id_from_path(path: &str) -> Option<String> {
    let id = path
        .strip_prefix("/com/spotify/track/")
        .or_else(|| path.strip_prefix("spotify:track:"))?;
    (!id.is_empty() && id.chars().all(|c| c.is_ascii_alphanumeric())).then(|| id.to_string())
}

fn loop_status(mode: RepeatMode) -> &'static str {
    match mode {
        RepeatMode::Off => "None",
        RepeatMode::Track => "Track",
        RepeatMode::Context => "Playlist",
    }
}

fn repeat_mode(status: &str) -> RepeatMode {
    match status {
        "Track" => RepeatMode::Track,
        "Playlist" => RepeatMode::Context,
        _ => RepeatMode::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn track_ids_come_from_track_paths_only() {
        assert_eq!(
            track_id_from_path("/com/spotify/track/5x6ZQVigUJhSIHN65rp6g9").as_deref(),
            Some("5x6ZQVigUJhSIHN65rp6g9")
        );
        assert_eq!(
            track_id_from_path("spotify:track:abc123").as_deref(),
            Some("abc123")
        );
        assert_eq!(track_id_from_path("/com/spotify/ad/123"), None);
        assert_eq!(track_id_from_path("/com/spotify/episode/123"), None);
        assert_eq!(track_id_from_path("/com/spotify/track/"), None);
    }

    /// Reads the real Spotify app, when one runs: `cargo test -- --ignored live_snapshot`
    #[test]
    #[ignore]
    fn live_snapshot() {
        println!("running: {}", is_running());
        println!("{:?}", snapshot());
    }

    #[test]
    fn set_position_arguments_parse() {
        let args = glib::Variant::parse(
            Some(glib::VariantTy::new("(ox)").unwrap()),
            "(objectpath '/com/spotify/track/abc', int64 5000)",
        )
        .unwrap();
        assert_eq!(args.type_().as_str(), "(ox)");
    }

    #[test]
    fn repeat_modes_round_trip_through_loop_status() {
        for mode in [RepeatMode::Off, RepeatMode::Track, RepeatMode::Context] {
            assert_eq!(repeat_mode(loop_status(mode)), mode);
        }
        assert_eq!(repeat_mode("anything else"), RepeatMode::Off);
    }
}
