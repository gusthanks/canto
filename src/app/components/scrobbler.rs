//! App-layer Last.fm and ListenBrainz scrobbling controller.
//!
//! Strictly zero overhead (no timers, no HTTP requests, no disk queue work)
//! when `scrobble-enabled` is false or no destination credentials are saved.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use gio::prelude::*;
use canto_api::scrobbler::{
    LastFmClient, ListenBrainzClient, PendingScrobble, RetryQueue, ScrobbleAction,
    ScrobbleTarget, ScrobbleTrack, ScrobbleTrackMetadata, ScrobbleTracker,
};
use canto_api::DomainError;
use canto_config::settings::{
    LASTFM_API_KEY, LASTFM_API_SECRET, LASTFM_SESSION_KEY, LISTENBRAINZ_TOKEN, SCROBBLE_ENABLED,
    SCROBBLE_RESPECT_REMOTE, SETTINGS,
};

use crate::app::components::EventListener;
use crate::app::state::{Device, PlaybackEvent};
use crate::app::{AppEvent, AppModel};

#[derive(Clone, Default)]
struct Credentials {
    enabled: bool,
    respect_remote: bool,
    lastfm_api_key: String,
    lastfm_api_secret: String,
    lastfm_session_key: String,
    listenbrainz_token: String,
}

impl Credentials {
    fn from_gsettings(s: &gio::Settings) -> Self {
        Self {
            enabled: s.boolean(SCROBBLE_ENABLED),
            respect_remote: s.boolean(SCROBBLE_RESPECT_REMOTE),
            lastfm_api_key: s.string(LASTFM_API_KEY).to_string(),
            lastfm_api_secret: s.string(LASTFM_API_SECRET).to_string(),
            lastfm_session_key: s.string(LASTFM_SESSION_KEY).to_string(),
            listenbrainz_token: s.string(LISTENBRAINZ_TOKEN).to_string(),
        }
    }

    fn has_lastfm(&self) -> bool {
        !self.lastfm_api_key.trim().is_empty()
            && !self.lastfm_api_secret.trim().is_empty()
            && !self.lastfm_session_key.trim().is_empty()
    }

    fn has_listenbrainz(&self) -> bool {
        !self.listenbrainz_token.trim().is_empty()
    }

    fn is_active(&self) -> bool {
        self.enabled && (self.has_lastfm() || self.has_listenbrainz())
    }
}

struct Inner {
    app_model: Rc<AppModel>,
    _settings: gio::Settings,
    creds: RefCell<Credentials>,
    tracker: RefCell<ScrobbleTracker>,
    retry_queue: Arc<tokio::sync::Mutex<RetryQueue>>,
    tick_source: RefCell<Option<glib::SourceId>>,
    flushing: Cell<bool>,
}

pub struct ScrobblerController {
    inner: Rc<Inner>,
}

impl ScrobblerController {
    pub fn new(app_model: Rc<AppModel>) -> Self {
        let settings = gio::Settings::new(SETTINGS);
        let creds = Credentials::from_gsettings(&settings);
        let retry_queue = Arc::new(tokio::sync::Mutex::new(RetryQueue::load_default()));
        let inner = Rc::new(Inner {
            app_model,
            _settings: settings.clone(),
            creds: RefCell::new(creds),
            tracker: RefCell::new(ScrobbleTracker::new()),
            retry_queue,
            tick_source: RefCell::new(None),
            flushing: Cell::new(false),
        });

        let reload_keys = [
            SCROBBLE_ENABLED,
            SCROBBLE_RESPECT_REMOTE,
            LASTFM_API_KEY,
            LASTFM_API_SECRET,
            LASTFM_SESSION_KEY,
            LISTENBRAINZ_TOKEN,
        ];
        for key in reload_keys {
            let weak = Rc::downgrade(&inner);
            settings.connect_changed(Some(key), move |s, _| {
                if let Some(inner) = weak.upgrade() {
                    inner.on_settings_changed(s);
                }
            });
        }

        // Initial retry flush is deferred to AppEvent::Started after the Tokio
        // runtime is entered by App::attach.
        Self { inner }
    }
}

impl Inner {
    fn on_settings_changed(self: &Rc<Self>, settings: &gio::Settings) {
        let new_creds = Credentials::from_gsettings(settings);
        let active = new_creds.is_active();
        *self.creds.borrow_mut() = new_creds;

        if !active {
            self.stop_tick();
            self.tracker.borrow_mut().reset();
            return;
        }

        self.sync_from_state();
        self.spawn_flush_retry_queue();
    }

    fn stop_tick(&self) {
        if let Some(id) = self.tick_source.borrow_mut().take() {
            id.remove();
        }
    }

    fn ensure_tick(self: &Rc<Self>, is_playing: bool) {
        let active = self.creds.borrow().is_active();
        if !active || !is_playing {
            self.stop_tick();
            return;
        }
        if self.tick_source.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let source = glib::timeout_add_seconds_local(1, move || {
            let Some(inner) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
            if !inner.creds.borrow().is_active() {
                *inner.tick_source.borrow_mut() = None;
                return glib::ControlFlow::Break;
            }
            inner.sync_from_state();
            if inner.app_model.get_state().playback.is_playing() {
                glib::ControlFlow::Continue
            } else {
                *inner.tick_source.borrow_mut() = None;
                glib::ControlFlow::Break
            }
        });
        *self.tick_source.borrow_mut() = Some(source);
    }

    fn sync_from_state(self: &Rc<Self>) {
        let creds = self.creds.borrow().clone();
        if !creds.is_active() {
            self.stop_tick();
            return;
        }

        let (meta, is_playing, position_ms, is_remote) = {
            let state = self.app_model.get_state();
            let playback = &state.playback;
            let track = playback.current_song().map(|t| {
                let artist = t
                    .artists
                    .iter()
                    .map(|a| a.name.as_str())
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>()
                    .join(", ");
                ScrobbleTrackMetadata {
                    id: t.rri.id.clone(),
                    title: t.title.clone(),
                    artist,
                    album: t.album.as_ref().map(|a| a.name.clone()),
                    album_artist: t.artists.first().map(|a| a.name.clone()),
                    duration_ms: t.duration_ms,
                }
            });
            let is_playing = playback.is_playing();
            let pos_ms = playback.seek_position().min(u32::MAX as u64) as u32;
            let is_remote = matches!(playback.current_device(), Device::Connect(_));
            (track, is_playing, pos_ms, is_remote)
        };

        self.ensure_tick(is_playing);

        let now_ms = now_unix_ms();
        let actions = self.tracker.borrow_mut().update(
            meta.as_ref(),
            is_playing,
            position_ms,
            is_remote,
            creds.respect_remote,
            now_ms,
        );

        for action in actions {
            self.dispatch_action(&creds, action);
        }
    }

    fn dispatch_action(self: &Rc<Self>, creds: &Credentials, action: ScrobbleAction) {
        let client = self.app_model.api().http_client().clone();
        let creds = creds.clone();
        let queue = Arc::clone(&self.retry_queue);

        match action {
            ScrobbleAction::SendNowPlaying(track) => {
                tokio::spawn(async move {
                    if creds.has_lastfm() {
                        if let Err(e) = LastFmClient::update_now_playing(
                            &client,
                            &creds.lastfm_api_key,
                            &creds.lastfm_api_secret,
                            &creds.lastfm_session_key,
                            &track,
                        )
                        .await
                        {
                            debug!("scrobbler: Last.fm now playing failed: {e}");
                        }
                    }
                    if creds.has_listenbrainz() {
                        if let Err(e) = ListenBrainzClient::submit_playing_now(
                            &client,
                            &creds.listenbrainz_token,
                            &track,
                        )
                        .await
                        {
                            debug!("scrobbler: ListenBrainz playing_now failed: {e}");
                        }
                    }
                });
            }
            ScrobbleAction::SubmitScrobble(track) => {
                let weak = Rc::downgrade(self);
                glib::MainContext::default().spawn_local(async move {
                    let mut flushed_ok = false;
                    if creds.has_lastfm() {
                        let res = send_lastfm(&client, &creds, &track).await;
                        match res {
                            Ok(()) => flushed_ok = true,
                            Err(e) if e.is_transient() => {
                                queue.lock().await.enqueue(ScrobbleTarget::LastFm, track.clone());
                            }
                            Err(e) => warn!("scrobbler: Last.fm rejected scrobble: {e}"),
                        }
                    }
                    if creds.has_listenbrainz() {
                        let res = ListenBrainzClient::submit_scrobble(
                            &client,
                            &creds.listenbrainz_token,
                            &track,
                        )
                        .await;
                        match res {
                            Ok(()) => flushed_ok = true,
                            Err(e) if e.is_transient() => {
                                queue
                                    .lock()
                                    .await
                                    .enqueue(ScrobbleTarget::ListenBrainz, track.clone());
                            }
                            Err(e) => warn!("scrobbler: ListenBrainz rejected scrobble: {e}"),
                        }
                    }
                    if flushed_ok {
                        if let Some(inner) = weak.upgrade() {
                            inner.spawn_flush_retry_queue();
                        }
                    }
                });
            }
        }
    }

    fn spawn_flush_retry_queue(self: &Rc<Self>) {
        if self.flushing.get() {
            return;
        }
        let creds = self.creds.borrow().clone();
        if !creds.is_active() {
            return;
        }
        self.flushing.set(true);
        let client = self.app_model.api().http_client().clone();
        let queue = Arc::clone(&self.retry_queue);
        let weak = Rc::downgrade(self);

        glib::MainContext::default().spawn_local(async move {
            let pending = {
                let mut guard = queue.lock().await;
                if guard.is_empty() {
                    Vec::new()
                } else {
                    guard.take_all()
                }
            };

            if !pending.is_empty() {
                let remaining = flush_pending_items(&client, &creds, pending).await;
                if !remaining.is_empty() {
                    queue.lock().await.requeue_remaining(remaining);
                }
            }

            if let Some(inner) = weak.upgrade() {
                inner.flushing.set(false);
            }
        });
    }
}

async fn send_lastfm(
    client: &canto_api::HttpClient,
    creds: &Credentials,
    track: &ScrobbleTrack,
) -> Result<(), DomainError> {
    LastFmClient::scrobble(
        client,
        &creds.lastfm_api_key,
        &creds.lastfm_api_secret,
        &creds.lastfm_session_key,
        track,
    )
    .await
}

#[derive(Default)]
struct TransientTargets {
    lastfm: bool,
    listenbrainz: bool,
}

impl TransientTargets {
    fn is_blocked(&self, target: ScrobbleTarget) -> bool {
        match target {
            ScrobbleTarget::LastFm => self.lastfm,
            ScrobbleTarget::ListenBrainz => self.listenbrainz,
        }
    }

    fn mark(&mut self, target: ScrobbleTarget) {
        match target {
            ScrobbleTarget::LastFm => self.lastfm = true,
            ScrobbleTarget::ListenBrainz => self.listenbrainz = true,
        }
    }
}

async fn flush_pending_items(
    client: &canto_api::HttpClient,
    creds: &Credentials,
    pending: Vec<PendingScrobble>,
) -> Vec<PendingScrobble> {
    let mut keep = Vec::new();
    let mut blocked = TransientTargets::default();

    for mut item in pending {
        if blocked.is_blocked(item.target) {
            keep.push(item);
            continue;
        }
        let result = match item.target {
            ScrobbleTarget::LastFm => {
                if !creds.has_lastfm() {
                    keep.push(item);
                    continue;
                }
                send_lastfm(client, creds, &item.track).await
            }
            ScrobbleTarget::ListenBrainz => {
                if !creds.has_listenbrainz() {
                    keep.push(item);
                    continue;
                }
                ListenBrainzClient::submit_scrobble(client, &creds.listenbrainz_token, &item.track)
                    .await
            }
        };

        match result {
            Ok(()) => {}
            Err(e) if e.is_transient() => {
                item.attempts = item.attempts.saturating_add(1);
                blocked.mark(item.target);
                keep.push(item);
            }
            Err(e) => {
                warn!("scrobbler: dropping permanently rejected queued item: {e}");
            }
        }
    }

    keep
}

impl EventListener for ScrobblerController {
    fn on_event(&mut self, event: &AppEvent) {
        if !self.inner.creds.borrow().is_active() {
            return;
        }
        match event {
            AppEvent::Started => {
                self.inner.spawn_flush_retry_queue();
            }
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlaybackStopped
                | PlaybackEvent::PlayStateSynced(_)
                | PlaybackEvent::SeekSynced(_)
                | PlaybackEvent::TrackSeeked(_)
                | PlaybackEvent::SwitchedDevice(_),
            ) => {
                self.inner.sync_from_state();
            }
            _ => {}
        }
    }
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis()
        .min(u128::from(u64::MAX)) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transient_target_blocks_are_tracked_independently() {
        let mut blocked = TransientTargets::default();
        assert!(!blocked.is_blocked(ScrobbleTarget::LastFm));
        assert!(!blocked.is_blocked(ScrobbleTarget::ListenBrainz));

        blocked.mark(ScrobbleTarget::LastFm);
        assert!(blocked.is_blocked(ScrobbleTarget::LastFm));
        assert!(!blocked.is_blocked(ScrobbleTarget::ListenBrainz));
    }
}
