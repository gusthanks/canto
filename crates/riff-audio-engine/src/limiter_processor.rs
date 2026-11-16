//! Peak limiter, the last stage before the level meter and the backend.
//!
//! Equalizer boosts and pitch shifting can push samples past full scale, which
//! the backend would hard-clip into audible distortion. The limiter lowers the
//! gain just enough to keep every frame under the ceiling, then lets it
//! recover. Audio that never reaches the ceiling passes through bit-for-bit.

use super::{AudioBuffer, Processor};

/// The loudest a sample may be after limiting (about -0.2 dBFS).
pub const LIMITER_CEILING: f64 = 0.977;

/// How long the gain takes to recover most of the way after a peak.
const RELEASE_SECONDS: f64 = 0.15;

/// A [`Processor`] that keeps samples under [`LIMITER_CEILING`].
pub struct LimiterProcessor {
    gain: f64,
    release_rate: f64,
    sample_rate: u32,
}

impl LimiterProcessor {
    pub fn new() -> Self {
        Self {
            gain: 1.0,
            release_rate: 0.0,
            sample_rate: 0,
        }
    }

    fn update_release(&mut self, sample_rate: u32) {
        if sample_rate != self.sample_rate && sample_rate > 0 {
            self.sample_rate = sample_rate;
            // Per-frame step that closes ~63% of the gap in RELEASE_SECONDS
            self.release_rate = 1.0 - (-1.0 / (RELEASE_SECONDS * f64::from(sample_rate))).exp();
        }
    }
}

impl Default for LimiterProcessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Processor for LimiterProcessor {
    fn name(&self) -> &str {
        "limiter"
    }

    fn is_active(&self) -> bool {
        self.gain < 1.0
    }

    fn process(&mut self, buffer: &mut AudioBuffer) {
        self.update_release(buffer.sample_rate);
        let channels = usize::from(buffer.channels.max(1));
        for frame in buffer.samples.chunks_mut(channels) {
            let peak = frame.iter().fold(0.0_f64, |peak, s| peak.max(s.abs()));
            // Fast down: never let a frame through over the ceiling
            if peak * self.gain > LIMITER_CEILING {
                self.gain = LIMITER_CEILING / peak;
            }
            if self.gain < 1.0 {
                for sample in frame.iter_mut() {
                    *sample *= self.gain;
                }
                // Slow up
                self.gain += (1.0 - self.gain) * self.release_rate;
                if self.gain > 0.9999 {
                    self.gain = 1.0;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(amplitude: f64, frames: usize) -> Vec<f64> {
        (0..frames)
            .flat_map(|i| {
                let s = amplitude * (i as f64 * 0.05).sin();
                [s, s]
            })
            .collect()
    }

    #[test]
    fn quiet_audio_passes_untouched() {
        let samples = sine(0.8, 4410);
        let mut buffer = AudioBuffer::stereo_44100(samples.clone());
        let mut limiter = LimiterProcessor::new();
        limiter.process(&mut buffer);
        assert_eq!(buffer.samples, samples);
        assert!(!limiter.is_active());
    }

    #[test]
    fn loud_audio_stays_under_the_ceiling() {
        let mut buffer = AudioBuffer::stereo_44100(sine(1.8, 4410));
        let mut limiter = LimiterProcessor::new();
        limiter.process(&mut buffer);
        let peak = buffer.samples.iter().fold(0.0_f64, |p, s| p.max(s.abs()));
        assert!(peak <= LIMITER_CEILING + 1e-12, "peak {peak}");
        assert!(peak > 0.9, "the limiter shouldn't squash the audio: {peak}");
    }

    #[test]
    fn gain_recovers_after_a_peak() {
        let mut limiter = LimiterProcessor::new();
        let mut spike = AudioBuffer::stereo_44100(vec![2.0, 2.0]);
        limiter.process(&mut spike);
        assert!(limiter.is_active());
        // Two seconds of silence bring the gain back to unity
        let mut silence = AudioBuffer::stereo_44100(vec![0.0; 44100 * 2 * 2]);
        limiter.process(&mut silence);
        assert!(!limiter.is_active());
        let samples = sine(0.5, 100);
        let mut after = AudioBuffer::stereo_44100(samples.clone());
        limiter.process(&mut after);
        assert_eq!(after.samples, samples);
    }
}
