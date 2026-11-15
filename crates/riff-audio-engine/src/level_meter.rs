//! Lock-free audio RMS and peak meter for external visualizers.
//!
//! Disabled by default. When inactive, [`LevelMeterProcessor::process`] checks
//! a single atomic flag and returns immediately with zero arithmetic, zero
//! allocations, and zero locks.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

use super::{AudioBuffer, Processor};

/// Lock-free handle shared between the audio thread and UI/runtime bridge.
#[derive(Clone)]
pub struct LevelMeterController {
    enabled: Arc<AtomicBool>,
    rms_bits: Arc<AtomicU32>,
    peak_bits: Arc<AtomicU32>,
}

impl LevelMeterController {
    pub fn new(enabled: bool) -> Self {
        Self {
            enabled: Arc::new(AtomicBool::new(enabled)),
            rms_bits: Arc::new(AtomicU32::new(0.0_f32.to_bits())),
            peak_bits: Arc::new(AtomicU32::new(0.0_f32.to_bits())),
        }
    }

    /// Enable or disable audio metering. Disabling zeroes the cached levels.
    pub fn set_enabled(&self, enabled: bool) {
        self.enabled.store(enabled, Ordering::Release);
        if !enabled {
            self.reset();
        }
    }

    #[inline]
    pub fn is_enabled(&self) -> bool {
        self.enabled.load(Ordering::Acquire)
    }

    /// Zero the published RMS and peak levels (e.g. when playback pauses/stops).
    pub fn reset(&self) {
        let zero = 0.0_f32.to_bits();
        self.rms_bits.store(zero, Ordering::Release);
        self.peak_bits.store(zero, Ordering::Release);
    }

    /// Read the latest normalized `(rms, peak)` levels in `0.0..=1.0`.
    pub fn levels(&self) -> (f32, f32) {
        if !self.is_enabled() {
            return (0.0, 0.0);
        }
        let rms = sanitize_unit(f32::from_bits(self.rms_bits.load(Ordering::Acquire)));
        let peak = sanitize_unit(f32::from_bits(self.peak_bits.load(Ordering::Acquire)));
        (rms, peak)
    }

    #[inline]
    fn store_levels(&self, rms: f32, peak: f32) {
        self.rms_bits
            .store(sanitize_unit(rms).to_bits(), Ordering::Release);
        self.peak_bits
            .store(sanitize_unit(peak).to_bits(), Ordering::Release);
    }
}

#[inline]
fn sanitize_unit(value: f32) -> f32 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

/// Non-mutating [`Processor`] that publishes audio RMS and peak energy levels
/// to a [`LevelMeterController`] without modifying audio samples.
pub struct LevelMeterProcessor {
    controller: LevelMeterController,
}

impl LevelMeterProcessor {
    pub fn new(controller: LevelMeterController) -> Self {
        Self { controller }
    }
}

impl Processor for LevelMeterProcessor {
    fn name(&self) -> &str {
        "level_meter"
    }

    fn is_active(&self) -> bool {
        self.controller.is_enabled()
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        if !self.controller.is_enabled() {
            return;
        }
        if buffer.samples.is_empty() {
            self.controller.store_levels(0.0, 0.0);
            return;
        }

        let mut sum_sq = 0.0_f64;
        let mut peak = 0.0_f64;
        let mut count = 0_usize;

        for &sample in &buffer.samples {
            if !sample.is_finite() {
                continue;
            }
            let abs = sample.abs();
            if abs > peak {
                peak = abs;
            }
            sum_sq += sample * sample;
            count += 1;
        }

        if count == 0 {
            self.controller.store_levels(0.0, 0.0);
            return;
        }

        let rms = (sum_sq / count as f64).sqrt();
        self.controller.store_levels(rms as f32, peak as f32);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_meter_does_no_work_and_reports_zero() {
        let controller = LevelMeterController::new(false);
        let mut proc = LevelMeterProcessor::new(controller.clone());
        let original = vec![0.5, -0.5, 0.9, -0.9];
        let mut buf = AudioBuffer::stereo_44100(original.clone());

        assert!(!proc.is_active());
        proc.process(&mut buf);

        assert_eq!(buf.samples, original);
        assert_eq!(controller.levels(), (0.0, 0.0));
    }

    #[test]
    fn enabled_meter_tracks_rms_and_peak_without_altering_samples() {
        let controller = LevelMeterController::new(true);
        let mut proc = LevelMeterProcessor::new(controller.clone());
        let original = vec![0.5, -0.5, 0.5, -0.5];
        let mut buf = AudioBuffer::stereo_44100(original.clone());

        assert!(proc.is_active());
        proc.process(&mut buf);

        assert_eq!(buf.samples, original);
        let (rms, peak) = controller.levels();
        assert!((rms - 0.5).abs() < 1e-5);
        assert!((peak - 0.5).abs() < 1e-5);
    }

    #[test]
    fn disabling_meter_clears_previous_readings() {
        let controller = LevelMeterController::new(true);
        let mut proc = LevelMeterProcessor::new(controller.clone());
        let mut buf = AudioBuffer::stereo_44100(vec![1.0, -1.0]);
        proc.process(&mut buf);
        assert!(controller.levels().0 > 0.9);

        controller.set_enabled(false);
        assert_eq!(controller.levels(), (0.0, 0.0));
    }

    #[test]
    fn clamps_clipped_and_handles_empty_or_non_finite_samples() {
        let controller = LevelMeterController::new(true);
        let mut proc = LevelMeterProcessor::new(controller.clone());

        let mut empty = AudioBuffer::stereo_44100(vec![]);
        proc.process(&mut empty);
        assert_eq!(controller.levels(), (0.0, 0.0));

        let mut non_finite = AudioBuffer::stereo_44100(vec![f64::NAN, f64::INFINITY, 2.0, -2.0]);
        proc.process(&mut non_finite);
        let (rms, peak) = controller.levels();
        assert_eq!((rms, peak), (1.0, 1.0));
    }
}
