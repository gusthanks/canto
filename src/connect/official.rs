//! Manages the official Spotify desktop client as a hidden playback backend.
//!
//! Some accounts can no longer be decrypted by librespot (Spotify's PlayPlay
//! DRM). For those, Canto stays the UI and drives the official client over
//! Spotify Connect. The client is only started on demand, its window is moved
//! to a hidden Hyprland special workspace, and it is terminated when Canto
//! exits or after a long idle period, to keep CPU usage low on weak machines.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::app::models::{Device, DeviceKind};

const HIDDEN_WORKSPACE: &str = "special:canto";
const WINDOW_SEARCH_TIMEOUT: Duration = Duration::from_secs(20);
const HYPRLAND_PID_TIMEOUT: Duration = Duration::from_secs(4);
static PIDFILE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Path of the marker file remembering that this account needs the official client.
fn marker_path() -> PathBuf {
    glib::user_data_dir().join("canto").join("use-official-client")
}

/// Path of the file remembering the Connect id of this computer's Spotify app.
fn device_id_path() -> PathBuf {
    glib::user_data_dir().join("canto").join("official-device-id")
}

/// Remembers the Connect id of the Spotify app on this computer, so a later
/// session recognises it even while the app is closed.
pub fn remember_official_device_id(id: &str) {
    let path = device_id_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, format!("{id}\n")) {
        warn!("could not persist the official device id: {e}");
    }
}

pub fn remembered_official_device_id() -> Option<String> {
    std::fs::read_to_string(device_id_path())
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Whether a Connect device label names this computer.
pub fn is_this_computer(label: &str) -> bool {
    hostname().is_some_and(|h| label.eq_ignore_ascii_case(&h))
}

pub fn account_needs_official_client() -> bool {
    marker_path().exists()
}

pub fn remember_account_needs_official_client() {
    let path = marker_path();
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(&path, "1\n") {
        warn!("could not persist official client preference: {e}");
    }
}

fn spotify_command() -> String {
    std::env::var("CANTO_SPOTIFY_CMD").unwrap_or_else(|_| "spotify".to_string())
}

fn is_binary_available(program: &str) -> bool {
    let path = Path::new(program);
    if path.components().count() > 1 {
        return path.is_file();
    }
    std::env::var_os("PATH")
        .map(|paths| {
            std::env::split_paths(&paths).any(|dir| {
                let candidate = dir.join(program);
                candidate.is_file()
            })
        })
        .unwrap_or(false)
}

fn hostname() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/hostname")
        .ok()
        .map(|h| h.trim().to_string())
        .filter(|h| !h.is_empty())
}

/// Picks the Connect device that corresponds to the official client on this machine.
pub fn find_local_official_device(devices: &[Device], allow_any_computer: bool) -> Option<Device> {
    let host = hostname();
    let computers = devices.iter().filter(|d| d.kind == DeviceKind::Computer);
    let by_name = host
        .as_ref()
        .and_then(|h| computers.clone().find(|d| d.label.eq_ignore_ascii_case(h)));
    by_name
        .or_else(|| allow_any_computer.then(|| computers.clone().next()).flatten())
        .cloned()
}

#[derive(Debug)]
struct ManagedProcess {
    pid: u32,
    program: String,
    waiter_running: bool,
}

#[derive(Default)]
pub struct OfficialClient {
    /// Tracked official client spawned by Canto (None if not running or not ours).
    process: Arc<Mutex<Option<ManagedProcess>>>,
    idle_since: Mutex<Option<Instant>>,
}

impl Drop for OfficialClient {
    fn drop(&mut self) {
        self.stop();
    }
}

impl OfficialClient {
    pub fn is_spawned(&self) -> bool {
        let Ok(mut guard) = self.process.lock() else {
            return false;
        };
        let Some(proc_info) = guard.as_ref() else {
            return false;
        };
        if proc_info.waiter_running || process_matches(proc_info.pid, &proc_info.program) {
            true
        } else {
            *guard = None;
            false
        }
    }

    /// Starts the official client hidden, unless Canto already started one.
    pub fn spawn(&self) -> Result<(), String> {
        keep_hidden_workspace_closed(Duration::from_secs(60));
        if self.is_spawned() {
            return Ok(());
        }
        let program = spotify_command();
        if !is_binary_available(&program) {
            return Err(format!("official Spotify client binary not found: {program}"));
        }

        let on_hyprland = std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_some();
        if on_hyprland {
            if let Some(existing) = find_existing_spotify_window() {
                if !existing.on_hidden_workspace {
                    let moved = move_window_to_hidden_workspace(&existing.address);
                    debug!(
                        "reusing existing official client window {} (moved silently: {moved})",
                        existing.address
                    );
                } else {
                    debug!(
                        "official client window {} already hidden on {HIDDEN_WORKSPACE}",
                        existing.address
                    );
                }
                return Ok(());
            }

            info!("starting official Spotify client silently via Hyprland: {program}");
            match spawn_silently_via_hyprland(&program) {
                Ok(pid) => {
                    if let Ok(mut guard) = self.process.lock() {
                        *guard = Some(ManagedProcess {
                            pid,
                            program: program.clone(),
                            waiter_running: false,
                        });
                    }
                    std::thread::spawn(move || ensure_window_hidden_on_hyprland(pid));
                    return Ok(());
                }
                Err(err) => {
                    warn!("silent Hyprland launch failed ({err}); falling back to direct spawn");
                }
            }
        } else {
            info!("starting official Spotify client: {program}");
        }

        let pid = self.spawn_direct(&program)?;
        if on_hyprland {
            std::thread::spawn(move || ensure_window_hidden_on_hyprland(pid));
        }
        Ok(())
    }

    fn spawn_direct(&self, program: &str) -> Result<u32, String> {
        let mut command = Command::new(program);
        command
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        // Make sure the client dies with Canto, even on a crash.
        unsafe {
            use std::os::unix::process::CommandExt;
            command.pre_exec(|| {
                libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
                Ok(())
            });
        }

        // The child is owned by a dedicated thread that lives as long as the
        // child does, so PDEATHSIG is bound to a stable thread.
        let (tx, rx) = std::sync::mpsc::channel();
        let proc_slot = Arc::clone(&self.process);
        let program_owned = program.to_string();
        std::thread::spawn(move || match command.spawn() {
            Ok(mut child) => {
                let pid = child.id();
                if let Ok(mut slot) = proc_slot.lock() {
                    *slot = Some(ManagedProcess {
                        pid,
                        program: program_owned,
                        waiter_running: true,
                    });
                }
                let _ = tx.send(Ok(pid));
                wait_and_clear_child(&mut child, pid, &proc_slot);
            }
            Err(e) => {
                let _ = tx.send(Err(format!("could not start '{program_owned}': {e}")));
            }
        });

        rx.recv_timeout(Duration::from_secs(5))
            .map_err(|_| "timed out starting the official client".to_string())?
    }

    /// Stops the client, but only if Canto started it.
    pub fn stop(&self) {
        let proc_info = self.process.lock().ok().and_then(|mut g| g.take());
        if let Some(info) = proc_info {
            if process_matches(info.pid, &info.program) {
                info!("stopping official Spotify client (pid {})", info.pid);
                unsafe {
                    libc::kill(info.pid as i32, libc::SIGTERM);
                }
            }
        }
        if let Ok(mut idle) = self.idle_since.lock() {
            *idle = None;
        }
    }

    /// Tracks idleness; returns true once the client has been idle for `limit`.
    pub fn idle_for(&self, is_playing: bool, limit: Duration) -> bool {
        let Ok(mut idle) = self.idle_since.lock() else {
            return false;
        };
        if is_playing || !self.is_spawned() {
            *idle = None;
            return false;
        }
        let since = idle.get_or_insert_with(Instant::now);
        since.elapsed() >= limit
    }
}

fn wait_and_clear_child(
    child: &mut Child,
    pid: u32,
    proc_slot: &Arc<Mutex<Option<ManagedProcess>>>,
) {
    let _ = child.wait();
    info!("official Spotify client (pid {pid}) exited");
    if let Ok(mut slot) = proc_slot.lock() {
        if slot.as_ref().is_some_and(|p| p.pid == pid) {
            *slot = None;
        }
    }
}

fn shell_quote(arg: &str) -> String {
    let mut quoted = String::with_capacity(arg.len() + 2);
    quoted.push('\'');
    for ch in arg.chars() {
        if ch == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(ch);
        }
    }
    quoted.push('\'');
    quoted
}

/// Quotes a string as a Lua double-quoted literal for `hyprctl eval`.
fn lua_quote(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for ch in value.chars() {
        match ch {
            '\\' => quoted.push_str("\\\\"),
            '"' => quoted.push_str("\\\""),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

fn build_hyprland_shell_command(pid_file: &Path, program: &str) -> String {
    let pid_q = shell_quote(&pid_file.to_string_lossy());
    let prog_q = shell_quote(program);
    // Single-quote safe command passed to `sh -c` so PID is recorded before `exec` replaces `sh`.
    let inner = "printf '%s\\n' \"$$\" > \"$1\"; exec \"$2\" >/dev/null 2>&1";
    format!("sh -c {} _ {pid_q} {prog_q}", shell_quote(inner))
}

fn build_hyprland_exec_spec(workspace: &str, pid_file: &Path, program: &str) -> String {
    let cmd = build_hyprland_shell_command(pid_file, program);
    format!("[workspace {workspace} silent; suppressevent {QUIET_EVENTS}] {cmd}")
}

/// Builds a Hyprland Lua `hl.exec_cmd` call targeting a silent workspace.
fn build_hyprland_lua_exec(workspace: &str, pid_file: &Path, program: &str) -> String {
    let cmd = build_hyprland_shell_command(pid_file, program);
    let rule = format!("{workspace} silent");
    format!(
        "hl.exec_cmd({}, {{ workspace = {}, suppress_event = {} }})",
        lua_quote(&cmd),
        lua_quote(&rule),
        lua_quote(QUIET_EVENTS)
    )
}

/// Window events the hidden Spotify app may not use to show itself.
const QUIET_EVENTS: &str = "activate activatefocus";

/// A named (so re-adding replaces it) Hyprland rule that stops the Spotify app
/// from asking for focus. With `misc:focus_on_activate` on (Omarchy's
/// default), its request would open the hidden workspace over the user's.
fn build_hyprland_lua_quiet_rule() -> String {
    format!(
        "hl.window_rule({{ name = \"canto-official-quiet\", match = {{ class = \"^([Ss]potify)$\" }}, suppress_event = {} }})",
        lua_quote(QUIET_EVENTS)
    )
}

/// Whether `hyprctl monitors -j` shows the hidden workspace open somewhere.
fn hidden_workspace_shown(monitors_json: &[u8]) -> bool {
    let Ok(serde_json::Value::Array(monitors)) = serde_json::from_slice(monitors_json) else {
        return false;
    };
    monitors.iter().any(|m| {
        m.get("specialWorkspace")
            .and_then(|w| w.get("name"))
            .and_then(|n| n.as_str())
            == Some(HIDDEN_WORKSPACE)
    })
}

/// For `duration`, closes the hidden workspace whenever the Spotify app pops
/// it open (it can still ask for focus when playback moves to it).
pub fn keep_hidden_workspace_closed(duration: Duration) {
    if std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none() {
        return;
    }
    hyprctl_eval(&build_hyprland_lua_quiet_rule());
    std::thread::spawn(move || {
        let deadline = Instant::now() + duration;
        let name = HIDDEN_WORKSPACE.trim_start_matches("special:");
        let toggle = format!("hl.dispatch(hl.dsp.workspace.toggle_special({}))", lua_quote(name));
        while Instant::now() < deadline {
            let shown = Command::new("hyprctl")
                .args(["monitors", "-j"])
                .stdin(Stdio::null())
                .output()
                .is_ok_and(|o| o.status.success() && hidden_workspace_shown(&o.stdout));
            if shown {
                info!("the Spotify app opened its hidden workspace; closing it");
                hyprctl_eval(&toggle);
            }
            std::thread::sleep(Duration::from_millis(250));
        }
    });
}

/// Builds a Hyprland Lua call moving an existing window silently to a workspace.
fn build_hyprland_lua_move(workspace: &str, address: &str) -> String {
    format!(
        "hl.dispatch(hl.dsp.window.move({{ workspace = {}, follow = false, window = {} }}))",
        lua_quote(workspace),
        lua_quote(&format!("address:{address}"))
    )
}

fn unique_pid_file() -> PathBuf {
    let seq = PIDFILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let my_pid = std::process::id();
    glib::user_runtime_dir().join(format!("canto-official-{my_pid}-{seq}.pid"))
}

fn spawn_silently_via_hyprland(program: &str) -> Result<u32, String> {
    let pid_file = unique_pid_file();
    if let Some(parent) = pid_file.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&pid_file);

    let spec = build_hyprland_exec_spec(HIDDEN_WORKSPACE, &pid_file, program);
    let dispatched = hyprctl_dispatch(&["exec", &spec]) || {
        let lua = build_hyprland_lua_exec(HIDDEN_WORKSPACE, &pid_file, program);
        hyprctl_eval(&lua)
    };
    if !dispatched {
        let _ = std::fs::remove_file(&pid_file);
        return Err("hyprctl dispatch/eval exec did not return ok".to_string());
    }

    let deadline = Instant::now() + HYPRLAND_PID_TIMEOUT;
    while Instant::now() < deadline {
        if let Ok(raw) = std::fs::read_to_string(&pid_file) {
            if let Ok(pid) = raw.trim().parse::<u32>() {
                let _ = std::fs::remove_file(&pid_file);
                return Ok(pid);
            }
        }
        std::thread::sleep(Duration::from_millis(40));
    }

    let _ = std::fs::remove_file(&pid_file);
    Err("timed out waiting for official client PID after silent exec".to_string())
}

/// Returns true only when `hyprctl` succeeded and explicitly reported `ok` on stdout.
fn parse_hyprctl_ok(stdout: &str) -> bool {
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        return false;
    }
    trimmed
        .split(['\n', ';'])
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .all(|part| part.eq_ignore_ascii_case("ok"))
}

fn hyprctl_dispatch(args: &[&str]) -> bool {
    let Ok(output) = Command::new("hyprctl")
        .arg("dispatch")
        .args(args)
        .stdin(Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let ok = parse_hyprctl_ok(&stdout);
    if !ok {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("hyprctl dispatch rejected: stdout='{}' stderr='{}'", stdout.trim(), stderr.trim());
    }
    ok
}

fn hyprctl_eval(lua_expr: &str) -> bool {
    let Ok(output) = Command::new("hyprctl")
        .args(["eval", lua_expr])
        .stdin(Stdio::null())
        .output()
    else {
        return false;
    };
    if !output.status.success() {
        return false;
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let trimmed = stdout.trim();
    // `hyprctl eval` returns `ok` on void/unit evaluations or non-error output without `Invalid`
    parse_hyprctl_ok(trimmed)
        || (!trimmed.is_empty() && !trimmed.to_ascii_lowercase().contains("error"))
}

#[derive(Debug, PartialEq, Eq)]
struct HyprClientMatch {
    address: String,
    on_hidden_workspace: bool,
}

fn find_spotify_client_in_json(clients_json: &[u8], pid: Option<u32>) -> Option<HyprClientMatch> {
    let clients: serde_json::Value = serde_json::from_slice(clients_json).ok()?;
    let list = clients.as_array()?;

    let by_pid = pid.and_then(|target_pid| {
        list.iter().find_map(|c| {
            let same_pid = c.get("pid")?.as_u64() == Some(u64::from(target_pid));
            same_pid.then(|| client_match_from_value(c)).flatten()
        })
    });

    by_pid.or_else(|| {
        list.iter().find_map(|c| {
            let class = c
                .get("class")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let initial_class = c
                .get("initialClass")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            (class.eq_ignore_ascii_case("spotify")
                || initial_class.eq_ignore_ascii_case("spotify"))
            .then(|| client_match_from_value(c))
            .flatten()
        })
    })
}

fn client_match_from_value(client: &serde_json::Value) -> Option<HyprClientMatch> {
    let address = client.get("address")?.as_str()?.to_string();
    let workspace_name = client
        .get("workspace")
        .and_then(|w| w.get("name"))
        .and_then(|n| n.as_str())
        .unwrap_or_default();
    Some(HyprClientMatch {
        address,
        on_hidden_workspace: workspace_name == HIDDEN_WORKSPACE,
    })
}

fn query_hyprctl_clients() -> Option<Vec<u8>> {
    let output = Command::new("hyprctl")
        .args(["clients", "-j"])
        .stdin(Stdio::null())
        .output()
        .ok()?;
    output.status.success().then_some(output.stdout)
}

fn find_existing_spotify_window() -> Option<HyprClientMatch> {
    let json = query_hyprctl_clients()?;
    find_spotify_client_in_json(&json, None)
}

fn find_spotify_window(pid: u32) -> Option<HyprClientMatch> {
    let json = query_hyprctl_clients()?;
    find_spotify_client_in_json(&json, Some(pid))
}

fn move_window_to_hidden_workspace(address: &str) -> bool {
    let legacy = format!("{HIDDEN_WORKSPACE},address:{address}");
    if hyprctl_dispatch(&["movetoworkspacesilent", &legacy]) {
        return true;
    }
    let lua = build_hyprland_lua_move(HIDDEN_WORKSPACE, address);
    hyprctl_eval(&lua)
}

fn ensure_window_hidden_on_hyprland(pid: u32) {
    let deadline = Instant::now() + WINDOW_SEARCH_TIMEOUT;
    while Instant::now() < deadline {
        if let Some(matched) = find_spotify_window(pid) {
            if matched.on_hidden_workspace {
                debug!(
                    "official client window {} confirmed hidden on {HIDDEN_WORKSPACE}",
                    matched.address
                );
                return;
            }
            let ok = move_window_to_hidden_workspace(&matched.address);
            debug!(
                "moved official client window {} to {HIDDEN_WORKSPACE}: {ok}",
                matched.address
            );
            if ok {
                return;
            }
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    warn!("official client window not found on Hyprland within timeout");
}

fn process_matches(pid: u32, expected_program: &str) -> bool {
    let alive = unsafe { libc::kill(pid as i32, 0) == 0 };
    if !alive {
        return false;
    }
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) {
        if let Some(after_comm) = stat.rsplit_once(") ").map(|(_, rest)| rest) {
            if after_comm.starts_with('Z') {
                return false;
            }
        }
    }
    let expected_name = Path::new(expected_program)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(expected_program)
        .to_ascii_lowercase();
    if expected_name.is_empty() {
        return true;
    }
    if let Ok(cmdline) = std::fs::read(format!("/proc/{pid}/cmdline")) {
        if !cmdline.is_empty() {
            let text = String::from_utf8_lossy(&cmdline).to_ascii_lowercase();
            return text.contains(&expected_name) || text.contains("spotify");
        }
    }
    if let Ok(comm) = std::fs::read_to_string(format!("/proc/{pid}/comm")) {
        let lower = comm.trim().to_ascii_lowercase();
        return lower.contains("spotify")
            || expected_name.starts_with(&lower)
            || lower.starts_with(&expected_name);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hyprctl_ok_checks_stdout_explicitly() {
        assert!(parse_hyprctl_ok("ok"));
        assert!(parse_hyprctl_ok("ok\n"));
        assert!(parse_hyprctl_ok("ok ; ok\n"));
        assert!(!parse_hyprctl_ok(""));
        assert!(!parse_hyprctl_ok("Invalid dispatcher, requested foo does not exist"));
        assert!(!parse_hyprctl_ok("Window 0x123 not found"));
        assert!(!parse_hyprctl_ok("ok\nInvalid"));
    }

    #[test]
    fn shell_quote_handles_spaces_and_quotes() {
        assert_eq!(shell_quote("/usr/bin/spotify"), "'/usr/bin/spotify'");
        assert_eq!(shell_quote("/tmp/a'b c.pid"), "'/tmp/a'\\''b c.pid'");
    }

    #[test]
    fn build_exec_spec_targets_silent_special_workspace() {
        let spec = build_hyprland_exec_spec(
            "special:canto",
            Path::new("/run/user/1000/canto.pid"),
            "/usr/bin/spotify",
        );
        assert!(spec.starts_with(
            "[workspace special:canto silent; suppressevent activate activatefocus] sh -c "
        ));
        assert!(spec.contains("'/run/user/1000/canto.pid'"));
        assert!(spec.contains("'/usr/bin/spotify'"));
    }

    #[test]
    fn lua_quote_escapes_special_characters() {
        assert_eq!(lua_quote("a\"b\\c\nd"), "\"a\\\"b\\\\c\\nd\"");
    }

    #[test]
    fn build_lua_exec_targets_silent_workspace() {
        let lua = build_hyprland_lua_exec(
            "special:canto",
            Path::new("/run/user/1000/canto.pid"),
            "/usr/bin/spotify",
        );
        assert!(lua.starts_with("hl.exec_cmd(\"sh -c "));
        assert!(lua.ends_with(
            ", { workspace = \"special:canto silent\", suppress_event = \"activate activatefocus\" })"
        ));
        assert!(lua.contains("'/run/user/1000/canto.pid'"));
        assert!(lua.contains("'/usr/bin/spotify'"));
    }

    #[test]
    fn hidden_workspace_detection_reads_special_workspace() {
        let open = br#"[{"specialWorkspace":{"id":-98,"name":"special:canto"}}]"#;
        let closed = br#"[{"specialWorkspace":{"id":0,"name":""}}]"#;
        let other = br#"[{"specialWorkspace":{"id":-97,"name":"special:scratchpad"}}]"#;
        assert!(hidden_workspace_shown(open));
        assert!(!hidden_workspace_shown(closed));
        assert!(!hidden_workspace_shown(other));
        assert!(!hidden_workspace_shown(b"not json"));
    }

    #[test]
    fn quiet_rule_names_itself_and_suppresses_activation() {
        let rule = build_hyprland_lua_quiet_rule();
        assert!(rule.starts_with("hl.window_rule({ name = \"canto-official-quiet\""));
        assert!(rule.contains("suppress_event = \"activate activatefocus\""));
    }

    #[test]
    fn build_lua_move_dispatches_window_move_silently() {
        let lua = build_hyprland_lua_move("special:canto", "0x222");
        assert_eq!(
            lua,
            "hl.dispatch(hl.dsp.window.move({ workspace = \"special:canto\", follow = false, window = \"address:0x222\" }))"
        );
    }

    #[test]
    fn find_spotify_client_matches_pid_and_workspace() {
        let json = br#"[
            {"address": "0x111", "pid": 42, "class": "Alacritty", "workspace": {"name": "1"}},
            {"address": "0x222", "pid": 99, "class": "Spotify", "workspace": {"name": "special:canto"}}
        ]"#;
        assert_eq!(
            find_spotify_client_in_json(json, Some(99)),
            Some(HyprClientMatch {
                address: "0x222".to_string(),
                on_hidden_workspace: true,
            })
        );
        assert_eq!(
            find_spotify_client_in_json(json, Some(999)),
            Some(HyprClientMatch {
                address: "0x222".to_string(),
                on_hidden_workspace: true,
            })
        );
    }

    #[test]
    fn find_spotify_client_detects_visible_initial_class() {
        let json = br#"[
            {"address": "0x333", "pid": 123, "class": "", "initialClass": "spotify", "workspace": {"name": "2"}}
        ]"#;
        assert_eq!(
            find_spotify_client_in_json(json, None),
            Some(HyprClientMatch {
                address: "0x333".to_string(),
                on_hidden_workspace: false,
            })
        );
    }
}
