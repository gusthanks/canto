//! Opt-in Visual & Audio Bridge for external visualizers (e.g. Chaosphere).
//!
//! Strictly dormant when `visual-bridge-enabled` is `false` (default):
//! zero timers, zero disk I/O, and zero audio-thread metering cost.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use canto_audio_engine::LevelMeterController;
use canto_config::settings::{SETTINGS, VISUAL_BRIDGE_ENABLED};
use gio::prelude::*;

use crate::app::components::EventListener;
use crate::app::state::PlaybackEvent;
use crate::app::theme::{ThemeEngine, ThemeSnapshot};
use crate::app::{AppEvent, AppModel};

const BRIDGE_INTERVAL_MS: u64 = 66; // ~15 Hz while enabled and playing

#[derive(Clone, Debug)]
pub struct VisualBridgeTrack {
    pub id: String,
    pub title: String,
    pub artists: Vec<String>,
    pub album: Option<String>,
    pub duration_ms: u32,
    pub progress_ms: u64,
    pub artwork_url: Option<String>,
}

#[derive(Clone, Debug)]
pub struct VisualBridgeAudio {
    pub rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug)]
pub struct VisualBridgeTheme {
    pub kind: String,
    pub is_dark: bool,
    pub background: String,
    pub surface: String,
    pub foreground: String,
    pub accent: String,
    pub highlight: String,
}

impl From<ThemeSnapshot> for VisualBridgeTheme {
    fn from(snap: ThemeSnapshot) -> Self {
        Self {
            kind: snap.kind.to_string(),
            is_dark: snap.is_dark,
            background: snap.background,
            surface: snap.surface,
            foreground: snap.foreground,
            accent: snap.accent,
            highlight: snap.highlight,
        }
    }
}

#[derive(Clone, Debug)]
pub struct VisualBridgeState {
    pub app: &'static str,
    pub version: u32,
    pub active: bool,
    pub playing: bool,
    pub track: Option<VisualBridgeTrack>,
    pub audio: VisualBridgeAudio,
    pub theme: VisualBridgeTheme,
    pub updated_at_ms: u64,
}

impl VisualBridgeState {
    pub fn to_json_value(&self) -> serde_json::Value {
        let track_val = self.track.as_ref().map(|t| {
            serde_json::json!({
                "id": t.id,
                "title": t.title,
                "artists": t.artists,
                "album": t.album,
                "duration_ms": t.duration_ms,
                "progress_ms": t.progress_ms,
                "artwork_url": t.artwork_url,
            })
        });
        serde_json::json!({
            "app": self.app,
            "version": self.version,
            "active": self.active,
            "playing": self.playing,
            "track": track_val,
            "audio": {
                "rms": self.audio.rms,
                "peak": self.audio.peak,
            },
            "theme": {
                "kind": self.theme.kind,
                "is_dark": self.theme.is_dark,
                "background": self.theme.background,
                "surface": self.theme.surface,
                "foreground": self.theme.foreground,
                "accent": self.theme.accent,
                "highlight": self.theme.highlight,
            },
            "updated_at_ms": self.updated_at_ms,
        })
    }
}

pub fn runtime_state_path() -> PathBuf {
    let base = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(glib::user_runtime_dir);
    base.join("canto").join("state.json")
}

pub fn write_atomic_state(path: &Path, state: &VisualBridgeState) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let bytes = serde_json::to_vec(&state.to_json_value())
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let tmp = path.with_extension(format!("tmp.{}", std::process::id()));
    std::fs::write(&tmp, &bytes)?;
    if let Err(e) = std::fs::rename(&tmp, path) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e);
    }
    Ok(())
}

struct Inner {
    app_model: Rc<AppModel>,
    theme_engine: Rc<ThemeEngine>,
    level_meter: LevelMeterController,
    _settings: gio::Settings,
    enabled: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    state_path: PathBuf,
}

pub struct VisualBridge {
    inner: Rc<Inner>,
}

impl VisualBridge {
    pub fn new(
        app_model: Rc<AppModel>,
        theme_engine: Rc<ThemeEngine>,
        level_meter: LevelMeterController,
    ) -> Self {
        let settings = gio::Settings::new(SETTINGS);
        let enabled = settings.boolean(VISUAL_BRIDGE_ENABLED);
        level_meter.set_enabled(enabled);

        let inner = Rc::new(Inner {
            app_model,
            theme_engine: Rc::clone(&theme_engine),
            level_meter,
            _settings: settings.clone(),
            enabled: Cell::new(enabled),
            timer: RefCell::new(None),
            state_path: runtime_state_path(),
        });

        let weak = Rc::downgrade(&inner);
        settings.connect_changed(Some(VISUAL_BRIDGE_ENABLED), move |s, key| {
            if let Some(inner) = weak.upgrade() {
                inner.set_enabled(s.boolean(key));
            }
        });

        let weak_theme = Rc::downgrade(&inner);
        theme_engine.subscribe(Rc::new(move || {
            if let Some(inner) = weak_theme.upgrade() {
                if inner.enabled.get() {
                    inner.write_snapshot(true);
                }
            }
        }));

        if enabled {
            inner.sync_timer_and_write();
        }

        Self { inner }
    }
}

impl Inner {
    fn set_enabled(self: &Rc<Self>, enabled: bool) {
        let was_enabled = self.enabled.replace(enabled);
        self.level_meter.set_enabled(enabled);
        if !enabled {
            self.stop_timer();
            if was_enabled {
                // Publish a clean inactive/zeroed frame once when turned off.
                self.write_snapshot(false);
            }
            return;
        }
        self.sync_timer_and_write();
    }

    fn stop_timer(&self) {
        if let Some(id) = self.timer.borrow_mut().take() {
            id.remove();
        }
    }

    fn sync_timer_and_write(self: &Rc<Self>) {
        if !self.enabled.get() {
            self.stop_timer();
            return;
        }
        let is_playing = self.app_model.get_state().playback.is_playing();
        if !is_playing {
            self.level_meter.reset();
            self.stop_timer();
        } else if self.timer.borrow().is_none() {
            let weak = Rc::downgrade(self);
            let source =
                glib::timeout_add_local(Duration::from_millis(BRIDGE_INTERVAL_MS), move || {
                    let Some(inner) = weak.upgrade() else {
                        return glib::ControlFlow::Break;
                    };
                    if !inner.enabled.get() || !inner.app_model.get_state().playback.is_playing() {
                        inner.level_meter.reset();
                        *inner.timer.borrow_mut() = None;
                        return glib::ControlFlow::Break;
                    }
                    inner.write_snapshot(true);
                    glib::ControlFlow::Continue
                });
            *self.timer.borrow_mut() = Some(source);
        }
        self.write_snapshot(true);
    }

    fn write_snapshot(&self, active: bool) {
        let snapshot = self.build_snapshot(active);
        if let Err(e) = write_atomic_state(&self.state_path, &snapshot) {
            debug!(
                "visual_bridge: failed writing {}: {e}",
                self.state_path.display()
            );
        }
    }

    fn build_snapshot(&self, active: bool) -> VisualBridgeState {
        let (playing, track) = if active {
            let state = self.app_model.get_state();
            let playback = &state.playback;
            let playing = playback.is_playing();
            let progress_ms = playback.seek_position();
            let track = playback.current_song().map(|t| VisualBridgeTrack {
                id: t.rri.id.clone(),
                title: t.title.clone(),
                artists: t.artists.iter().map(|a| a.name.clone()).collect(),
                album: t.album.as_ref().map(|a| a.name.clone()),
                duration_ms: t.duration_ms,
                progress_ms,
                artwork_url: t.art.best_for_width(300).map(ToString::to_string),
            });
            (playing, track)
        } else {
            (false, None)
        };

        let (rms, peak) = if active && playing {
            self.level_meter.levels()
        } else {
            (0.0, 0.0)
        };

        VisualBridgeState {
            app: "canto",
            version: 1,
            active,
            playing,
            track,
            audio: VisualBridgeAudio { rms, peak },
            theme: self.theme_engine.current_snapshot().into(),
            updated_at_ms: now_unix_ms(),
        }
    }
}

impl EventListener for VisualBridge {
    fn on_event(&mut self, event: &AppEvent) {
        if !self.inner.enabled.get() {
            return;
        }
        match event {
            AppEvent::Started => self.inner.sync_timer_and_write(),
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlaybackStopped
                | PlaybackEvent::PlayStateSynced(_)
                | PlaybackEvent::SeekSynced(_)
                | PlaybackEvent::TrackSeeked(_),
            ) => self.inner.sync_timer_and_write(),
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
    fn writes_valid_atomic_state_json_without_leftover_temp_file() {
        let dir = std::env::temp_dir().join(format!(
            "canto_bridge_test_{}_{}",
            std::process::id(),
            now_unix_ms()
        ));
        let path = dir.join("state.json");
        let state = VisualBridgeState {
            app: "canto",
            version: 1,
            active: true,
            playing: true,
            track: Some(VisualBridgeTrack {
                id: "track1".into(),
                title: "Echoes".into(),
                artists: vec!["Pink Floyd".into()],
                album: Some("Meddle".into()),
                duration_ms: 1_412_000,
                progress_ms: 42_000,
                artwork_url: None,
            }),
            audio: VisualBridgeAudio {
                rms: 0.42,
                peak: 0.88,
            },
            theme: VisualBridgeTheme {
                kind: "omarchy".into(),
                is_dark: true,
                background: "#111111".into(),
                surface: "#222222".into(),
                foreground: "#eeeeee".into(),
                accent: "#ff8800".into(),
                highlight: "#ff8800".into(),
            },
            updated_at_ms: 1_700_000_000_000,
        };

        write_atomic_state(&path, &state).expect("write should succeed");
        let text = std::fs::read_to_string(&path).expect("state.json should exist");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["app"], "canto");
        assert_eq!(parsed["playing"], true);
        assert_eq!(parsed["track"]["title"], "Echoes");
        assert!((parsed["audio"]["rms"].as_f64().unwrap() - 0.42).abs() < 1e-4);

        let entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["state.json".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
