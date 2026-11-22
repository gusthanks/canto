//! Audio mixing / equal-power crossfade processor.
//!
//! Records a bounded circular tail buffer while crossfade is enabled
//! (`duration_ms > 0`) and blends the previous track's tail out while fading
//! the incoming track in using an equal-power (`cos` / `sin`) curve whenever a
//! transition is triggered.

use super::{AudioBuffer, Processor};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::Arc;

/// Maximum user-configurable crossfade duration in seconds.
pub const MAX_CROSSFADE_SECONDS: u32 = 12;

/// Default duration used by the legacy boolean constructor [`MixController::new(true)`].
const LEGACY_ENABLED_MS: u32 = 3_000;

/// A thread-safe handle for controlling crossfades while audio is playing.
///
/// Uses lock-free atomic counters so UI updates and track transitions never block
/// the audio callback thread.
#[derive(Clone)]
pub struct MixController {
    generation: Arc<AtomicU64>,
    duration_ms: Arc<AtomicU32>,
    transition_gen: Arc<AtomicU64>,
    reset_gen: Arc<AtomicU64>,
}

impl MixController {
    /// Legacy boolean constructor (`true` selects 3 seconds, `false` disables).
    pub fn new(enabled: bool) -> Self {
        Self::with_duration_ms(if enabled { LEGACY_ENABLED_MS } else { 0 })
    }

    /// Create a controller with a crossfade duration in seconds (clamped to `0..=MAX_CROSSFADE_SECONDS`).
    pub fn with_duration_secs(seconds: u32) -> Self {
        let clamped_ms = seconds.min(MAX_CROSSFADE_SECONDS).saturating_mul(1000);
        Self::with_duration_ms(clamped_ms)
    }

    fn with_duration_ms(ms: u32) -> Self {
        Self {
            generation: Arc::new(AtomicU64::new(0)),
            duration_ms: Arc::new(AtomicU32::new(ms)),
            transition_gen: Arc::new(AtomicU64::new(0)),
            reset_gen: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Legacy boolean update helper (`true` -> 3 seconds, `false` -> 0).
    pub fn update(&self, enabled: bool) {
        self.set_duration_secs(if enabled { 3 } else { 0 });
    }

    /// Set the crossfade duration in seconds (`0` disables crossfading).
    pub fn set_duration_secs(&self, seconds: u32) {
        let ms = seconds.min(MAX_CROSSFADE_SECONDS).saturating_mul(1000);
        self.duration_ms.store(ms, Ordering::Release);
        self.generation.fetch_add(1, Ordering::Release);
    }

    /// Currently configured crossfade duration in milliseconds.
    pub fn duration_ms(&self) -> u32 {
        self.duration_ms.load(Ordering::Acquire)
    }

    /// Snapshot the recorded tail into an active equal-power crossfade.
    pub fn trigger_transition(&self) {
        if self.duration_ms.load(Ordering::Acquire) > 0 {
            self.transition_gen.fetch_add(1, Ordering::Release);
        }
    }

    /// Discard any recorded tail buffer and cancel any in-flight crossfade.
    pub fn reset(&self) {
        self.reset_gen.fetch_add(1, Ordering::Release);
    }

    fn generation(&self) -> u64 {
        self.generation.load(Ordering::Acquire)
    }

    fn transition_generation(&self) -> u64 {
        self.transition_gen.load(Ordering::Acquire)
    }

    fn reset_generation(&self) -> u64 {
        self.reset_gen.load(Ordering::Acquire)
    }
}

/// Real-time equal-power crossfade [`Processor`].
///
/// While no transition has been triggered, audio passes through bit-for-bit
/// untouched (only being mirrored into the ring buffer when `duration_ms > 0`).
pub struct MixProcessor {
    controller: MixController,
    duration_ms: u32,
    generation: u64,
    transition_gen: u64,
    reset_gen: u64,
    tail_ring: Vec<f64>,
    tail_head: usize,
    tail_len: usize,
    fade_out_buf: Vec<f64>,
    fade_pos: usize,
}

impl MixProcessor {
    /// Create a processor bound to the given controller.
    pub fn new(controller: MixController) -> Self {
        let duration_ms = controller.duration_ms();
        let generation = controller.generation();
        let transition_gen = controller.transition_generation();
        let reset_gen = controller.reset_generation();
        Self {
            controller,
            duration_ms,
            generation,
            transition_gen,
            reset_gen,
            tail_ring: Vec::new(),
            tail_head: 0,
            tail_len: 0,
            fade_out_buf: Vec::new(),
            fade_pos: 0,
        }
    }

    fn clear_tail(&mut self, deallocate: bool) {
        self.tail_head = 0;
        self.tail_len = 0;
        if deallocate {
            self.tail_ring = Vec::new();
        }
    }

    fn clear_fade(&mut self) {
        self.fade_out_buf.clear();
        self.fade_pos = 0;
    }

    fn ensure_ring_capacity(&mut self, channels: usize, sample_rate: u32) {
        let target_cap = target_ring_samples(self.duration_ms, channels, sample_rate);
        if target_cap == 0 {
            self.clear_tail(true);
            return;
        }
        if self.tail_ring.len() != target_cap {
            self.tail_ring.resize(target_cap, 0.0);
            self.tail_head = 0;
            self.tail_len = 0;
        }
    }

    fn snapshot_tail_for_fade(&mut self, channels: usize, sample_rate: u32) {
        if self.duration_ms == 0 || self.tail_len == 0 || self.tail_ring.is_empty() {
            return;
        }
        let target_samples = target_ring_samples(self.duration_ms, channels, sample_rate);
        let available = self.tail_len.min(target_samples);
        let take = (available / channels) * channels;
        if take == 0 {
            return;
        }

        let cap = self.tail_ring.len();
        let start = (self.tail_head + cap - take) % cap;
        self.fade_out_buf.clear();
        self.fade_out_buf.reserve(take);
        if start + take <= cap {
            self.fade_out_buf
                .extend_from_slice(&self.tail_ring[start..start + take]);
        } else {
            let first = cap - start;
            self.fade_out_buf
                .extend_from_slice(&self.tail_ring[start..]);
            self.fade_out_buf
                .extend_from_slice(&self.tail_ring[..take - first]);
        }
        self.fade_pos = 0;
        self.clear_tail(false);
    }

    #[inline]
    fn push_tail_sample(&mut self, sample: f64) {
        let cap = self.tail_ring.len();
        if cap == 0 {
            return;
        }
        self.tail_ring[self.tail_head] = sample;
        self.tail_head += 1;
        if self.tail_head == cap {
            self.tail_head = 0;
        }
        if self.tail_len < cap {
            self.tail_len += 1;
        }
    }

    #[inline]
    fn refresh(&mut self, channels: usize, sample_rate: u32) {
        let reset_gen = self.controller.reset_generation();
        if reset_gen != self.reset_gen {
            self.reset_gen = reset_gen;
            self.clear_tail(false);
            self.clear_fade();
        }

        let generation = self.controller.generation();
        if generation != self.generation {
            self.generation = generation;
            self.duration_ms = self.controller.duration_ms();
            if self.duration_ms == 0 {
                self.clear_tail(true);
                self.clear_fade();
            }
        }

        let transition_gen = self.controller.transition_generation();
        if transition_gen != self.transition_gen {
            self.transition_gen = transition_gen;
            self.snapshot_tail_for_fade(channels, sample_rate);
        }
    }
}

fn target_ring_samples(duration_ms: u32, channels: usize, sample_rate: u32) -> usize {
    if duration_ms == 0 || channels == 0 || sample_rate == 0 {
        return 0;
    }
    let frames = ((duration_ms as u64) * (sample_rate as u64)) / 1000;
    (frames as usize).saturating_mul(channels)
}

impl Processor for MixProcessor {
    fn name(&self) -> &str {
        "mixer"
    }

    fn is_active(&self) -> bool {
        self.duration_ms > 0 || !self.fade_out_buf.is_empty()
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        let channels = (buffer.channels as usize).max(1);
        self.refresh(channels, buffer.sample_rate);

        if self.duration_ms == 0 && self.fade_out_buf.is_empty() {
            return;
        }

        self.ensure_ring_capacity(channels, buffer.sample_rate);

        let fade_frames = (self.fade_out_buf.len() / channels).max(1);
        for frame in buffer.samples.chunks_exact_mut(channels) {
            // Capture the incoming dry signal before mixing so chained skips
            // always crossfade cleanly from the latest track.
            for &sample in frame.iter() {
                self.push_tail_sample(sample);
            }

            if self.fade_pos + channels <= self.fade_out_buf.len() {
                let frame_idx = self.fade_pos / channels;
                let t = if fade_frames > 1 {
                    (frame_idx as f64 / (fade_frames - 1) as f64).clamp(0.0, 1.0)
                } else {
                    0.5
                };
                let angle = t * std::f64::consts::FRAC_PI_2;
                let gain_out = angle.cos();
                let gain_in = angle.sin();

                for (ch, sample) in frame.iter_mut().enumerate() {
                    *sample = self.fade_out_buf[self.fade_pos + ch] * gain_out + *sample * gain_in;
                }
                self.fade_pos += channels;
                if self.fade_pos + channels > self.fade_out_buf.len() {
                    self.clear_fade();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stub_does_not_modify_audio_when_disabled() {
        let mut processor = MixProcessor::new(MixController::new(false));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.1, 0.2, 0.3, 0.4]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn stub_does_not_modify_audio_when_enabled() {
        let mut processor = MixProcessor::new(MixController::new(true));
        let mut buffer = AudioBuffer::stereo_44100(vec![0.1, 0.2, 0.3, 0.4]);
        let original = buffer.clone();
        processor.process(&mut buffer);
        assert_eq!(buffer, original);
    }

    #[test]
    fn controller_updates_are_observed() {
        let controller = MixController::new(false);
        let mut processor = MixProcessor::new(controller.clone());
        controller.update(true);
        let mut buffer = AudioBuffer::stereo_44100(vec![0.5, 0.5]);
        processor.process(&mut buffer);
        assert_eq!(buffer.samples, vec![0.5, 0.5]);
        assert!(processor.is_active());
    }

    #[test]
    fn equal_power_crossfade_blends_outgoing_tail_into_incoming_track() {
        let controller = MixController::with_duration_secs(1);
        let mut processor = MixProcessor::new(controller.clone());

        // Use a tiny sample rate so 1 second = 5 stereo frames (10 samples) for crisp assertions.
        let mut track_a = AudioBuffer::new(vec![1.0; 10], 2, 5);
        processor.process(&mut track_a);
        assert_eq!(track_a.samples, vec![1.0; 10]);

        controller.trigger_transition();

        // Track B has constant amplitude 1.0 on right, -1.0 on left across 5 fade frames + 2 post-fade frames.
        let mut track_b_samples = Vec::with_capacity(14);
        for _ in 0..7 {
            track_b_samples.push(-1.0);
            track_b_samples.push(1.0);
        }
        let mut track_b = AudioBuffer::new(track_b_samples, 2, 5);
        processor.process(&mut track_b);

        // Frame 0: cos(0) = 1.0, sin(0) = 0.0 -> pure outgoing Track A (1.0, 1.0).
        assert!((track_b.samples[0] - 1.0).abs() < 1e-6);
        assert!((track_b.samples[1] - 1.0).abs() < 1e-6);

        // Midpoint frame 2 (t = 0.5): cos(pi/4) = sin(pi/4) = sqrt(0.5).
        // L: 1.0 * sqrt(0.5) + (-1.0) * sqrt(0.5) = 0.0
        // R: 1.0 * sqrt(0.5) + (1.0) * sqrt(0.5) = sqrt(2)
        assert!(track_b.samples[4].abs() < 1e-6);
        assert!((track_b.samples[5] - std::f64::consts::SQRT_2).abs() < 1e-6);

        // Frame 4 (t = 1.0): cos(pi/2) = 0.0, sin(pi/2) = 1.0 -> pure incoming Track B (-1.0, 1.0).
        assert!((track_b.samples[8] - (-1.0)).abs() < 1e-6);
        assert!((track_b.samples[9] - 1.0).abs() < 1e-6);

        // Post-fade frames pass Track B through untouched.
        assert_eq!(&track_b.samples[10..14], &[-1.0, 1.0, -1.0, 1.0]);
    }

    #[test]
    fn reset_discards_tail_and_stops_active_fade() {
        let controller = MixController::with_duration_secs(1);
        let mut processor = MixProcessor::new(controller.clone());

        let mut track_a = AudioBuffer::new(vec![0.8; 8], 2, 4);
        processor.process(&mut track_a);
        controller.reset();

        // Triggering after reset must not resurrect the cleared tail.
        controller.trigger_transition();
        let mut track_b = AudioBuffer::new(vec![0.25; 4], 2, 4);
        processor.process(&mut track_b);
        assert_eq!(track_b.samples, vec![0.25; 4]);
    }

    #[test]
    fn setting_duration_to_zero_restores_passthrough() {
        let controller = MixController::with_duration_secs(2);
        let mut processor = MixProcessor::new(controller.clone());

        let mut track_a = AudioBuffer::new(vec![0.9; 8], 2, 4);
        processor.process(&mut track_a);

        controller.set_duration_secs(0);
        let mut next = AudioBuffer::new(vec![0.1, -0.2, 0.3, -0.4], 2, 4);
        let expected = next.samples.clone();
        processor.process(&mut next);
        assert_eq!(next.samples, expected);
        assert!(!processor.is_active());
    }
}
