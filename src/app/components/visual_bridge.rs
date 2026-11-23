//! Opt-in Visual & Audio Bridge for status bars, widgets, and external visualizers.
//!
//! Strictly dormant when `visual-bridge-enabled` is `false` (default):
//! zero timers, zero disk I/O, zero lyrics fetches, and zero audio-thread metering cost.

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use canto_api::Lyrics;
use canto_audio_engine::LevelMeterController;
use canto_config::settings::{LYRICS_OFFSET_MS, SETTINGS, VISUAL_BRIDGE_ENABLED};
use gio::prelude::*;

use crate::app::components::EventListener;
use crate::app::models::TrackExt;
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
    pub lyric: Option<String>,
    /// The synced lines around `lyric` and when `lyric` ends, for widgets
    /// that show more than one line or page a long one in time
    pub lyric_previous: Option<String>,
    pub lyric_next: Option<String>,
    pub lyric_ends_in_ms: Option<u64>,
    /// Whether the playing track is in Liked Songs; `None` when unknown or
    /// not a Spotify track (nothing to like)
    pub liked: Option<bool>,
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
            "lyric": self.lyric,
            "lyric_previous": self.lyric_previous,
            "lyric_next": self.lyric_next,
            "lyric_ends_in_ms": self.lyric_ends_in_ms,
            "liked": self.liked,
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

/// The synced lines around the playing position: the active one (`current`),
/// the nearest non-blank lines before and after it, and how long until the
/// next line starts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LyricContext {
    pub previous: Option<String>,
    pub current: Option<String>,
    pub next: Option<String>,
    pub current_ends_in_ms: Option<u64>,
}

/// The lyric lines around `progress_ms` (shifted by `offset_ms`). Everything
/// is `None` for missing or unsynced lyrics; `current` is `None` before the
/// first line and on a blank instrumental break, while `next` still tells
/// what comes.
pub fn synced_lyric_context(
    lyrics: Option<&Lyrics>,
    progress_ms: u64,
    offset_ms: i32,
) -> LyricContext {
    let Some(lyrics @ Lyrics::Synced(lines)) = lyrics else {
        return LyricContext::default();
    };
    let adjusted =
        (progress_ms as i128 + i128::from(offset_ms)).clamp(0, i128::from(u64::MAX)) as u64;
    let text_at = |i: usize| {
        let text = lines.get(i)?.text.trim();
        (!text.is_empty()).then(|| text.to_string())
    };
    let index = lyrics.active_line_index(adjusted);
    let after = index.map_or(0, |i| i + 1);
    LyricContext {
        previous: index.and_then(|i| (0..i).rev().find_map(text_at)),
        current: index.and_then(text_at),
        next: (after..lines.len()).find_map(text_at),
        current_ends_in_ms: index
            .and(lines.get(after))
            .map(|line| line.start_time_ms.saturating_sub(adjusted)),
    }
}

/// Returns the currently active synced lyric line at `progress_ms` (shifted by
/// `offset_ms`), or `None` when lyrics are missing, unsynced, before the first
/// line, or on a blank instrumental break line.
pub fn active_synced_lyric(
    lyrics: Option<&Lyrics>,
    progress_ms: u64,
    offset_ms: i32,
) -> Option<String> {
    synced_lyric_context(lyrics, progress_ms, offset_ms).current
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
    lyrics_track_id: RefCell<Option<String>>,
    lyrics: RefCell<Option<Lyrics>>,
    lyrics_offset_ms: Cell<i32>,
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
        Self::new_with_path(app_model, theme_engine, level_meter, runtime_state_path())
    }

    pub fn new_with_path(
        app_model: Rc<AppModel>,
        theme_engine: Rc<ThemeEngine>,
        level_meter: LevelMeterController,
        state_path: PathBuf,
    ) -> Self {
        let settings = gio::Settings::new(SETTINGS);
        let enabled = settings.boolean(VISUAL_BRIDGE_ENABLED);
        let lyrics_offset_ms = settings.int(LYRICS_OFFSET_MS);
        level_meter.set_enabled(enabled);

        let inner = Rc::new(Inner {
            app_model,
            theme_engine: Rc::clone(&theme_engine),
            level_meter,
            _settings: settings.clone(),
            enabled: Cell::new(enabled),
            timer: RefCell::new(None),
            state_path,
            lyrics_track_id: RefCell::new(None),
            lyrics: RefCell::new(None),
            lyrics_offset_ms: Cell::new(lyrics_offset_ms),
        });

        let weak = Rc::downgrade(&inner);
        settings.connect_changed(Some(VISUAL_BRIDGE_ENABLED), move |s, key| {
            if let Some(inner) = weak.upgrade() {
                inner.set_enabled(s.boolean(key));
            }
        });

        let weak_offset = Rc::downgrade(&inner);
        settings.connect_changed(Some(LYRICS_OFFSET_MS), move |s, key| {
            if let Some(inner) = weak_offset.upgrade() {
                inner.lyrics_offset_ms.set(s.int(key));
                if inner.enabled.get() {
                    inner.write_snapshot(true);
                }
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

impl Drop for VisualBridge {
    fn drop(&mut self) {
        self.inner.stop_timer();
        if self.inner.enabled.replace(false) {
            self.inner.level_meter.set_enabled(false);
            self.inner.write_snapshot(false);
        }
    }
}

impl Inner {
    fn set_enabled(self: &Rc<Self>, enabled: bool) {
        let was_enabled = self.enabled.replace(enabled);
        self.level_meter.set_enabled(enabled);
        if !enabled {
            self.stop_timer();
            *self.lyrics_track_id.borrow_mut() = None;
            *self.lyrics.borrow_mut() = None;
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

    fn ensure_lyrics_loaded(self: &Rc<Self>) {
        if !self.enabled.get() {
            return;
        }
        let Some(track) = self.app_model.get_state().playback.current_song() else {
            *self.lyrics_track_id.borrow_mut() = None;
            *self.lyrics.borrow_mut() = None;
            return;
        };

        let track_id = track.rri.id.clone();
        if self.lyrics_track_id.borrow().as_deref() == Some(track_id.as_str()) {
            return;
        }
        *self.lyrics_track_id.borrow_mut() = Some(track_id.clone());
        *self.lyrics.borrow_mut() = None;

        let key = track
            .rri
            .uri
            .clone()
            .unwrap_or_else(|| track.rri.id.clone());
        let api = self.app_model.api();

        // Check disk cache synchronously first (no Genius fallback: Genius is unsynced).
        if let Some(cached) = api.cached_lyrics(&key, false) {
            if cached.lyrics.is_synced() {
                *self.lyrics.borrow_mut() = Some(cached.lyrics);
            }
            return;
        }

        let title = track.title.clone();
        let artist = track.artists_name();
        let album = track.album.as_ref().map(|a| a.name.clone());
        let duration_secs = track.duration_ms / 1000;
        let weak = Rc::downgrade(self);

        glib::spawn_future_local(async move {
            let fetched = api
                .fetch_lyrics(
                    &key,
                    &title,
                    &artist,
                    album.as_deref(),
                    Some(duration_secs),
                    None,
                )
                .await;
            let Some(inner) = weak.upgrade() else {
                return;
            };
            if !inner.enabled.get() {
                return;
            }
            if inner.lyrics_track_id.borrow().as_deref() != Some(track_id.as_str()) {
                return;
            }
            if let Some(found) = fetched {
                if found.lyrics.is_synced() {
                    *inner.lyrics.borrow_mut() = Some(found.lyrics);
                    inner.write_snapshot(true);
                }
            }
        });
    }

    fn refresh_cached_lyrics_for(&self, updated_track_id: &str) {
        if !self.enabled.get() {
            return;
        }
        let Some(track) = self.app_model.get_state().playback.current_song() else {
            return;
        };
        if track.rri.id != updated_track_id {
            return;
        }
        let key = track
            .rri
            .uri
            .clone()
            .unwrap_or_else(|| track.rri.id.clone());
        let cached = self
            .app_model
            .api()
            .cached_lyrics(&key, false)
            .map(|found| found.lyrics)
            .filter(Lyrics::is_synced);
        *self.lyrics_track_id.borrow_mut() = Some(track.rri.id.clone());
        *self.lyrics.borrow_mut() = cached;
        self.write_snapshot(true);
    }

    fn sync_timer_and_write(self: &Rc<Self>) {
        if !self.enabled.get() {
            self.stop_timer();
            return;
        }
        self.ensure_lyrics_loaded();
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
        let liked = if active {
            let spotify = self
                .app_model
                .get_state()
                .playback
                .current_song()
                .is_some_and(|t| t.rri.provider == canto_api::models::Provider::Spotify);
            spotify
                .then(|| crate::app::components::now_panel::current_track_liked(&self.app_model))
                .flatten()
        } else {
            None
        };
        let (playing, track, lyric) = if active {
            let state = self.app_model.get_state();
            let playback = &state.playback;
            let playing = playback.is_playing();
            let progress_ms = playback.seek_position();
            let current_song = playback.current_song();
            let track = current_song.as_ref().map(|t| VisualBridgeTrack {
                id: t.rri.id.clone(),
                title: t.title.clone(),
                artists: t.artists.iter().map(|a| a.name.clone()).collect(),
                album: t.album.as_ref().map(|a| a.name.clone()),
                duration_ms: t.duration_ms,
                progress_ms,
                artwork_url: t.art.best_for_width(300).map(ToString::to_string),
            });
            let lyric = if current_song.as_ref().is_some_and(|t| {
                self.lyrics_track_id.borrow().as_deref() == Some(t.rri.id.as_str())
            }) {
                synced_lyric_context(
                    self.lyrics.borrow().as_ref(),
                    progress_ms,
                    self.lyrics_offset_ms.get(),
                )
            } else {
                LyricContext::default()
            };
            (playing, track, lyric)
        } else {
            (false, None, LyricContext::default())
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
            lyric: lyric.current,
            lyric_previous: lyric.previous,
            lyric_next: lyric.next,
            lyric_ends_in_ms: lyric.current_ends_in_ms,
            liked,
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
            AppEvent::LyricsUpdated(track_id) => self.inner.refresh_cached_lyrics_for(track_id),
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
    use canto_api::LyricLine;

    fn sample_theme() -> VisualBridgeTheme {
        VisualBridgeTheme {
            kind: "omarchy".into(),
            is_dark: true,
            background: "#111111".into(),
            surface: "#222222".into(),
            foreground: "#eeeeee".into(),
            accent: "#ff8800".into(),
            highlight: "#ff8800".into(),
        }
    }

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
            lyric: Some("Overhead the albatross".into()),
            lyric_previous: Some("Hangs motionless upon the air".into()),
            lyric_next: Some("And deep beneath the rolling waves".into()),
            lyric_ends_in_ms: Some(1_500),
            liked: Some(true),
            audio: VisualBridgeAudio {
                rms: 0.42,
                peak: 0.88,
            },
            theme: sample_theme(),
            updated_at_ms: 1_700_000_000_000,
        };

        write_atomic_state(&path, &state).expect("write should succeed");
        let text = std::fs::read_to_string(&path).expect("state.json should exist");
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid JSON");
        assert_eq!(parsed["app"], "canto");
        assert_eq!(parsed["playing"], true);
        assert_eq!(parsed["track"]["title"], "Echoes");
        assert_eq!(parsed["lyric"], "Overhead the albatross");
        assert_eq!(parsed["lyric_previous"], "Hangs motionless upon the air");
        assert_eq!(parsed["lyric_next"], "And deep beneath the rolling waves");
        assert_eq!(parsed["lyric_ends_in_ms"], 1_500);
        assert_eq!(parsed["liked"], true);
        assert!((parsed["audio"]["rms"].as_f64().unwrap() - 0.42).abs() < 1e-4);

        let entries: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(entries, vec!["state.json".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn serializes_lyric_null_when_unavailable_or_inactive() {
        let state = VisualBridgeState {
            app: "canto",
            version: 1,
            active: false,
            playing: false,
            track: None,
            lyric: None,
            lyric_previous: None,
            lyric_next: None,
            lyric_ends_in_ms: None,
            liked: None,
            audio: VisualBridgeAudio {
                rms: 0.0,
                peak: 0.0,
            },
            theme: sample_theme(),
            updated_at_ms: 1_700_000_000_001,
        };

        let json = state.to_json_value();
        assert!(json["lyric"].is_null());
        assert!(json["lyric_next"].is_null());
        assert!(json["lyric_ends_in_ms"].is_null());
        assert!(json["liked"].is_null());
        assert!(json["track"].is_null());
        assert_eq!(json["active"], false);
    }

    #[test]
    fn active_synced_lyric_handles_synced_plain_offset_and_empty_lines() {
        let synced = Lyrics::Synced(vec![
            LyricLine::new(10_000, "First line"),
            LyricLine::new(20_000, "   "),
            LyricLine::new(25_000, "  Second line  "),
        ]);

        // Before the first line -> None.
        assert_eq!(active_synced_lyric(Some(&synced), 5_000, 0), None);
        // Active line matches and trims surrounding whitespace.
        assert_eq!(
            active_synced_lyric(Some(&synced), 12_000, 0).as_deref(),
            Some("First line")
        );
        // Blank instrumental break line -> None (not empty string).
        assert_eq!(active_synced_lyric(Some(&synced), 22_000, 0), None);
        assert_eq!(
            active_synced_lyric(Some(&synced), 25_000, 0).as_deref(),
            Some("Second line")
        );
        // Positive offset advances lyrics earlier; negative delays and clamps at 0.
        assert_eq!(
            active_synced_lyric(Some(&synced), 9_500, 600).as_deref(),
            Some("First line")
        );
        assert_eq!(active_synced_lyric(Some(&synced), 10_500, -1_000), None);
        assert_eq!(active_synced_lyric(Some(&synced), 100, -5_000), None);

        // Plain (unsynced) lyrics and None -> always None.
        let plain = Lyrics::Plain(vec!["Unsynced verse".into()]);
        assert_eq!(active_synced_lyric(Some(&plain), 15_000, 0), None);
        assert_eq!(active_synced_lyric(None, 15_000, 0), None);
    }

    #[test]
    fn lyric_context_gives_the_lines_around_and_when_the_current_ends() {
        let synced = Lyrics::Synced(vec![
            LyricLine::new(10_000, "First line"),
            LyricLine::new(20_000, "   "),
            LyricLine::new(25_000, "Second line"),
            LyricLine::new(30_000, "Third line"),
        ]);
        // Before the first line: nothing current, the first line is next
        let before = synced_lyric_context(Some(&synced), 5_000, 0);
        assert_eq!(before.current, None);
        assert_eq!(before.next.as_deref(), Some("First line"));
        assert_eq!(before.current_ends_in_ms, None);
        // Inside a line: its neighbours skip blank lines; it ends when the
        // next line (blank or not) starts
        let second = synced_lyric_context(Some(&synced), 26_000, 0);
        assert_eq!(second.previous.as_deref(), Some("First line"));
        assert_eq!(second.current.as_deref(), Some("Second line"));
        assert_eq!(second.next.as_deref(), Some("Third line"));
        assert_eq!(second.current_ends_in_ms, Some(4_000));
        let first = synced_lyric_context(Some(&synced), 12_000, 0);
        assert_eq!(first.next.as_deref(), Some("Second line"));
        assert_eq!(first.current_ends_in_ms, Some(8_000));
        // The last line has no next and no known end
        let last = synced_lyric_context(Some(&synced), 31_000, 0);
        assert_eq!(last.next, None);
        assert_eq!(last.current_ends_in_ms, None);
        // The offset shifts the position, as for `lyric`
        let shifted = synced_lyric_context(Some(&synced), 24_500, 600);
        assert_eq!(shifted.current.as_deref(), Some("Second line"));
        // Unsynced or missing lyrics: all empty
        let plain = Lyrics::Plain(vec!["Unsynced verse".into()]);
        assert_eq!(
            synced_lyric_context(Some(&plain), 15_000, 0),
            LyricContext::default()
        );
        assert_eq!(
            synced_lyric_context(None, 15_000, 0),
            LyricContext::default()
        );
    }
}
