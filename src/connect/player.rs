use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use futures::channel::mpsc::UnboundedSender;
use gettextrs::gettext;

use crate::app::load;
use crate::app::models::{queue_songs, PlayerState, RepeatMode, Track};
use crate::app::state::{Device, PlaybackAction, RemotePlaybackState};
use crate::app::{AppAction, SongsSource};
use riff_api::ApiService;
use riff_api::DomainError;

use super::official::{self, OfficialClient};

const OFFICIAL_CLIENT_STARTUP_TIMEOUT: Duration = Duration::from_secs(45);
const OFFICIAL_CLIENT_IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// At most one automatic restart of the Spotify app per this long, so a
/// device that stays unreachable ends in a message instead of a loop.
const OFFICIAL_RECOVERY_COOLDOWN: Duration = Duration::from_secs(60);

#[derive(Debug, Clone)]
pub enum ConnectCommand {
    SetDevice(String),
    /// Start (if needed) the hidden official client and play on it.
    UseOfficialClient,
    PlayerLoadInContext {
        source: SongsSource,
        offset: usize,
        song: String,
    },
    PlayerLoad {
        songs: Vec<String>,
        offset: usize,
    },
    PlayerResume,
    PlayerPause,
    PlayerStop,
    PlayerSeek(usize),
    PlayerRepeat(RepeatMode),
    PlayerShuffle(bool),
    PlayerSetVolume(u8),
}

pub struct ConnectPlayer {
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
    device_id: RwLock<Option<String>>,
    official_device_id: RwLock<Option<String>>,
    last_queue: RwLock<u64>,
    last_track_id: RwLock<Option<String>>,
    last_state: RwLock<PlayerState>,
    poll_count: AtomicU32,
    remote_playing: AtomicBool,
    official: OfficialClient,
    last_recovery: std::sync::Mutex<Option<std::time::Instant>>,
    /// When the last playback command was sent. A poll that started before
    /// Spotify could apply it would undo it in the UI (the play button
    /// "not working"), so such polls are dropped.
    last_command: std::sync::Mutex<Option<std::time::Instant>>,
}

impl ConnectPlayer {
    pub fn new(api: Arc<ApiService>, action_sender: UnboundedSender<AppAction>) -> Self {
        Self {
            api: api.clone(),
            action_sender,
            device_id: Default::default(),
            official_device_id: Default::default(),
            last_queue: Default::default(),
            last_track_id: Default::default(),
            last_state: Default::default(),
            poll_count: AtomicU32::new(0),
            remote_playing: AtomicBool::new(false),
            official: Default::default(),
            last_recovery: Default::default(),
            last_command: Default::default(),
        }
    }

    pub fn is_remote_playing(&self) -> bool {
        self.remote_playing.load(Ordering::Relaxed)
    }

    fn send_actions(&self, actions: impl IntoIterator<Item = AppAction>) {
        for action in actions.into_iter() {
            self.action_sender.unbounded_send(action).unwrap();
        }
    }

    fn device_lost(&self) {
        let _ = self.device_id.write().unwrap().take();
        self.send_actions([
            AppAction::ShowNotification(gettext("Connection to device lost!")),
            PlaybackAction::SwitchDevice(Device::Local).into(),
            PlaybackAction::SetAvailableDevices(vec![]).into(),
        ]);
    }

    async fn get_queue_if_changed(&self) -> Result<Option<Vec<Track>>, DomainError> {
        let last_queue = *self.last_queue.read().ok().as_deref().unwrap_or(&0u64);
        let songs = queue_songs(self.api.get_player_queue(load::transport()).await?);
        let hash = {
            let mut hasher = DefaultHasher::new();
            for s in &songs {
                s.rri.id.hash(&mut hasher);
            }
            hasher.finish()
        };
        if hash == last_queue {
            return Ok(None);
        }
        if let Ok(mut guard) = self.last_queue.write() {
            *guard = hash;
        }
        Ok(Some(songs))
    }

    async fn apply_remote_state(&self, state: &PlayerState) {
        self.remote_playing
            .store(state.is_playing, Ordering::Relaxed);

        let remote_track = state.current_track_id.as_ref().map(|t| t.id.clone());
        let track_changed = self
            .last_track_id
            .read()
            .map(|last| *last != remote_track)
            .unwrap_or(true);
        let poll_idx = self.poll_count.fetch_add(1, Ordering::Relaxed);
        let tracks = if track_changed || poll_idx.is_multiple_of(4) {
            match self.get_queue_if_changed().await {
                Ok(queue) => {
                    if let Ok(mut last) = self.last_track_id.write() {
                        *last = remote_track.clone();
                    }
                    queue
                }
                Err(_) => None,
            }
        } else {
            None
        };

        if !self.has_device() {
            return;
        }

        let volume = state
            .device
            .as_ref()
            .and_then(|d| d.volume_percent)
            .map(|pct| f64::from(pct) / 100.0);

        self.send_actions([PlaybackAction::SyncRemoteState(Box::new(
            RemotePlaybackState {
                tracks,
                track_id: remote_track,
                is_playing: state.is_playing,
                position_ms: state.progress_ms,
                repeat: state.repeat,
                shuffle: state.shuffle,
                volume,
            },
        ))
        .into()]);
    }

    /// Spotify can take this long to reflect a command in its state.
    const COMMAND_SETTLE: Duration = Duration::from_millis(1500);

    fn mark_command(&self) {
        if let Ok(mut last) = self.last_command.lock() {
            *last = Some(std::time::Instant::now());
        }
    }

    /// Whether a playback command was sent after (or shortly before) a poll
    /// that started at `request_started`.
    fn command_overlaps(&self, request_started: std::time::Instant) -> bool {
        self.last_command
            .lock()
            .ok()
            .and_then(|last| *last)
            .is_some_and(|sent| sent + Self::COMMAND_SETTLE > request_started)
    }

    pub fn has_device(&self) -> bool {
        self.device_id
            .read()
            .map(|it| it.is_some())
            .unwrap_or(false)
    }

    pub async fn sync_state(&self) {
        debug!("polling connect device...");
        let request_started = std::time::Instant::now();
        let player_state = self.api.get_player_state(load::transport()).await;
        let Ok(mut state) = player_state else {
            return;
        };
        if self.command_overlaps(request_started) {
            debug!("dropping a poll that raced a playback command");
            return;
        }
        if !self.has_device() {
            return;
        }
        let now_epoch_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .ok()
            .map(|d| d.as_millis() as u64);
        state.progress_ms = compensate_connect_progress(
            state.progress_ms,
            state.is_playing,
            request_started.elapsed(),
            state.timestamp_ms,
            now_epoch_ms,
        );
        if let Ok(mut last_state) = self.last_state.write() {
            *last_state = state.clone();
        }
        self.apply_remote_state(&state).await;

        let active_is_official = match (
            self.device_id.read().ok().and_then(|d| d.clone()),
            self.official_device_id.read().ok().and_then(|d| d.clone()),
        ) {
            (Some(active), Some(official)) => active == official,
            _ => false,
        };
        // Only treat the spawned official client as idle when it is not playing elsewhere;
        // never tear down or hijack playback on an external Connect speaker like Alexa.
        let official_playing = active_is_official && state.is_playing;
        if self
            .official
            .idle_for(official_playing, OFFICIAL_CLIENT_IDLE_LIMIT)
        {
            info!("official client idle for too long; shutting it down");
            self.official.stop();
            let _ = self.official_device_id.write().map(|mut d| d.take());
            if active_is_official {
                let _ = self.device_id.write().map(|mut d| d.take());
                self.send_actions([PlaybackAction::SwitchDevice(Device::Local).into()]);
            }
            if let Ok(devices) = self.api.get_devices(load::transport()).await {
                self.send_actions([PlaybackAction::SetAvailableDevices(devices).into()]);
            }
        }
    }

    async fn handle_player_load_in_context(
        &self,
        device_id: String,
        current_state: &PlayerState,
        command: ConnectCommand,
    ) -> Result<(), DomainError> {
        let ConnectCommand::PlayerLoadInContext {
            source,
            offset,
            song,
        } = command
        else {
            panic!("Illegal call");
        };
        let is_diff_song = current_state
            .current_track_id
            .as_ref()
            .map(|it| it.id != song)
            .unwrap_or(true);
        let is_paused = !current_state.is_playing;
        if is_diff_song {
            let context = source.spotify_uri().unwrap();
            self.api
                .player_play_in_context(&device_id, &context, offset)
                .await
        } else if is_paused {
            self.api.player_resume(&device_id).await
        } else {
            Ok(())
        }
    }

    async fn handle_player_load(
        &self,
        device_id: String,
        current_state: &PlayerState,
        command: ConnectCommand,
    ) -> Result<(), DomainError> {
        let ConnectCommand::PlayerLoad { songs, offset } = command else {
            panic!("Illegal call");
        };
        let Some(target_id) = songs.get(offset) else {
            return Ok(());
        };
        let is_diff_song = current_state
            .current_track_id
            .as_ref()
            .map(|it| &it.id != target_id)
            .unwrap_or(true);
        let is_paused = !current_state.is_playing;
        if is_diff_song {
            self.api
                .player_play_uris(
                    &device_id,
                    songs
                        .into_iter()
                        .map(|s| format!("spotify:track:{}", s))
                        .collect(),
                    offset,
                )
                .await
        } else if is_paused {
            self.api.player_resume(&device_id).await
        } else {
            Ok(())
        }
    }

    async fn handle_other_command(
        &self,
        device_id: String,
        command: ConnectCommand,
    ) -> Result<(), DomainError> {
        let state = self.last_state.read();
        let Ok(state) = state.as_deref() else {
            return Ok(());
        };
        match command {
            ConnectCommand::PlayerLoadInContext { .. } => {
                self.handle_player_load_in_context(device_id, state, command)
                    .await
            }
            ConnectCommand::PlayerLoad { .. } => {
                self.handle_player_load(device_id, state, command).await
            }
            ConnectCommand::PlayerResume if !state.is_playing => {
                self.api.player_resume(&device_id).await
            }
            ConnectCommand::PlayerPause if state.is_playing => {
                self.api.player_pause(&device_id).await
            }
            ConnectCommand::PlayerSeek(offset) => {
                self.api.player_seek(&device_id, offset as u32).await
            }
            ConnectCommand::PlayerRepeat(mode) => self.api.player_repeat(&device_id, mode).await,
            ConnectCommand::PlayerShuffle(shuffle) => {
                self.api.player_shuffle(&device_id, shuffle).await
            }
            ConnectCommand::PlayerSetVolume(volume) => {
                self.api.player_volume(&device_id, volume).await
            }
            _ => Ok(()),
        }
    }

    async fn use_official_client(&self) {
        if let Some(device) = self.wait_for_official_device(false, false).await {
            return self.switch_to(device).await;
        }
        self.send_actions([AppAction::ShowNotification(gettext(
            "Starting Spotify in the background…",
        ))]);
        if let Err(e) = self.official.spawn() {
            warn!("{e}");
            self.send_actions([
                AppAction::OfficialClientFailed,
                AppAction::ShowNotification(gettext(
                    "Could not start the Spotify app. Is it installed?",
                )),
            ]);
            return;
        }
        match self.wait_for_official_device(true, true).await {
            Some(device) => self.switch_to(device).await,
            None => {
                self.official.stop();
                self.send_actions([
                    AppAction::OfficialClientFailed,
                    AppAction::ShowNotification(gettext(
                        "The Spotify app did not show up as a device. Try again in a moment.",
                    )),
                ]);
            }
        }
    }

    async fn wait_for_official_device(
        &self,
        keep_trying: bool,
        allow_any_computer: bool,
    ) -> Option<crate::app::models::Device> {
        let deadline = std::time::Instant::now() + OFFICIAL_CLIENT_STARTUP_TIMEOUT;
        loop {
            if let Ok(devices) = self.api.get_devices(load::transport()).await {
                if let Some(device) =
                    official::find_local_official_device(&devices, allow_any_computer)
                {
                    self.send_actions([PlaybackAction::SetAvailableDevices(devices).into()]);
                    return Some(device);
                }
            }
            if !keep_trying || std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn switch_to(&self, device: crate::app::models::Device) {
        info!("playing on official client device '{}'", device.label);
        if let Ok(mut official_id) = self.official_device_id.write() {
            *official_id = Some(device.id.clone());
        }
        official::remember_official_device_id(&device.id);
        // Taking over playback can make the app ask for focus
        official::keep_hidden_workspace_closed(Duration::from_secs(20));
        // SwitchDevice makes the notifier send SetDevice + the current track.
        self.send_actions([PlaybackAction::SwitchDevice(Device::Connect(device)).into()]);
    }

    /// Whether `device_id` is the Spotify app on this computer.
    async fn is_local_official(&self, device_id: &str) -> bool {
        let known = |id: Option<String>| id.as_deref() == Some(device_id);
        if known(self.official_device_id.read().ok().and_then(|d| d.clone()))
            || known(official::remembered_official_device_id())
        {
            return true;
        }
        let from_state = self.last_state.read().ok().and_then(|s| s.device.clone());
        if from_state.is_some_and(|d| d.id == device_id && official::is_this_computer(&d.label)) {
            return true;
        }
        self.api
            .get_devices(load::transport())
            .await
            .is_ok_and(|devices| {
                devices
                    .iter()
                    .any(|d| d.id == device_id && official::is_this_computer(&d.label))
            })
    }

    /// The Spotify app on this computer stopped answering (closed, crashed or
    /// asleep): start it again in the background and replay `command` on it.
    /// Returns whether the command went through.
    async fn recover_local_official(&self, device_id: &str, command: ConnectCommand) -> bool {
        let wants_playback = matches!(
            command,
            ConnectCommand::PlayerResume
                | ConnectCommand::PlayerLoad { .. }
                | ConnectCommand::PlayerLoadInContext { .. }
        );
        if !wants_playback
            || !official::account_needs_official_client()
            || !self.is_local_official(device_id).await
        {
            return false;
        }
        {
            let Ok(mut last) = self.last_recovery.lock() else {
                return false;
            };
            if last.is_some_and(|t| t.elapsed() < OFFICIAL_RECOVERY_COOLDOWN) {
                return false;
            }
            *last = Some(std::time::Instant::now());
        }
        info!("official client unreachable; starting it again");
        self.send_actions([AppAction::ShowNotification(gettext(
            "Starting Spotify in the background…",
        ))]);
        if let Err(e) = self.official.spawn() {
            warn!("{e}");
            return false;
        }
        let Some(device) = self.wait_for_official_device(true, true).await else {
            return false;
        };
        official::keep_hidden_workspace_closed(Duration::from_secs(20));
        let _ = self.api.player_transfer(&device.id).await;
        tokio::time::sleep(Duration::from_millis(500)).await;
        if let Ok(mut id) = self.device_id.write() {
            *id = Some(device.id.clone());
        }
        let changed = device.id != device_id;
        let new_id = device.id.clone();
        if changed {
            // The UI follows; SwitchDevice re-sends the current track
            self.switch_to(device).await;
            return true;
        }
        if let Ok(mut official_id) = self.official_device_id.write() {
            *official_id = Some(new_id.clone());
        }
        official::remember_official_device_id(&new_id);
        self.handle_other_command(new_id, command).await.is_ok()
    }

    pub async fn handle_command(&self, command: ConnectCommand) -> Option<()> {
        let device_lost = match command {
            ConnectCommand::SetDevice(new_device_id) => {
                self.remote_playing.store(false, Ordering::Relaxed);
                if let Ok(mut q) = self.last_queue.write() {
                    *q = 0;
                }
                if let Ok(mut t) = self.last_track_id.write() {
                    *t = None;
                }
                self.poll_count.store(0, Ordering::Relaxed);
                if self.is_local_official(&new_device_id).await {
                    official::keep_hidden_workspace_closed(Duration::from_secs(20));
                }
                let _ = self.api.player_transfer(&new_device_id).await;
                self.device_id.write().ok()?.replace(new_device_id);
                tokio::time::sleep(Duration::from_millis(250)).await;
                self.sync_state().await;
                false
            }
            ConnectCommand::UseOfficialClient => {
                self.use_official_client().await;
                false
            }
            ConnectCommand::PlayerStop => {
                self.remote_playing.store(false, Ordering::Relaxed);
                let device_id = self.device_id.write().ok()?.take();
                if let Some(old_id) = device_id {
                    let _ = self.api.player_pause(&old_id).await;
                }
                false
            }
            _ => {
                let device_id = self.device_id.read().ok()?.clone();
                if let Some(device_id) = device_id {
                    self.mark_command();
                    let mut result = self
                        .handle_other_command(device_id.clone(), command.clone())
                        .await;
                    if matches!(result, Err(DomainError::NotFound { .. })) {
                        // Wake a sleeping Connect device (e.g. Alexa) by transferring and retrying once.
                        if self.api.player_transfer(&device_id).await.is_ok() {
                            tokio::time::sleep(Duration::from_millis(350)).await;
                            result = self
                                .handle_other_command(device_id.clone(), command.clone())
                                .await;
                        }
                    }
                    if matches!(result, Err(DomainError::NotFound { .. }))
                        && self.recover_local_official(&device_id, command).await
                    {
                        result = Ok(());
                    }
                    if matches!(result, Err(DomainError::RateLimited { .. })) {
                        self.send_actions([AppAction::ShowNotification(gettext(
                            // translators: This notification is shown when Spotify throttles requests.
                            "Rate limited by Spotify. Please wait a moment and try again.",
                        ))]);
                    } else if matches!(result, Err(DomainError::NotFound { .. })) {
                        self.send_actions([AppAction::ShowNotification(gettext(
                            // translators: Shown when a Spotify Connect speaker is asleep or unreachable.
                            "Spotify Connect device is asleep or inactive. Start playback on the device first.",
                        ))]);
                    }
                    false
                } else {
                    false
                }
            }
        };

        if device_lost {
            self.device_lost();
        }

        Some(())
    }
}

/// Compensates Spotify Connect playback progress using Spotify's snapshot
/// timestamp when within a sane clock-skew window (<= 10s), and at least
/// half of the request round-trip time, capped at 5s total.
fn compensate_connect_progress(
    progress_ms: u32,
    is_playing: bool,
    round_trip: std::time::Duration,
    timestamp_ms: Option<u64>,
    now_epoch_ms: Option<u64>,
) -> u32 {
    if !is_playing {
        return progress_ms;
    }
    let half_rtt_ms = (round_trip.as_millis() / 2).min(1500) as u32;
    let snapshot_age_ms = match (timestamp_ms, now_epoch_ms) {
        (Some(ts), Some(now)) if now >= ts && (now - ts) <= 10_000 => Some((now - ts) as u32),
        _ => None,
    };
    let compensation_ms = snapshot_age_ms.unwrap_or(0).max(half_rtt_ms).min(5_000);
    progress_ms.saturating_add(compensation_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compensate_connect_progress_uses_timestamp_and_half_rtt_safely() {
        assert_eq!(
            compensate_connect_progress(
                10_000,
                true,
                std::time::Duration::from_millis(240),
                None,
                None
            ),
            10_120
        );
        assert_eq!(
            compensate_connect_progress(
                10_000,
                false,
                std::time::Duration::from_millis(240),
                Some(1_000_000),
                Some(1_000_600)
            ),
            10_000
        );
        assert_eq!(
            compensate_connect_progress(
                10_000,
                true,
                std::time::Duration::from_millis(240),
                Some(1_000_000),
                Some(1_000_650)
            ),
            10_650
        );
        // Clock skew / stale timestamp (> 10s or negative) falls back to bounded half-RTT.
        assert_eq!(
            compensate_connect_progress(
                10_000,
                true,
                std::time::Duration::from_millis(240),
                Some(1_000_000),
                Some(1_030_000)
            ),
            10_120
        );
        assert_eq!(
            compensate_connect_progress(
                10_000,
                true,
                std::time::Duration::from_millis(240),
                Some(1_005_000),
                Some(1_000_000)
            ),
            10_120
        );
        assert_eq!(
            compensate_connect_progress(
                10_000,
                true,
                std::time::Duration::from_secs(10),
                Some(1_000_000),
                Some(1_008_000)
            ),
            15_000
        );
    }
}
