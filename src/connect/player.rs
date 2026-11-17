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
use canto_api::ApiService;
use canto_api::DomainError;

use super::official::{self, OfficialClient};
use super::soloist;
use super::spotify_mpris;

const OFFICIAL_CLIENT_STARTUP_TIMEOUT: Duration = Duration::from_secs(45);
const OFFICIAL_CLIENT_IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);
/// At most one automatic restart of the Spotify app per this long, so a
/// device that stays unreachable ends in a message instead of a loop.
const OFFICIAL_RECOVERY_COOLDOWN: Duration = Duration::from_secs(60);
/// How long Soloist gets to log in and show up as a device.
const SOLOIST_STARTUP_TIMEOUT: Duration = Duration::from_secs(25);
/// Soloist's volume when Canto starts it.
const SOLOIST_INITIAL_VOLUME: u8 = 50;

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
    /// Skip on the device itself, past what Canto knows of its queue.
    PlayerNext,
    PlayerPrevious,
}

pub struct ConnectPlayer {
    api: Arc<ApiService>,
    action_sender: UnboundedSender<AppAction>,
    device_id: RwLock<Option<String>>,
    official_device_id: RwLock<Option<String>>,
    soloist_device_id: RwLock<Option<String>>,
    /// Until when a switch to the player on this computer is under way:
    /// don't follow another device (an Alexa Spotify still calls active)
    /// meanwhile
    switching_here: std::sync::Mutex<Option<std::time::Instant>>,
    /// Until when what the device reports is old news: Canto's own Soloist
    /// just started, restored the account's last track (paused), and the
    /// track the user chose is on its way
    quiet_until: std::sync::Mutex<Option<std::time::Instant>>,
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
            soloist_device_id: Default::default(),
            switching_here: Default::default(),
            quiet_until: Default::default(),
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

    /// Whether the device is the player here, read locally (cheap to poll).
    pub fn polls_locally(&self) -> bool {
        self.device_id
            .read()
            .ok()
            .and_then(|d| d.clone())
            .is_some_and(|id| self.controls_locally(&id))
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

    async fn get_queue_if_changed(
        &self,
        current: Option<&str>,
    ) -> Result<Option<Vec<Track>>, DomainError> {
        let last_queue = *self.last_queue.read().ok().as_deref().unwrap_or(&0u64);
        let mut songs = queue_songs(self.api.get_player_queue(load::transport()).await?);
        // The queue endpoint sometimes comes back empty, or without the track
        // playing, while the app plays: fetch that track so it still shows
        if let Some(id) = current {
            if songs.first().is_none_or(|s| s.rri.id != id) {
                match self.api.get_track(id, load::transport()).await {
                    Ok(track) => songs.insert(0, track),
                    Err(e) => debug!("couldn't fetch the playing track: {e:?}"),
                }
            }
        }
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
            match self.get_queue_if_changed(remote_track.as_deref()).await {
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

        self.send_actions([
            PlaybackAction::SyncRemoteState(Box::new(RemotePlaybackState {
                tracks,
                track_id: remote_track,
                is_playing: state.is_playing,
                position_ms: state.progress_ms,
                repeat: state.repeat,
                shuffle: state.shuffle,
                volume,
            }))
            .into(),
        ]);
    }

    /// A player state built from the Spotify app's MPRIS report, keeping the
    /// device from the last Web API state.
    fn state_from_snapshot(&self, snapshot: spotify_mpris::Snapshot) -> PlayerState {
        let device = self.last_state.read().ok().and_then(|s| s.device.clone());
        let volume = snapshot.volume;
        PlayerState {
            is_playing: snapshot.playing,
            current_track_id: snapshot.track_id.map(|id| {
                canto_api::models::ResourceId::new(canto_api::models::Provider::Spotify, id)
            }),
            progress_ms: snapshot.position_ms,
            // Read just now: nothing to compensate for
            timestamp_ms: None,
            repeat: snapshot.repeat,
            shuffle: snapshot.shuffle,
            device: device.map(|mut d| {
                if let Some(volume) = volume {
                    d.volume_percent = Some((volume * 100.0).round() as u8);
                }
                d
            }),
        }
    }

    /// When nothing is being controlled yet but the Spotify app here is
    /// playing (Canto was reopened, or playback started in the app), take
    /// over: show what it plays and control it.
    /// With no device chosen yet, follow the one Spotify says is active (an
    /// Alexa, a phone), without moving its music anywhere.
    pub async fn attach_to_active_device(&self) {
        if self.has_device() || self.is_switching_here() {
            return;
        }
        let Ok(devices) = self.api.get_devices(load::transport()).await else {
            return;
        };
        let Some(active) = devices.iter().find(|d| d.is_active).cloned() else {
            return;
        };
        if official::is_this_computer(&active.label) {
            // The Spotify app here: only when it really plays
            self.attach_to_playing_official().await;
            return;
        }
        // Only when it really plays: an idle device Spotify still calls
        // active (an Alexa from yesterday) would take the next play away
        // from this computer
        let playing = self
            .api
            .get_player_state(load::transport())
            .await
            .is_ok_and(|state| {
                state.is_playing && state.device.as_ref().is_some_and(|d| d.id == active.id)
            });
        if !playing {
            return;
        }
        info!("following what plays on '{}'", active.label);
        self.send_actions([
            PlaybackAction::SetAvailableDevices(devices).into(),
            PlaybackAction::SwitchDevice(Device::Connect(active)).into(),
        ]);
    }

    pub async fn attach_to_playing_official(&self) {
        if self.has_device()
            || !official::account_needs_official_client()
            || !spotify_mpris::snapshot().is_some_and(|s| s.playing)
        {
            return;
        }
        if let Some(device) = self.wait_for_official_device(false, true).await {
            // The app here also reports "playing" while it only shows what
            // another device plays (an Alexa, a phone): taking over then
            // pulled the music away from that device and paused it
            if !device.is_active {
                debug!("the Spotify app here shows another device playing; leaving it there");
                return;
            }
            info!("the Spotify app here is playing; taking over its playback");
            self.switch_to(device).await;
        }
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
        if self.is_quiet() {
            debug!("waiting for the chosen track before reading the device");
            return;
        }
        debug!("polling connect device...");
        let request_started = std::time::Instant::now();
        let active = self.device_id.read().ok().and_then(|d| d.clone());
        let local = active
            .as_deref()
            .filter(|id| self.controls_locally(id))
            .and_then(|id| self.local_snapshot(id));
        // A Soloist that plays nothing (just started, the track not loaded
        // yet) has no news: applying it would take the track the user just
        // chose off the player
        if local.as_ref().is_some_and(|snap| snap.track_id.is_none())
            && active
                .as_deref()
                .is_some_and(|id| self.is_known_soloist(id))
        {
            debug!("Soloist plays nothing yet; keeping the current track");
            return;
        }
        let player_state = match local {
            // The app's own report: immediate, and no "device not found"
            Some(snapshot) => Ok(self.state_from_snapshot(snapshot)),
            None => self.api.get_player_state(load::transport()).await,
        };
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
            let result = self
                .retrying(&device_id, || {
                    self.api
                        .player_play_in_context(&device_id, &context, offset)
                })
                .await;
            self.fall_back_to_open_uri(&device_id, result, &song)
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
            let target = target_id.clone();
            let uris: Vec<String> = songs
                .into_iter()
                .map(|s| format!("spotify:track:{}", s))
                .collect();
            let result = self
                .retrying(&device_id, || {
                    self.api.player_play_uris(&device_id, uris.clone(), offset)
                })
                .await;
            self.fall_back_to_open_uri(&device_id, result, &target)
        } else if is_paused {
            self.api.player_resume(&device_id).await
        } else {
            Ok(())
        }
    }

    /// Runs a Web API play request, trying again twice when the Spotify app
    /// on this computer is the target: its Connect side often answers "not
    /// found" or a server error for a moment while the app plays fine.
    async fn retrying<F, Fut>(&self, device_id: &str, request: F) -> Result<(), DomainError>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<(), DomainError>>,
    {
        let mut result = request().await;
        if !self.controls_locally(device_id) {
            return result;
        }
        for wait_ms in [600, 1500] {
            let retry = match &result {
                Ok(()) | Err(DomainError::RateLimited { .. }) => false,
                Err(_) => true,
            };
            if !retry {
                break;
            }
            debug!("play request failed ({result:?}); trying again in {wait_ms} ms");
            tokio::time::sleep(Duration::from_millis(wait_ms)).await;
            result = request().await;
        }
        result
    }

    /// When the Web API still can't start `track_id` on the Spotify app here,
    /// opens it over MPRIS: it plays, outside its album or playlist.
    fn fall_back_to_open_uri(
        &self,
        device_id: &str,
        result: Result<(), DomainError>,
        track_id: &str,
    ) -> Result<(), DomainError> {
        if result.is_ok() || !self.controls_locally(device_id) {
            return result;
        }
        warn!("Web API couldn't start the track ({result:?}); opening it on the player here");
        let uri = format!("spotify:track:{track_id}");
        let opened = if self.is_known_soloist(device_id) {
            soloist::open_uri(&uri)
        } else {
            spotify_mpris::open_uri(&uri)
        };
        match opened {
            Ok(()) => {
                self.remember_playing(&Ok(()), true);
                Ok(())
            }
            Err(e) => {
                warn!("opening it here failed too: {e}");
                result
            }
        }
    }

    /// After a successful play or pause, the known state follows it.
    fn remember_playing(&self, result: &Result<(), DomainError>, playing: bool) {
        if result.is_ok() {
            if let Ok(mut state) = self.last_state.write() {
                state.is_playing = playing;
            }
            self.remote_playing.store(playing, Ordering::Relaxed);
        }
    }

    async fn handle_other_command(
        &self,
        device_id: String,
        command: ConnectCommand,
    ) -> Result<(), DomainError> {
        if self.controls_locally(&device_id) {
            if let Some(result) = self.try_local_command(&device_id, &command) {
                match command {
                    ConnectCommand::PlayerResume => self.remember_playing(&result, true),
                    ConnectCommand::PlayerPause => self.remember_playing(&result, false),
                    _ => {}
                }
                return result;
            }
        }
        // Always sent: the last polled state can be seconds old, and
        // skipping on it dropped real presses. Handled before the state lock,
        // which `remember_playing` takes for writing.
        match command {
            ConnectCommand::PlayerResume => {
                let result = self.api.player_resume(&device_id).await;
                self.remember_playing(&result, true);
                return result;
            }
            ConnectCommand::PlayerPause => {
                let result = self.api.player_pause(&device_id).await;
                self.remember_playing(&result, false);
                return result;
            }
            _ => {}
        }
        // A copy: holding the lock across the awaits below deadlocked the
        // whole Connect thread once a fallback needed to write the state
        let Some(state) = self.last_state.read().ok().map(|s| s.clone()) else {
            return Ok(());
        };
        let state = &state;
        match command {
            ConnectCommand::PlayerLoadInContext { .. } => {
                let result = self
                    .handle_player_load_in_context(device_id, state, command)
                    .await;
                self.remember_playing(&result, true);
                // A moment for the device to switch tracks
                self.set_quiet(Some(Duration::from_millis(1500)));
                result
            }
            ConnectCommand::PlayerLoad { .. } => {
                let result = self.handle_player_load(device_id, state, command).await;
                self.remember_playing(&result, true);
                // A moment for the device to switch tracks
                self.set_quiet(Some(Duration::from_millis(1500)));
                result
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
        self.set_switching_here(Some(Duration::from_secs(120)));
        self.use_player_here().await;
        // The switch itself (SetDevice) is queued right behind: a little
        // longer, in case it is
        self.set_switching_here(Some(Duration::from_secs(5)));
    }

    fn set_switching_here(&self, for_: Option<Duration>) {
        if let Ok(mut until) = self.switching_here.lock() {
            *until = for_.map(|d| std::time::Instant::now() + d);
        }
    }

    fn set_quiet(&self, for_: Option<Duration>) {
        if let Ok(mut until) = self.quiet_until.lock() {
            *until = for_.map(|d| std::time::Instant::now() + d);
        }
    }

    fn is_quiet(&self) -> bool {
        self.quiet_until
            .lock()
            .ok()
            .and_then(|until| *until)
            .is_some_and(|until| std::time::Instant::now() < until)
    }

    fn is_switching_here(&self) -> bool {
        self.switching_here
            .lock()
            .ok()
            .and_then(|until| *until)
            .is_some_and(|until| std::time::Instant::now() < until)
    }

    async fn use_player_here(&self) {
        if self.use_soloist().await {
            return;
        }
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

    /// Plays through Soloist (Spotify's own headless player) when it's on,
    /// installed and has a key, starting it if needed. Returns whether it
    /// took over; if not, the hidden Spotify app is the fallback.
    async fn use_soloist(&self) -> bool {
        if !soloist::is_enabled() || soloist::binary().is_none() {
            return false;
        }
        if !soloist::is_running() {
            let Some(key) = canto_auth::soloist_key::load().await else {
                info!("no Soloist key saved; playing through the Spotify app instead");
                return false;
            };
            let started =
                tokio::task::spawn_blocking(move || soloist::spawn(&key, SOLOIST_INITIAL_VOLUME))
                    .await;
            match started {
                Ok(Ok(())) => {}
                Ok(Err(e)) => {
                    warn!("{e}");
                    return false;
                }
                Err(e) => {
                    warn!("starting Soloist panicked: {e}");
                    return false;
                }
            }
        }
        match self.wait_for_soloist_device().await {
            Some(device) => {
                self.switch_to_soloist(device).await;
                true
            }
            None => {
                warn!("Soloist didn't show up as a device; using the Spotify app instead");
                false
            }
        }
    }

    async fn wait_for_soloist_device(&self) -> Option<crate::app::models::Device> {
        let deadline = std::time::Instant::now() + SOLOIST_STARTUP_TIMEOUT;
        let mut activated = false;
        loop {
            if let Ok(devices) = self.api.get_devices(load::transport()).await {
                let remembered = soloist::remembered_device_id();
                let found = devices
                    .iter()
                    .find(|d| remembered.as_deref() == Some(d.id.as_str()))
                    .or_else(|| devices.iter().find(|d| d.label == soloist::DEVICE_NAME))
                    .cloned();
                if let Some(device) = found {
                    self.send_actions([PlaybackAction::SetAvailableDevices(devices).into()]);
                    return Some(device);
                }
            }
            // Logged in but not listed yet: becoming the active device
            // makes Spotify list it
            if !activated && soloist::status().is_some_and(|s| s.logged_in) {
                activated = soloist::activate().is_ok();
            }
            if std::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn switch_to_soloist(&self, device: crate::app::models::Device) {
        info!("playing through Soloist ('{}')", device.label);
        if let Ok(mut id) = self.soloist_device_id.write() {
            *id = Some(device.id.clone());
        }
        soloist::remember_device_id(&device.id);
        // The hidden app isn't needed any more
        self.official.stop();
        let _ = self.official_device_id.write().map(|mut d| d.take());
        // SwitchDevice makes the notifier send SetDevice + the current track.
        self.send_actions([PlaybackAction::SwitchDevice(Device::Connect(device)).into()]);
    }

    /// Whether `device_id` is Canto's own Soloist.
    fn is_known_soloist(&self, device_id: &str) -> bool {
        let known = |id: Option<String>| id.as_deref() == Some(device_id);
        known(self.soloist_device_id.read().ok().and_then(|d| d.clone()))
            || known(soloist::remembered_device_id())
            || self
                .last_state
                .read()
                .ok()
                .and_then(|s| s.device.clone())
                .is_some_and(|d| d.id == device_id && d.label == soloist::DEVICE_NAME)
    }

    /// What the player here reports, immediately and without the Web API.
    fn local_snapshot(&self, device_id: &str) -> Option<spotify_mpris::Snapshot> {
        if self.is_known_soloist(device_id) {
            soloist::snapshot()
        } else {
            spotify_mpris::snapshot()
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

    /// Whether `device_id` is known (without asking Spotify) to be the
    /// Spotify app on this computer.
    fn is_known_official(&self, device_id: &str) -> bool {
        let known = |id: Option<String>| id.as_deref() == Some(device_id);
        known(self.official_device_id.read().ok().and_then(|d| d.clone()))
            || known(official::remembered_official_device_id())
            || self
                .last_state
                .read()
                .ok()
                .and_then(|s| s.device.clone())
                .is_some_and(|d| d.id == device_id && official::is_this_computer(&d.label))
    }

    /// Whether commands for `device_id` can go straight to the player here
    /// (Soloist, or the Spotify app over MPRIS) instead of the Web API.
    fn controls_locally(&self, device_id: &str) -> bool {
        if self.is_known_soloist(device_id) {
            return soloist::is_running();
        }
        self.is_known_official(device_id) && spotify_mpris::is_running()
    }

    /// Plays, pauses, seeks, repeats, shuffles or sets the volume over MPRIS.
    /// `None` when the command isn't one of those or MPRIS failed, so the
    /// caller falls back to the Web API.
    fn try_local_command(
        &self,
        device_id: &str,
        command: &ConnectCommand,
    ) -> Option<Result<(), DomainError>> {
        if self.is_known_soloist(device_id) {
            let result = match command {
                ConnectCommand::PlayerResume => soloist::play(),
                ConnectCommand::PlayerPause => soloist::pause(),
                ConnectCommand::PlayerNext => soloist::next(),
                ConnectCommand::PlayerPrevious => soloist::previous(),
                ConnectCommand::PlayerSeek(offset) => soloist::seek(*offset as u32),
                ConnectCommand::PlayerRepeat(mode) => soloist::set_repeat(*mode),
                ConnectCommand::PlayerShuffle(shuffle) => soloist::set_shuffle(*shuffle),
                ConnectCommand::PlayerSetVolume(volume) => {
                    soloist::set_volume(f64::from(*volume) / 100.0)
                }
                _ => return None,
            };
            return match result {
                Ok(()) => {
                    debug!("{command:?} sent to Soloist");
                    Some(Ok(()))
                }
                Err(e) => {
                    warn!("Soloist {command:?} failed ({e}); using the Web API");
                    None
                }
            };
        }
        let result = match command {
            ConnectCommand::PlayerResume => spotify_mpris::play(),
            ConnectCommand::PlayerPause => spotify_mpris::pause(),
            ConnectCommand::PlayerNext => spotify_mpris::next(),
            ConnectCommand::PlayerPrevious => spotify_mpris::previous(),
            ConnectCommand::PlayerSeek(offset) => {
                let item = spotify_mpris::snapshot()?.item_path?;
                spotify_mpris::set_position(&item, *offset as u32)
            }
            ConnectCommand::PlayerRepeat(mode) => spotify_mpris::set_repeat(*mode),
            ConnectCommand::PlayerShuffle(shuffle) => spotify_mpris::set_shuffle(*shuffle),
            ConnectCommand::PlayerSetVolume(volume) => {
                spotify_mpris::set_volume(f64::from(*volume) / 100.0)
            }
            _ => return None,
        };
        match result {
            Ok(()) => {
                debug!("{command:?} sent to the Spotify app over MPRIS");
                Some(Ok(()))
            }
            Err(e) => {
                warn!("MPRIS {command:?} failed ({e}); using the Web API");
                None
            }
        }
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

    /// At most one automatic restart per [`OFFICIAL_RECOVERY_COOLDOWN`].
    fn recovery_allowed(&self) -> bool {
        let Ok(mut last) = self.last_recovery.lock() else {
            return false;
        };
        if last.is_some_and(|t| t.elapsed() < OFFICIAL_RECOVERY_COOLDOWN) {
            return false;
        }
        *last = Some(std::time::Instant::now());
        true
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
        if self.is_known_soloist(device_id) {
            // Soloist stopped (or crashed): start it again
            if !wants_playback || soloist::is_running() || !self.recovery_allowed() {
                return false;
            }
            info!("Soloist unreachable; starting it again");
            return self.use_soloist().await;
        }
        if !wants_playback
            || !official::account_needs_official_client()
            || spotify_mpris::is_running()
            || !self.is_local_official(device_id).await
        {
            // A running app that won't answer isn't fixed by starting it again
            return false;
        }
        if !self.recovery_allowed() {
            return false;
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
        let _ = self.api.player_transfer(&device.id, false).await;
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
                self.set_switching_here(None);
                // Moving to another device keeps the music playing there
                let was_playing = self.remote_playing.load(Ordering::Relaxed)
                    || self.last_state.read().is_ok_and(|s| s.is_playing);
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
                // Already the active device: only follow it. Transferring to
                // it anyway (with nothing known playing here) paused it.
                let already_active =
                    self.api
                        .get_devices(load::transport())
                        .await
                        .is_ok_and(|devices| {
                            devices.iter().any(|d| d.id == new_device_id && d.is_active)
                        });
                // Canto's own Soloist with nothing to carry over: the play
                // that follows targets it directly. A transfer would bring
                // back the account's last session (another track) first
                let own_idle_soloist = !was_playing && self.is_known_soloist(&new_device_id);
                if !already_active && !own_idle_soloist {
                    let _ = self.api.player_transfer(&new_device_id, was_playing).await;
                }
                self.device_id.write().ok()?.replace(new_device_id);
                if own_idle_soloist {
                    // What it reports now is the account's last track, not
                    // the one about to be played: wait for that
                    self.set_quiet(Some(Duration::from_secs(8)));
                } else {
                    tokio::time::sleep(Duration::from_millis(250)).await;
                    self.sync_state().await;
                }
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
                        if self.api.player_transfer(&device_id, false).await.is_ok() {
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
