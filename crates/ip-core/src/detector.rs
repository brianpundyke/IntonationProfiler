use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::yin::YINDetector;
use pitch_detection::detector::PitchDetector as ExternalPitchDetector;

use crate::config::{Algorithm, FrequencyRange};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DetectedPitch {
    pub hz: f64,
    /// Unified confidence in [0, 1], higher = more periodic/confident,
    /// regardless of which underlying algorithm produced it.
    pub confidence: f64,
}

enum Inner {
    McLeod(McLeodDetector<f64>),
    Yin(YINDetector<f64>),
}

/// Wraps the `pitch-detection` crate's detectors behind one interface so
/// the algorithm is a runtime config choice, not a call-site rewrite.
pub struct Detector {
    inner: Inner,
    sample_rate: usize,
    power_threshold: f64,
    clarity_threshold: f64,
    frequency_range: FrequencyRange,
}

impl Detector {
    pub fn new(
        algorithm: Algorithm,
        window_size: usize,
        sample_rate: usize,
        frequency_range: FrequencyRange,
        power_threshold: f64,
        clarity_threshold: f64,
    ) -> Self {
        let padding = window_size / 2;
        let inner = match algorithm {
            Algorithm::McLeod => Inner::McLeod(McLeodDetector::new(window_size, padding)),
            Algorithm::Yin => Inner::Yin(YINDetector::new(window_size, padding)),
        };
        Detector { inner, sample_rate, power_threshold, clarity_threshold, frequency_range }
    }

    /// Returns `None` if nothing periodic enough was found, or if the best
    /// candidate falls outside `frequency_range` -- most often an octave
    /// error rather than a real note in that register.
    pub fn detect(&mut self, signal: &[f64]) -> Option<DetectedPitch> {
        let pitch = self.detect_raw(signal)?;
        if !self.in_range(pitch.hz) {
            return None;
        }
        Some(pitch)
    }

    /// The underlying detector's best guess, before the frequency-range
    /// filter. Diagnostic use only -- lets a caller see e.g. a pitch-halving
    /// error (detector locks onto half the true frequency) that `detect`
    /// would otherwise report as nothing at all, indistinguishable from a
    /// low-confidence rejection.
    pub fn detect_raw(&mut self, signal: &[f64]) -> Option<DetectedPitch> {
        let pitch = match &mut self.inner {
            Inner::McLeod(d) => {
                d.get_pitch(signal, self.sample_rate, self.power_threshold, self.clarity_threshold)
            }
            Inner::Yin(d) => {
                d.get_pitch(signal, self.sample_rate, self.power_threshold, self.clarity_threshold)
            }
        }?;
        Some(DetectedPitch { hz: pitch.frequency, confidence: pitch.clarity })
    }

    pub fn in_range(&self, hz: f64) -> bool {
        self.frequency_range.contains(hz)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    fn sine_wave(freq: f64, sample_rate: usize, n: usize) -> Vec<f64> {
        (0..n).map(|i| (2.0 * PI * freq * i as f64 / sample_rate as f64).sin()).collect()
    }

    /// A fundamental mixed with a *stronger* 2nd harmonic -- the shape of
    /// signal a weak-fundamental wind-instrument note produces, and
    /// exactly the case a broken peak-selection threshold gets wrong.
    fn fundamental_plus_stronger_second_harmonic(fundamental: f64, sample_rate: usize) -> Vec<f64> {
        (0..2048)
            .map(|i| {
                let t = i as f64 / sample_rate as f64;
                0.3 * (2.0 * PI * fundamental * t).sin() + 0.7 * (2.0 * PI * (fundamental * 2.0) * t).sin()
            })
            .collect()
    }

    #[test]
    fn sane_threshold_prefers_the_fundamental_over_a_stronger_second_harmonic() {
        let sample_rate = 44_100;
        let fundamental = 293.66; // D4
        let signal = fundamental_plus_stronger_second_harmonic(fundamental, sample_rate);

        let mut detector = Detector::new(
            Algorithm::McLeod,
            2048,
            sample_rate,
            FrequencyRange::d_flute(),
            5.0,
            0.9, // the fixed, algorithm-sane peak-selection threshold
        );

        let result = detector.detect(&signal).expect("should find a periodic signal");
        assert!(
            (result.hz - fundamental).abs() < 5.0,
            "expected to lock onto the fundamental ({fundamental}Hz), got {}Hz -- likely picked the \
             stronger harmonic instead",
            result.hz
        );
    }

    #[test]
    fn near_zero_threshold_locks_onto_the_harmonic_instead() {
        // This is the actual bug `Pipeline` used to have: at a near-zero
        // peak-selection threshold, `choose_peak` isn't discriminating
        // fundamental from harmonic at all -- it grabs the first (shortest
        // lag, highest frequency) peak it finds, which for this signal is
        // the stronger 2nd harmonic, not the true fundamental.
        let sample_rate = 44_100;
        let fundamental = 293.66;
        let second_harmonic = fundamental * 2.0;
        let signal = fundamental_plus_stronger_second_harmonic(fundamental, sample_rate);

        let mut detector =
            Detector::new(Algorithm::McLeod, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.0);

        let result = detector.detect(&signal).expect("should find a periodic signal");
        assert!(
            (result.hz - second_harmonic).abs() < 5.0,
            "expected the near-zero threshold to mistakenly pick the harmonic ({second_harmonic}Hz), \
             got {}Hz",
            result.hz
        );
    }

    #[test]
    fn mcleod_detects_a4_sine_wave() {
        let sample_rate = 44_100;
        let signal = sine_wave(440.0, sample_rate, 2048);
        let mut detector =
            Detector::new(Algorithm::McLeod, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.6);

        let result = detector.detect(&signal).expect("should detect a clean sine tone");
        assert!((result.hz - 440.0).abs() < 2.0, "detected {} Hz", result.hz);
        assert!(result.confidence > 0.9);
    }

    #[test]
    fn yin_detects_a4_sine_wave() {
        let sample_rate = 44_100;
        let signal = sine_wave(440.0, sample_rate, 2048);
        let mut detector =
            Detector::new(Algorithm::Yin, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.6);

        let result = detector.detect(&signal).expect("should detect a clean sine tone");
        assert!((result.hz - 440.0).abs() < 2.0, "detected {} Hz", result.hz);
    }

    #[test]
    fn rejects_a_confident_pitch_outside_the_configured_range() {
        let sample_rate = 44_100;
        // 150Hz is well below the D-flute range (D4 = 293.66Hz) but is a
        // perfectly clean, easily-detected tone in isolation -- exactly the
        // kind of confident-but-wrong result an octave error looks like.
        let signal = sine_wave(150.0, sample_rate, 2048);
        let mut detector =
            Detector::new(Algorithm::McLeod, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.6);

        assert!(detector.detect(&signal).is_none());
    }

    #[test]
    fn a_flat_instrument_still_gets_measured_near_the_low_edge() {
        let sample_rate = 44_100;
        // A real D4 played on a flute running ~80 cents flat: still clearly
        // a D4, not an octave error, and must not be clipped by the guard.
        let flat_d4 = 293.66 * 2f64.powf(-80.0 / 1200.0);
        let signal = sine_wave(flat_d4, sample_rate, 2048);
        let mut detector =
            Detector::new(Algorithm::McLeod, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.6);

        let result = detector.detect(&signal).expect("mistuned but real note should survive");
        assert!((result.hz - flat_d4).abs() < 2.0);
    }

    #[test]
    fn a_sharp_instrument_still_gets_measured_near_the_high_edge() {
        let sample_rate = 44_100;
        // A real C6 played on a flute running ~80 cents sharp.
        let sharp_c6 = 1046.50 * 2f64.powf(80.0 / 1200.0);
        let signal = sine_wave(sharp_c6, sample_rate, 2048);
        let mut detector =
            Detector::new(Algorithm::McLeod, 2048, sample_rate, FrequencyRange::d_flute(), 5.0, 0.6);

        let result = detector.detect(&signal).expect("mistuned but real note should survive");
        assert!((result.hz - sharp_c6).abs() < 2.0);
    }
}
