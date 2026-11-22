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

use gettextrs::gettext;
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
/// Where Spotify publishes the current Soloist build.
const DOWNLOAD_URL: &str = "https://soloist-builds.spotifycdn.com/soloist_release_x86_64.tar.gz";
/// A new build is fetched once this few days are left.
pub const UPDATE_WITHIN_DAYS: i64 = 30;
/// How often the CDN is asked again when it had nothing newer.
const RECHECK_AFTER_SECS: i64 = 20 * 3600;

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

/// Non-empty `CANTO_SOLOIST_CMD` override from the environment.
fn custom_cmd() -> Option<std::ffi::OsString> {
    std::env::var_os("CANTO_SOLOIST_CMD").filter(|v| !v.to_string_lossy().trim().is_empty())
}

/// Resolves a user-supplied `CANTO_SOLOIST_CMD` value to an existing file:
/// paths with a separator are checked directly; bare names are looked up in `PATH`.
fn resolve_custom_binary(
    cmd: &std::ffi::OsStr,
    path_env: Option<&std::ffi::OsStr>,
) -> Option<PathBuf> {
    let trimmed = cmd.to_string_lossy();
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let path = PathBuf::from(trimmed);
    if path.components().count() > 1 {
        return path.is_file().then_some(path);
    }
    let paths = path_env?;
    std::env::split_paths(paths)
        .map(|dir| dir.join(&path))
        .find(|candidate| candidate.is_file())
}

/// The Soloist program: `CANTO_SOLOIST_CMD`, the copy Canto downloaded, or
/// one installed by the user.
pub fn binary() -> Option<PathBuf> {
    if let Some(cmd) = custom_cmd() {
        return resolve_custom_binary(&cmd, std::env::var_os("PATH").as_deref());
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

/// Whether Canto can automatically download and update Soloist on this machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AutoDownloadSupport {
    #[default]
    Available,
    /// Spotify's CDN only publishes Linux x86_64 Soloist builds.
    UnsupportedArch,
    /// `CANTO_SOLOIST_CMD` points at a user-managed binary.
    CustomBinary,
}

/// Reports whether automatic Soloist downloads are supported in this environment.
pub fn auto_download_support() -> AutoDownloadSupport {
    if custom_cmd().is_some() {
        AutoDownloadSupport::CustomBinary
    } else if cfg!(target_arch = "x86_64") {
        AutoDownloadSupport::Available
    } else {
        AutoDownloadSupport::UnsupportedArch
    }
}

/// What looking for a new Soloist build did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UpdateOutcome {
    /// The installed build has more than `UPDATE_WITHIN_DAYS` left.
    UpToDate,
    /// Asked a moment ago; not asking the CDN again yet.
    CheckedRecently,
    /// The CDN has the same build (or an older one).
    NothingNewer,
    /// A newer build is in place; it runs the next time Soloist starts.
    Updated { days_left: i64 },
    /// Automatic downloads are only published for x86_64 Linux.
    UnsupportedArchitecture,
    /// `CANTO_SOLOIST_CMD` is set, so Canto does not replace it.
    ManagedExternally,
}

/// Whether to ask the CDN for a new build: when none is installed, or few
/// days are left and the last ask was a while ago (always once expired).
pub fn should_check(days_left: Option<i64>, secs_since_last_check: Option<i64>) -> bool {
    match days_left {
        None => true,
        Some(days) if days <= 0 => true,
        Some(days) if days > UPDATE_WITHIN_DAYS => false,
        Some(_) => secs_since_last_check.is_none_or(|secs| secs >= RECHECK_AFTER_SECS),
    }
}

fn last_check_path() -> PathBuf {
    managed_dir().join("last-update-check")
}

fn secs_since_last_check(now_secs: i64) -> Option<i64> {
    let text = std::fs::read_to_string(last_check_path()).ok()?;
    let then: i64 = text.trim().parse().ok()?;
    Some(now_secs - then)
}

fn run_checked(command: &mut Command, what: &str) -> Result<(), String> {
    let output = command
        .stdin(Stdio::null())
        .output()
        .map_err(|e| format!("{what}: {e}"))?;
    if output.status.success() {
        Ok(())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("{what}: {}", err.trim()))
    }
}

/// Fetches Spotify's current Soloist build when the installed one is
/// missing or near its end, and puts it where `binary()` looks first. The
/// running Soloist keeps going; the new one is used from its next start.
/// Blocks for the download: call it off the main thread.
pub fn update() -> Result<UpdateOutcome, String> {
    match auto_download_support() {
        AutoDownloadSupport::Available => {}
        AutoDownloadSupport::UnsupportedArch => {
            info!("Automatic Soloist download is only available on x86_64; skipping update");
            return Ok(UpdateOutcome::UnsupportedArchitecture);
        }
        AutoDownloadSupport::CustomBinary => {
            return Ok(UpdateOutcome::ManagedExternally);
        }
    }
    let installed_day = version().as_deref().and_then(parse_build_day);
    let now_secs = (now_ms().ok_or("no clock")? / 1000) as i64;
    let days = installed_day.map(|day| day + BUILD_LIFETIME_DAYS - now_secs / 86_400);
    if !should_check(days, secs_since_last_check(now_secs)) {
        return Ok(if days.is_some_and(|d| d > UPDATE_WITHIN_DAYS) {
            UpdateOutcome::UpToDate
        } else {
            UpdateOutcome::CheckedRecently
        });
    }

    let dir = managed_dir();
    let work = dir.join("download");
    if work.exists() {
        // Leftovers of an earlier download Canto made itself
        std::fs::remove_dir_all(&work).map_err(|e| format!("cleaning {work:?}: {e}"))?;
    }
    std::fs::create_dir_all(&work).map_err(|e| format!("creating {work:?}: {e}"))?;
    let archive = work.join("soloist.tar.gz");
    info!("downloading the current Soloist build");
    run_checked(
        Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location"])
            .args(["--proto", "=https", "--max-time", "600", "--output"])
            .arg(&archive)
            .arg(DOWNLOAD_URL),
        "downloading Soloist",
    )?;
    run_checked(
        Command::new("tar")
            .arg("-xzf")
            .arg(&archive)
            .arg("-C")
            .arg(&work)
            .args(["soloist", "THIRD_PARTY_LICENSES.txt", "CHANGELOG.md"]),
        "unpacking Soloist",
    )?;
    let candidate = work.join("soloist");
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&candidate, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("making Soloist runnable: {e}"))?;
    }
    let output = Command::new(&candidate)
        .arg("--version")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map_err(|e| format!("the downloaded Soloist does not run: {e}"))?;
    let new_version = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let new_day = parse_build_day(&new_version)
        .ok_or_else(|| format!("the downloaded Soloist has no build date: {new_version:?}"))?;
    let _ = std::fs::write(last_check_path(), now_secs.to_string());
    if installed_day.is_some_and(|day| new_day <= day) {
        let _ = std::fs::remove_dir_all(&work);
        info!("no newer Soloist build yet ({new_version})");
        return Ok(UpdateOutcome::NothingNewer);
    }
    for name in ["THIRD_PARTY_LICENSES.txt", "CHANGELOG.md"] {
        let _ = std::fs::rename(work.join(name), dir.join(name));
    }
    // Same folder, so the switch is atomic; a running Soloist keeps the
    // file it opened
    std::fs::rename(&candidate, dir.join("soloist"))
        .map_err(|e| format!("putting the new Soloist in place: {e}"))?;
    let _ = std::fs::remove_dir_all(&work);
    let days_left = new_day + BUILD_LIFETIME_DAYS - now_secs / 86_400;
    info!("Soloist updated to {new_version} ({days_left} days left)");
    Ok(UpdateOutcome::Updated { days_left })
}

/// How Soloist stands, for the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Report {
    pub installed: bool,
    pub running: bool,
    pub logged_in: bool,
    pub active: bool,
    pub days_left: Option<i64>,
    pub auto_download: AutoDownloadSupport,
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
        auto_download: auto_download_support(),
    }
}

/// One line on how Soloist stands, for the settings window.
pub fn status_text(report: &Report, key_saved: bool) -> String {
    let mut text = if !report.installed {
        match report.auto_download {
            AutoDownloadSupport::Available if key_saved => {
                gettext("Not installed yet: Canto gets it from Spotify when you press play")
            }
            AutoDownloadSupport::Available => gettext(
                "Not installed. Canto plays through the Spotify app hidden in the background.",
            ),
            AutoDownloadSupport::UnsupportedArch if key_saved => gettext(
                "Not installed. Automatic download is only available on x86_64; install Soloist manually or set CANTO_SOLOIST_CMD.",
            ),
            AutoDownloadSupport::UnsupportedArch => gettext(
                "Not installed. Automatic download is only available on x86_64; install Soloist manually, or Canto uses the hidden Spotify app.",
            ),
            AutoDownloadSupport::CustomBinary => {
                gettext("Not found at the path set in CANTO_SOLOIST_CMD.")
            }
        }
    } else if !key_saved {
        gettext("Installed, waiting for a key")
    } else if !report.running {
        gettext("Ready. It starts when you press play.")
    } else if !report.logged_in {
        gettext("Running, not logged in yet: in Spotify on your phone, pick “Canto” in the device list once")
    } else if report.active {
        gettext("Running and playing through Canto")
    } else {
        gettext("Running and logged in")
    };
    match report.days_left {
        Some(days) if days <= 0 => {
            text.push_str(" · ");
            text.push_str(&match report.auto_download {
                AutoDownloadSupport::Available => gettext(
                    "This Soloist build has expired; Canto gets the new one when you press play",
                ),
                AutoDownloadSupport::UnsupportedArch => gettext(
                    "This Soloist build has expired; automatic updates are only available on x86_64, so update Soloist manually",
                ),
                AutoDownloadSupport::CustomBinary => gettext(
                    "This Soloist build has expired; update the binary configured in CANTO_SOLOIST_CMD",
                ),
            });
        }
        Some(days) if days <= 14 => {
            text.push_str(" · ");
            text.push_str(
                &gettext("This Soloist build stops working in {days} days")
                    .replace("{days}", &days.to_string()),
            );
        }
        _ => {}
    }
    text
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
    fn asks_for_a_new_build_only_when_needed() {
        let day = 24 * 3600;
        // Nothing installed, or expired: always
        assert!(should_check(None, Some(60)));
        assert!(should_check(Some(0), Some(60)));
        assert!(should_check(Some(-3), Some(60)));
        // Plenty of time left: never
        assert!(!should_check(Some(UPDATE_WITHIN_DAYS + 1), None));
        assert!(!should_check(Some(80), Some(30 * day)));
        // Near the end: unless asked a moment ago
        assert!(should_check(Some(UPDATE_WITHIN_DAYS), None));
        assert!(should_check(Some(10), Some(day)));
        assert!(!should_check(Some(10), Some(3600)));
    }

    /// The real download, into an empty folder:
    /// `XDG_DATA_HOME=<empty> HOME=<empty> PATH=/usr/bin <test bin>
    /// connect::soloist::tests::downloads_the_current_build --ignored --exact`
    #[test]
    #[ignore = "downloads about 14 MB from Spotify's CDN"]
    fn downloads_the_current_build() {
        assert!(
            binary().is_none(),
            "point XDG_DATA_HOME and HOME at an empty folder and PATH at /usr/bin"
        );
        let outcome = update().expect("the update fails");
        eprintln!("first: {outcome:?}");
        assert!(matches!(outcome, UpdateOutcome::Updated { days_left } if days_left > 0));
        assert_eq!(binary(), Some(managed_dir().join("soloist")));
        assert!(days_left().is_some_and(|d| d > UPDATE_WITHIN_DAYS));
        assert!(!managed_dir().join("download").exists());
        let again = update().expect("the second update fails");
        eprintln!("again: {again:?}");
        assert_eq!(again, UpdateOutcome::UpToDate);
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

    #[test]
    fn status_text_explains_manual_install_when_auto_download_unavailable() {
        let arm_missing = Report {
            installed: false,
            auto_download: AutoDownloadSupport::UnsupportedArch,
            ..Default::default()
        };
        let with_key = status_text(&arm_missing, true);
        assert!(
            with_key.contains("x86_64") && with_key.contains("CANTO_SOLOIST_CMD"),
            "ARM64 status with key should explain x86_64 limitation: {}",
            with_key
        );
        assert!(
            !with_key.contains("gets it from Spotify"),
            "ARM64 status must not promise automatic download: {}",
            with_key
        );
        let without_key = status_text(&arm_missing, false);
        assert!(
            without_key.contains("x86_64"),
            "ARM64 status without key should explain x86_64 limitation: {}",
            without_key
        );

        let arm_expired = Report {
            installed: true,
            running: false,
            days_left: Some(-2),
            auto_download: AutoDownloadSupport::UnsupportedArch,
            ..Default::default()
        };
        let expired = status_text(&arm_expired, true);
        assert!(
            expired.contains("x86_64") && expired.contains("manually"),
            "ARM64 expired status should tell user to update manually: {}",
            expired
        );
        assert!(
            !expired.contains("gets the new one"),
            "ARM64 expired status must not promise auto-update: {}",
            expired
        );

        let x86_missing = Report {
            installed: false,
            auto_download: AutoDownloadSupport::Available,
            ..Default::default()
        };
        assert!(status_text(&x86_missing, true).contains("gets it from Spotify"));

        let custom_missing = Report {
            installed: false,
            auto_download: AutoDownloadSupport::CustomBinary,
            ..Default::default()
        };
        assert!(status_text(&custom_missing, true).contains("CANTO_SOLOIST_CMD"));

        // days_left == 0 is already expired (matching player.rs and should_check)
        let expires_today = Report {
            installed: true,
            days_left: Some(0),
            auto_download: AutoDownloadSupport::UnsupportedArch,
            ..Default::default()
        };
        let today_text = status_text(&expires_today, true);
        assert!(
            today_text.contains("expired") && !today_text.contains("0 days"),
            "days_left == 0 must report expired, got: {}",
            today_text
        );
    }

    #[test]
    fn custom_binary_must_exist() {
        let exe = std::env::current_exe().expect("current_exe");
        let exe_dir = exe.parent().expect("exe parent");
        let exe_name = exe.file_name().expect("exe file_name");

        assert_eq!(
            resolve_custom_binary(exe.as_os_str(), None),
            Some(exe.clone())
        );
        assert_eq!(
            resolve_custom_binary(exe_name, Some(exe_dir.as_os_str())),
            Some(exe.clone())
        );
        assert_eq!(
            resolve_custom_binary(std::ffi::OsStr::new("/nonexistent/canto/soloist"), None),
            None
        );
        assert_eq!(
            resolve_custom_binary(
                std::ffi::OsStr::new("definitely-missing-soloist-bin"),
                Some(exe_dir.as_os_str())
            ),
            None
        );
        assert_eq!(
            resolve_custom_binary(std::ffi::OsStr::new("   "), None),
            None
        );
    }
}
