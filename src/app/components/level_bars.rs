//! Bars that move with the music: the "playing" mark of track rows, cards and
//! the side panel's Now Playing tab.
//!
//! While the song plays on this computer (Canto's own player, Soloist or the
//! hidden Spotify), `parec` records only that app's output stream (never the
//! rest of the system) and four band filters turn it into bar heights, from
//! the bass (left) to the treble (right). Playing on another device (Connect,
//! Alexa) there's no sound here to listen to: the bars move on their own, like
//! the old animation. Paused, they rest.
//!
//! Cost: one `parec` process and a few multiplications per sample while
//! playing here; redraws only for the bars actually on screen.

use std::cell::{Cell, RefCell};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{glib, graphene};

use crate::app::components::EventListener;
use crate::app::state::PlaybackEvent;
use crate::app::{AppEvent, AppModel};

const BARS: usize = 4;
const FRAME_MS: u64 = 33;
const RATE: u32 = 24_000;
const FRAME_SAMPLES: usize = (RATE as usize * FRAME_MS as usize) / 1000;
/// Band centers in Hz, bass to treble.
const BAND_HZ: [f32; BARS] = [90.0, 400.0, 1600.0, 6000.0];
/// Still shape of the Now Playing tab while nothing plays (in 16px units).
const REST_SHAPE: [f32; BARS] = [10.0, 5.0, 8.0, 6.0];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Mode {
    Paused,
    /// Playing, but not heard here (another device, or no stream found yet).
    Free,
    /// Playing here: heights come from the sound.
    Live,
}

/// Written by the reader thread, read by the UI.
#[derive(Default)]
struct Shared {
    levels: [AtomicU32; BARS],
    alive: AtomicBool,
}

impl Shared {
    fn level(&self, i: usize) -> f32 {
        f32::from_bits(self.levels[i].load(Ordering::Relaxed))
    }
}

thread_local! {
    static MODE: Cell<Mode> = const { Cell::new(Mode::Paused) };
    static SHARED: RefCell<Option<Arc<Shared>>> = const { RefCell::new(None) };
    static SHOWN: RefCell<Vec<glib::WeakRef<LevelBars>>> = const { RefCell::new(Vec::new()) };
    static TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    static HEIGHTS: Cell<[f32; BARS]> = const { Cell::new([0.0; BARS]) };
    static EPOCH: Instant = Instant::now();
}

fn mode() -> Mode {
    MODE.with(Cell::get)
}

fn set_mode(mode: Mode) {
    if MODE.with(|m| m.replace(mode)) == mode {
        return;
    }
    redraw_all();
    sync_timer();
}

fn redraw_all() {
    SHOWN.with(|shown| {
        for bars in shown.borrow().iter().filter_map(|w| w.upgrade()) {
            bars.queue_draw();
        }
    });
}

/// The timer runs only while something plays and a bar is on screen.
fn sync_timer() {
    let needed = mode() != Mode::Paused && SHOWN.with(|s| !s.borrow().is_empty());
    TIMER.with(|timer| {
        let mut timer = timer.borrow_mut();
        if needed && timer.is_none() {
            *timer = Some(glib::timeout_add_local(
                Duration::from_millis(FRAME_MS),
                || {
                    tick();
                    glib::ControlFlow::Continue
                },
            ));
        } else if !needed {
            if let Some(source) = timer.take() {
                source.remove();
            }
        }
    });
}

fn tick() {
    let heights = match mode() {
        Mode::Paused => return,
        Mode::Live => {
            let shared = SHARED.with(|s| s.borrow().clone());
            match shared {
                Some(shared) if shared.alive.load(Ordering::Relaxed) => {
                    std::array::from_fn(|i| shared.level(i))
                }
                _ => {
                    // The stream ended (next song on a new stream, device change)
                    set_mode(Mode::Free);
                    free_heights()
                }
            }
        }
        Mode::Free => free_heights(),
    };
    HEIGHTS.with(|h| h.set(heights));
    redraw_all();
}

/// Movement without sound: each bar on its own slow wave, like the old
/// animation.
fn free_heights() -> [f32; BARS] {
    let t = EPOCH.with(|e| e.elapsed().as_secs_f32());
    const SPEED: [f32; BARS] = [5.1, 7.3, 6.2, 8.9];
    const PHASE: [f32; BARS] = [0.0, 1.7, 3.1, 4.4];
    std::array::from_fn(|i| {
        let a = (t * SPEED[i] + PHASE[i]).sin();
        let b = (t * SPEED[i] * 0.43 + PHASE[i] * 2.0).sin();
        (0.55 + 0.3 * a + 0.15 * b).clamp(0.08, 1.0)
    })
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct LevelBars {
        pub active: Cell<bool>,
        pub rest_shape: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for LevelBars {
        const NAME: &'static str = "CantoLevelBars";
        type Type = super::LevelBars;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for LevelBars {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.set_hexpand(true);
            obj.set_vexpand(true);
            obj.set_can_target(false);
        }

        fn dispose(&self) {
            self.obj().unregister();
        }
    }

    impl WidgetImpl for LevelBars {
        fn measure(&self, _orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {
            (16, 16, -1, -1)
        }

        fn map(&self) {
            self.parent_map();
            self.obj().sync_registration();
        }

        fn unmap(&self) {
            self.obj().unregister();
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let obj = self.obj();
            let (w, h) = (obj.width() as f32, obj.height() as f32);
            let side = w.min(h);
            if side <= 0.0 {
                return;
            }
            let unit = side / 16.0;
            let (ox, oy) = ((w - side) / 2.0, (h - side) / 2.0);
            let color = obj.color();
            let paused = !self.active.get() || mode() == Mode::Paused;
            let levels = HEIGHTS.with(Cell::get);
            for i in 0..BARS {
                // Same grid as the old icons: bars sit on y = 14, 2 to 13 tall
                let tall = if paused {
                    if self.rest_shape.get() { REST_SHAPE[i] } else { 2.0 }
                } else {
                    2.0 + levels[i].clamp(0.0, 1.0) * 11.0
                };
                let rect = graphene::Rect::new(
                    ox + (0.5 + i as f32 * 4.0) * unit,
                    oy + (14.0 - tall) * unit,
                    3.0 * unit,
                    tall * unit,
                );
                snapshot.append_color(&color, &rect);
            }
        }
    }
}

glib::wrapper! {
    pub struct LevelBars(ObjectSubclass<imp::LevelBars>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for LevelBars {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl LevelBars {
    /// Bars that show the still Now Playing shape (instead of flat bars)
    /// while nothing plays.
    pub fn with_rest_shape() -> Self {
        let bars = Self::default();
        bars.imp().rest_shape.set(true);
        bars
    }

    /// Only active bars move (the playing row, the playing card): the rest of
    /// a list never redraws.
    pub fn set_active(&self, active: bool) {
        if self.imp().active.replace(active) != active {
            self.sync_registration();
            self.queue_draw();
        }
    }

    fn sync_registration(&self) {
        if self.imp().active.get() && self.is_mapped() {
            SHOWN.with(|shown| {
                let mut shown = shown.borrow_mut();
                shown.retain(|w| w.upgrade().is_some());
                if !shown.iter().any(|w| w.upgrade().as_ref() == Some(self)) {
                    shown.push(self.downgrade());
                }
            });
            sync_timer();
        } else {
            self.unregister();
        }
    }

    fn unregister(&self) {
        // try_with: widgets may be disposed while the app shuts down
        let removed = SHOWN.try_with(|shown| {
            shown
                .borrow_mut()
                .retain(|w| w.upgrade().is_some_and(|b| &b != self))
        });
        if removed.is_ok() {
            sync_timer();
        }
    }
}

/// Fills `container` (an empty indicator box) with bars and returns them.
pub fn fill(container: &gtk::Box, rest_shape: bool) -> LevelBars {
    let bars = if rest_shape {
        LevelBars::with_rest_shape()
    } else {
        LevelBars::default()
    };
    container.append(&bars);
    bars
}

// ---------------------------------------------------------------------------
// Listening: find this app's output stream and read it.

/// Reads the playback state and starts/stops listening.
pub struct LevelBarsListener {
    app_model: Rc<AppModel>,
    capture: Option<Capture>,
}

struct Capture {
    stop: Arc<AtomicBool>,
    child: Arc<Mutex<Option<Child>>>,
}

impl Capture {
    fn end(self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(mut child) = self.child.lock().ok().and_then(|mut c| c.take()) {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl LevelBarsListener {
    pub fn new(app_model: Rc<AppModel>) -> Self {
        Self {
            app_model,
            capture: None,
        }
    }

    fn sync(&mut self, restart: bool) {
        let playing = self.app_model.get_state().playback.is_playing();
        if !playing {
            if let Some(capture) = self.capture.take() {
                capture.end();
            }
            set_mode(Mode::Paused);
            return;
        }
        if mode() == Mode::Paused {
            set_mode(Mode::Free);
        }
        let alive = SHARED.with(|s| {
            s.borrow()
                .as_ref()
                .is_some_and(|s| s.alive.load(Ordering::Relaxed))
        });
        if restart || !alive {
            if let Some(capture) = self.capture.take() {
                capture.end();
            }
            self.capture = Some(start_capture());
        }
    }
}

impl EventListener for LevelBarsListener {
    fn on_event(&mut self, event: &AppEvent) {
        match event {
            // New song or new device: the stream may be a new one
            AppEvent::PlaybackEvent(
                PlaybackEvent::TrackChanged(_)
                | PlaybackEvent::TrackSynced
                | PlaybackEvent::SwitchedDevice(_),
            ) => self.sync(true),
            // "Synced" = the state came from the player (another device,
            // a device switch, Canto opening on a song already playing)
            AppEvent::PlaybackEvent(
                PlaybackEvent::PlaybackPaused
                | PlaybackEvent::PlaybackResumed
                | PlaybackEvent::PlaybackStopped
                | PlaybackEvent::PlayStateSynced(_),
            ) => self.sync(false),
            _ => {}
        }
    }
}

impl Drop for LevelBarsListener {
    fn drop(&mut self) {
        if let Some(capture) = self.capture.take() {
            capture.end();
        }
    }
}

/// Looks for the stream for a few seconds (it shows up a moment after play),
/// then reads it on a thread. Nothing found: the bars keep moving on their own.
fn start_capture() -> Capture {
    let stop = Arc::new(AtomicBool::new(false));
    let child: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
    let shared = Arc::new(Shared::default());
    SHARED.with(|s| s.replace(Some(Arc::clone(&shared))));

    let (stop2, child2) = (Arc::clone(&stop), Arc::clone(&child));
    let spawned = std::thread::Builder::new()
        .name("canto-level-bars".into())
        .spawn(move || {
            for _ in 0..8 {
                if stop2.load(Ordering::Relaxed) {
                    return;
                }
                if let Some(index) = find_stream() {
                    read_stream(index, &stop2, &child2, &shared);
                    return;
                }
                std::thread::sleep(Duration::from_millis(1000));
            }
            debug!("level bars: no stream of ours playing here (another device?)");
        });
    if let Err(e) = spawned {
        warn!("level bars: no thread: {e}");
    }
    // The UI switches to the sound as soon as the reader says it's alive
    glib::timeout_add_local(Duration::from_millis(250), {
        let stop = Arc::clone(&stop);
        let mut tries = 0;
        move || {
            tries += 1;
            if stop.load(Ordering::Relaxed) || tries > 40 {
                return glib::ControlFlow::Break;
            }
            let alive = SHARED.with(|s| {
                s.borrow()
                    .as_ref()
                    .is_some_and(|s| s.alive.load(Ordering::Relaxed))
            });
            if alive {
                if mode() == Mode::Free {
                    set_mode(Mode::Live);
                }
                return glib::ControlFlow::Break;
            }
            glib::ControlFlow::Continue
        }
    });
    Capture { stop, child }
}

/// The output stream (sink input) of the player we drive: Canto itself,
/// Soloist, or the hidden Spotify.
fn find_stream() -> Option<u32> {
    let output = Command::new("pactl")
        .args(["-f", "json", "list", "sink-inputs"])
        .stderr(Stdio::null())
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let inputs: serde_json::Value = serde_json::from_slice(&output.stdout).ok()?;
    pick_stream(&inputs, std::process::id())
}

fn pick_stream(inputs: &serde_json::Value, own_pid: u32) -> Option<u32> {
    let ours = |input: &serde_json::Value| {
        let props = &input["properties"];
        let text = |key: &str| props[key].as_str().unwrap_or("").to_lowercase();
        let pid = text("application.process.id");
        let binary = text("application.process.binary");
        let name = text("application.name");
        pid == own_pid.to_string()
            || ["soloist", "spotify"]
                .iter()
                .any(|p| binary.contains(p) || name.contains(p))
    };
    // Only a stream that is playing: a corked (paused) one is silence, and
    // while the music is on another device the bars should keep moving
    inputs
        .as_array()?
        .iter()
        .filter(|input| ours(input) && !input["corked"].as_bool().unwrap_or(false))
        .next()
        .and_then(|input| input["index"].as_u64())
        .map(|i| i as u32)
}

fn read_stream(
    index: u32,
    stop: &AtomicBool,
    child_slot: &Mutex<Option<Child>>,
    shared: &Shared,
) {
    let spawned = Command::new("parec")
        .arg(format!("--monitor-stream={index}"))
        .args([
            "--format=s16le",
            "--channels=1",
            &format!("--rate={RATE}"),
            "--latency-msec=40",
            "--client-name=Canto level bars",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawned {
        Ok(child) => child,
        Err(e) => {
            debug!("level bars: parec unavailable: {e}");
            return;
        }
    };
    let Some(mut out) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return;
    };
    if let Ok(mut slot) = child_slot.lock() {
        *slot = Some(child);
    }
    if stop.load(Ordering::Relaxed) {
        if let Some(mut child) = child_slot.lock().ok().and_then(|mut c| c.take()) {
            let _ = child.kill();
            let _ = child.wait();
        }
        return;
    }
    debug!("level bars: listening to stream {index}");
    let mut meter = BandMeter::new(RATE as f32);
    let mut bytes = vec![0u8; FRAME_SAMPLES * 2];
    while !stop.load(Ordering::Relaxed) {
        if out.read_exact(&mut bytes).is_err() {
            break; // stream gone (parec ends with it) or we were stopped
        }
        let samples = bytes
            .chunks_exact(2)
            .map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32768.0);
        let levels = meter.frame(samples);
        for (slot, level) in shared.levels.iter().zip(levels) {
            slot.store(level.to_bits(), Ordering::Relaxed);
        }
        shared.alive.store(true, Ordering::Relaxed);
    }
    shared.alive.store(false, Ordering::Relaxed);
    if let Some(mut child) = child_slot.lock().ok().and_then(|mut c| c.take()) {
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Four band-pass filters with their own slow auto-gain, so quiet songs and
/// low volume still fill the bars.
struct BandMeter {
    filters: [Biquad; BARS],
    peaks: [f32; BARS],
    shown: [f32; BARS],
}

impl BandMeter {
    fn new(rate: f32) -> Self {
        Self {
            filters: BAND_HZ.map(|hz| Biquad::band_pass(hz, 0.9, rate)),
            peaks: [1e-3; BARS],
            shown: [0.0; BARS],
        }
    }

    fn frame(&mut self, samples: impl Iterator<Item = f32>) -> [f32; BARS] {
        let mut sums = [0.0f32; BARS];
        let mut n = 0usize;
        for x in samples {
            for (filter, sum) in self.filters.iter_mut().zip(sums.iter_mut()) {
                let y = filter.run(x);
                *sum += y * y;
            }
            n += 1;
        }
        let n = n.max(1) as f32;
        for i in 0..BARS {
            let energy = (sums[i] / n).sqrt();
            self.peaks[i] = energy.max(self.peaks[i] * 0.995);
            let level = if self.peaks[i] > 1e-4 {
                (energy / self.peaks[i]).min(1.0)
            } else {
                0.0 // silence (or volume at zero): flat bars
            };
            // Up at once, down slowly: easier on the eye
            self.shown[i] = if level > self.shown[i] {
                level
            } else {
                self.shown[i] * 0.75 + level * 0.25
            };
        }
        self.shown
    }
}

/// RBJ band-pass (constant 0 dB peak gain).
struct Biquad {
    b0: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl Biquad {
    fn band_pass(hz: f32, q: f32, rate: f32) -> Self {
        let w0 = 2.0 * std::f32::consts::PI * hz / rate;
        let alpha = w0.sin() / (2.0 * q);
        let a0 = 1.0 + alpha;
        Self {
            b0: alpha / a0,
            b2: -alpha / a0,
            a1: -2.0 * w0.cos() / a0,
            a2: (1.0 - alpha) / a0,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.b2 * self.x2 - self.a1 * self.y1 - self.a2 * self.y2;
        self.x2 = self.x1;
        self.x1 = x;
        self.y2 = self.y1;
        self.y1 = y;
        y
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(hz: f32, frames: usize) -> Vec<f32> {
        (0..FRAME_SAMPLES * frames)
            .map(|n| (2.0 * std::f32::consts::PI * hz * n as f32 / RATE as f32).sin() * 0.5)
            .collect()
    }

    /// Mean band energy of a tone, relative across bands (no auto-gain).
    fn band_energy(hz: f32) -> [f32; BARS] {
        let mut filters = BAND_HZ.map(|c| Biquad::band_pass(c, 0.9, RATE as f32));
        let mut sums = [0.0f32; BARS];
        for x in tone(hz, 10) {
            for (f, s) in filters.iter_mut().zip(sums.iter_mut()) {
                let y = f.run(x);
                *s += y * y;
            }
        }
        sums
    }

    fn loudest(e: [f32; BARS]) -> usize {
        (0..BARS)
            .max_by(|&a, &b| e[a].partial_cmp(&e[b]).unwrap())
            .unwrap()
    }

    #[test]
    fn each_band_hears_its_own_range() {
        assert_eq!(loudest(band_energy(70.0)), 0, "bass");
        assert_eq!(loudest(band_energy(420.0)), 1, "low mids");
        assert_eq!(loudest(band_energy(1700.0)), 2, "high mids");
        assert_eq!(loudest(band_energy(7000.0)), 3, "treble");
    }

    #[test]
    fn silence_keeps_the_bars_flat() {
        let mut meter = BandMeter::new(RATE as f32);
        for _ in 0..20 {
            let levels = meter.frame(std::iter::repeat_n(0.0, FRAME_SAMPLES));
            assert!(levels.iter().all(|&l| l == 0.0));
        }
    }

    #[test]
    fn a_steady_tone_fills_its_bar() {
        let mut meter = BandMeter::new(RATE as f32);
        let samples = tone(90.0, 30);
        let mut levels = [0.0; BARS];
        for frame in samples.chunks(FRAME_SAMPLES) {
            levels = meter.frame(frame.iter().copied());
        }
        assert!(levels[0] > 0.9, "bass bar at {}", levels[0]);
    }

    #[test]
    fn picks_our_playing_stream_over_others() {
        let inputs = serde_json::json!([
            {"index": 3, "corked": false,
             "properties": {"application.name": "Firefox", "application.process.binary": "firefox",
                            "application.process.id": "10"}},
            {"index": 7, "corked": true,
             "properties": {"application.name": "Canto", "application.process.binary": "canto",
                            "application.process.id": "42"}},
            {"index": 9, "corked": false,
             "properties": {"application.name": "Canto", "application.process.binary": "canto",
                            "application.process.id": "42"}},
        ]);
        assert_eq!(pick_stream(&inputs, 42), Some(9));
        let only_paused = serde_json::json!([
            {"index": 7, "corked": true,
             "properties": {"application.name": "Spotify", "application.process.binary": "soloist",
                            "application.process.id": "77"}},
        ]);
        assert_eq!(pick_stream(&only_paused, 42), None);
        let soloist = serde_json::json!([
            {"index": 5, "corked": false,
             "properties": {"application.name": "spotify-soloist", "application.process.binary": "soloist",
                            "application.process.id": "77"}},
        ]);
        assert_eq!(pick_stream(&soloist, 42), Some(5));
        let others = serde_json::json!([
            {"index": 3, "corked": false,
             "properties": {"application.name": "Firefox", "application.process.binary": "firefox",
                            "application.process.id": "10"}},
        ]);
        assert_eq!(pick_stream(&others, 42), None);
    }
}
