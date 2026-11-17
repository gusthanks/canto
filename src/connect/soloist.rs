//! Spotify Soloist: Spotify's own headless player for Linux, as the engine
//! Canto plays through when librespot can't (Spotify's PlayPlay DRM).
//!
//! Soloist shows up in Spotify Connect under [`DEVICE_NAME`], has no window,
//! and takes a few dozen MB where the hidden desktop app took gigabytes.
//! Canto starts it with the user's developer key (from the Secret Service)
//! and drives it with `soloist ctl`, its local control command: play, pause,
//! skip, seek, volume, shuffle, repeat and the playback state, all local and
//! immediate. Starting an album or playlist at a given track still goes
//! through the Web API, as for any Connect device.
//!
//! Docs: <https://developer.spotify.com/documentation/soloist>

use gio::prelude::SettingsExt;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::app::models::RepeatMode;

use super::spotify_mpris::Snapshot;

/// The name Soloist shows in Spotify's device lists.
pub const DEVICE_NAME: &str = "Canto";
/// A Soloist build stops working this long after it was built.
pub const BUILD_LIFETIME_DAYS: i64 = 90;
const CTL_TIMEOUT: Duration = Duration::from_secs(3);

/// The pid of the Soloist Canto started, to stop it along with Canto.
static SPAWNED: Mutex<Option<u32>> = Mutex::new(None);

fn data_dir() -> PathBuf {
    glib::user_data_dir().join("soloist")
}

/// Where Canto keeps the Soloist it downloads.
pub fn managed_dir() -> PathBuf {
    glib::user_data_dir().join("canto").join("soloist")
}

fn device_id_path() -> PathBuf {
    glib::user_data_dir()
        .join("canto")
        .join("soloist-device-id")
}

/// The Soloist program: `CANTO_SOLOIST_CMD`, the copy Canto downloaded, or
/// one installed by the user.
pub fn binary() -> Option<PathBuf> {
    if let Some(cmd) = std::env::var_os("CANTO_SOLOIST_CMD") {
        return Some(PathBuf::from(cmd));
    }
    let mut candidates = vec![managed_dir().join("soloist")];
    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(PathBuf::from(home).join(".local/bin/soloist"));
    }
    if let Some(paths) = std::env::var_os("PATH") {
        candidates.extend(std::env::split_paths(&paths).map(|dir| dir.join("soloist")));
    }
    candidates.into_iter().find(|p| p.is_file())
}

/// Whether the user turned Soloist on (it is, unless they turned it off).
pub fn is_enabled() -> bool {
    gio::Settings::new(canto_config::settings::SETTINGS)
        .boolean(canto_config::settings::SOLOIST_ENABLED)
}

pub fn remember_device_id(id: &str) {
    let path = device_id_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, format!("{id}\n")) {
        warn!("could not persist the Soloist device id: {e}");
    }
}

pub fn remembered_device_id() -> Option<String> {
    std::fs::read_to_string(device_id_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn pid_alive_as_soloist(pid: u32) -> bool {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|cmdline| {
            cmdline
                .split(|b| *b == 0)
                .next()
                .map(|exe| String::from_utf8_lossy(exe).ends_with("soloist"))
        })
        .unwrap_or(false)
}

/// Whether a Soloist runs with its control port open (started by Canto or
/// by the user).
pub fn is_running() -> bool {
    let dir = data_dir();
    let pid = std::fs::read_to_string(dir.join("soloist.pid"))
        .ok()
        .and_then(|p| p.trim().parse::<u32>().ok());
    pid.is_some_and(pid_alive_as_soloist) && dir.join("ws.port").is_file()
}

fn ctl(args: &[&str]) -> Result<String, String> {
    let program = binary().ok_or("Soloist is not installed")?;
    let mut child = Command::new(program)
        .arg("ctl")
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| e.to_string())?;
    let started = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < CTL_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(15))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(format!("soloist ctl {} timed out", args.join(" ")));
            }
        }
    }
    let output = child.wait_with_output().map_err(|e| e.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() {
        Ok(stdout)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(format!(
            "soloist ctl {}: {}{}",
            args.join(" "),
            stdout.trim(),
            stderr.trim()
        ))
    }
}

/// Logged in and active device, as Soloist reports them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Status {
    pub logged_in: bool,
    pub active: bool,
}

pub fn parse_status(text: &str) -> Status {
    let yes = |key: &str| {
        text.lines()
            .any(|line| line.trim().eq_ignore_ascii_case(&format!("{key}: yes")))
    };
    Status {
        logged_in: yes("logged in"),
        active: yes("active device"),
    }
}

pub fn status() -> Option<Status> {
    is_running()
        .then(|| ctl(&["status"]).ok())
        .flatten()
        .map(|text| parse_status(&text))
}

/// Starts Soloist with the user's key, unless one already runs. It dies
/// with Canto.
pub fn spawn(api_key: &str, initial_volume: u8) -> Result<(), String> {
    if is_running() {
        return Ok(());
    }
    let program = binary().ok_or("Soloist is not installed")?;
    let mut command = Command::new(&program);
    command
        .args(["--device-name", DEVICE_NAME])
        .args(["--api-key", api_key])
        .args(["--initial-volume", &initial_volume.min(100).to_string()])
        .args(["--cache-size", "500"])
        .args(["--ws", "127.0.0.1:0"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    unsafe {
        use std::os::unix::process::CommandExt;
        command.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    // The child lives on its own thread, so PDEATHSIG is tied to a thread
    // that lasts as long as it does
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || match command.spawn() {
        Ok(mut child) => {
            let pid = child.id();
            if let Ok(mut spawned) = SPAWNED.lock() {
                *spawned = Some(pid);
            }
            let _ = tx.send(Ok(pid));
            let status = child.wait();
            info!("Soloist (pid {pid}) exited: {status:?}");
            if let Ok(mut spawned) = SPAWNED.lock() {
                if *spawned == Some(pid) {
                    *spawned = None;
                }
            }
        }
        Err(e) => {
            let _ = tx.send(Err(format!("could not start Soloist: {e}")));
        }
    });
    let pid = rx
        .recv_timeout(Duration::from_secs(5))
        .map_err(|_| "timed out starting Soloist".to_string())??;
    info!("started Soloist (pid {pid}) as '{DEVICE_NAME}'");
    // Its control port opens a moment later
    for _ in 0..40 {
        if is_running() {
            return Ok(());
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    Err("Soloist started but its control port didn't open".to_string())
}

/// Stops the Soloist Canto started (one the user started stays).
pub fn stop_spawned() {
    let pid = SPAWNED.lock().ok().and_then(|mut s| s.take());
    if let Some(pid) = pid {
        if pid_alive_as_soloist(pid) {
            info!("stopping Soloist (pid {pid}) along with Canto");
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }
    }
}

pub fn play() -> Result<(), String> {
    ctl(&["play"]).map(drop)
}

pub fn pause() -> Result<(), String> {
    ctl(&["pause"]).map(drop)
}

pub fn next() -> Result<(), String> {
    ctl(&["next"]).map(drop)
}

pub fn previous() -> Result<(), String> {
    ctl(&["prev"]).map(drop)
}

/// Plays a Spotify URI (a track, album or playlist) from its start.
pub fn open_uri(uri: &str) -> Result<(), String> {
    ctl(&["play", uri]).map(drop)
}

pub fn seek(position_ms: u32) -> Result<(), String> {
    ctl(&["seek", &position_ms.to_string()]).map(drop)
}

pub fn set_volume(volume: f64) -> Result<(), String> {
    let percent = (volume.clamp(0.0, 1.0) * 100.0).round() as u8;
    ctl(&["volume", &percent.to_string()]).map(drop)
}

pub fn set_shuffle(shuffle: bool) -> Result<(), String> {
    ctl(&["shuffle", if shuffle { "on" } else { "off" }]).map(drop)
}

pub fn set_repeat(mode: RepeatMode) -> Result<(), String> {
    let mode = match mode {
        RepeatMode::Off => "off",
        RepeatMode::Context => "context",
        RepeatMode::Track => "track",
    };
    ctl(&["repeat", mode]).map(drop)
}

/// Makes Soloist the active Connect device.
pub fn activate() -> Result<(), String> {
    ctl(&["activate"]).map(drop)
}

fn now_ms() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis() as u64)
}

/// Soloist's `playback_state` (`soloist ctl now --json`) as a snapshot; the
/// position is moved forward to `now_epoch_ms` while it plays.
pub fn parse_snapshot(json: &str, now_epoch_ms: Option<u64>) -> Option<Snapshot> {
    let state: serde_json::Value = serde_json::from_str(json).ok()?;
    let status = state.get("status")?.as_str()?;
    let track_id = state
        .pointer("/item/uri")
        .and_then(|u| u.as_str())
        .and_then(|uri| uri.strip_prefix("spotify:track:"))
        .filter(|id| !id.is_empty())
        .map(str::to_owned);
    let position = state.get("position");
    let at = position
        .and_then(|p| p.get("position_ms"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let stamp = position
        .and_then(|p| p.get("timestamp_ms"))
        .and_then(|v| v.as_f64());
    let speed = position
        .and_then(|p| p.get("speed"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let elapsed = match (stamp, now_epoch_ms) {
        (Some(stamp), Some(now)) if speed > 0.0 => {
            ((now as f64 - stamp) * speed).clamp(0.0, 10_000.0)
        }
        _ => 0.0,
    };
    let mut position_ms = (at + elapsed).max(0.0);
    if let Some(duration) = state
        .pointer("/item/decorations/playback/duration_ms")
        .and_then(|v| v.as_f64())
        .filter(|d| *d > 0.0)
    {
        position_ms = position_ms.min(duration);
    }
    let repeat = match state.pointer("/options/repeat").and_then(|v| v.as_str()) {
        Some("context") => RepeatMode::Context,
        Some("track") => RepeatMode::Track,
        _ => RepeatMode::Off,
    };
    Some(Snapshot {
        playing: matches!(status, "playing" | "buffering"),
        track_id,
        item_path: None,
        position_ms: position_ms.min(f64::from(u32::MAX)) as u32,
        repeat,
        shuffle: state
            .pointer("/options/shuffle")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        volume: state
            .get("volume")
            .and_then(|v| v.as_f64())
            .map(|v| (v / 100.0).clamp(0.0, 1.0)),
    })
}

/// What Soloist plays now, or `None` when it isn't reachable.
pub fn snapshot() -> Option<Snapshot> {
    if !is_running() {
        return None;
    }
    let json = ctl(&["now", "--json"]).ok()?;
    parse_snapshot(&json, now_ms())
}

/// The build date in `soloist --version` (`... (20261117) ...`), as days
/// since the Unix epoch.
pub fn parse_build_day(version: &str) -> Option<i64> {
    version.split(['(', ')']).find_map(|part| {
        if part.len() != 8 || !part.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        let year: i32 = part[0..4].parse().ok()?;
        let month: u32 = part[4..6].parse().ok()?;
        let day: u32 = part[6..8].parse().ok()?;
        let date = glib::DateTime::from_utc(year, month as i32, day as i32, 0, 0, 0.0).ok()?;
        Some(date.to_unix() / 86_400)
    })
}

/// The installed build's version line.
pub fn version() -> Option<String> {
    let output = Command::new(binary()?)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .ok()?;
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

/// Days left before the installed build stops working (negative once it has).
pub fn days_left() -> Option<i64> {
    let built = parse_build_day(&version()?)?;
    let today = now_ms()? as i64 / 86_400_000;
    Some(built + BUILD_LIFETIME_DAYS - today)
}

/// How Soloist stands, for the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    pub installed: bool,
    pub running: bool,
    pub logged_in: bool,
    pub active: bool,
    pub days_left: Option<i64>,
}

/// Reads Soloist's state. Blocks for a moment: call it off the main thread.
pub fn report() -> Report {
    let status = status().unwrap_or_default();
    Report {
        installed: binary().is_some(),
        running: is_running(),
        logged_in: status.logged_in,
        active: status.active,
        days_left: days_left(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: &str = r#"{"type":"playback_state","status":"playing","item":{"uri":"spotify:track:555rmZC4fMJDkVavXReVf7","entity_type":"track","decorations":{"identity":{"name":"NOITE PERFEITA"},"playback":{"duration_ms":169000,"content_ratings":[]}}},"position":{"position_ms":121112,"timestamp_ms":1794928730445,"speed":1},"volume":40,"is_active":true,"options":{"shuffle":true,"repeat":"context","playback_speed":1,"modes":{}}}"#;

    #[test]
    fn reads_the_playback_state() {
        let snap = parse_snapshot(NOW, Some(1794928730445 + 2000)).unwrap();
        assert!(snap.playing);
        assert_eq!(snap.track_id.as_deref(), Some("555rmZC4fMJDkVavXReVf7"));
        assert_eq!(snap.position_ms, 123112);
        assert_eq!(snap.repeat, RepeatMode::Context);
        assert!(snap.shuffle);
        assert_eq!(snap.volume, Some(0.4));
        assert_eq!(snap.item_path, None);
    }

    #[test]
    fn a_paused_position_stays_and_never_passes_the_end() {
        let paused = NOW
            .replace("\"playing\"", "\"paused\"")
            .replace("\"speed\":1", "\"speed\":0");
        let snap = parse_snapshot(&paused, Some(1794928730445 + 9000)).unwrap();
        assert!(!snap.playing);
        assert_eq!(snap.position_ms, 121112);
        let late = parse_snapshot(NOW, Some(1794928730445 + 60_000)).unwrap();
        assert_eq!(
            late.position_ms, 131112,
            "a stale stamp moves it 10 s at most"
        );
        let over = NOW.replace("121112", "168000");
        assert_eq!(
            parse_snapshot(&over, Some(1794928730445 + 5000))
                .unwrap()
                .position_ms,
            169000
        );
    }

    #[test]
    fn reads_status_and_build_date() {
        let status = parse_status("soloist: running (pid 1)\n  ws: listening on 127.0.0.1:1\n  logged in: yes\n  active device: no\n");
        assert_eq!(
            status,
            Status {
                logged_in: true,
                active: false
            }
        );
        assert_eq!(parse_status("soloist: not running"), Status::default());
        let day = parse_build_day(
            "soloist 1.3.10.78 build 1794916883 (20261117) (g19412df6ea) (linux/x86_64)",
        )
        .unwrap();
        let expected = glib::DateTime::from_utc(2026, 11, 17, 0, 0, 0.0)
            .unwrap()
            .to_unix()
            / 86_400;
        assert_eq!(day, expected);
        assert_eq!(parse_build_day("soloist 1.0 (linux/x86_64)"), None);
    }
}
