use std::time::Duration;

use crate::buffer::WindowBuffer;
use crate::config::SessionConfig;
use crate::detector::Detector;
use crate::frame::{NoteInstance, RawFrame};
use crate::gate::Gate;

/// Diagnostic snapshot of one detection window, before gating. Lets a
/// caller see what the detector actually found even for windows that end
/// up contributing nothing to the report -- e.g. a pitch-halving error
/// (detector locks onto half the true frequency) that lands outside
/// `frequency_range` and would otherwise be indistinguishable from a
/// plain low-confidence rejection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowDiagnostic {
    pub timestamp: Duration,
    /// Root-mean-square amplitude of the window, independent of whether
    /// the detector found anything -- distinguishes "signal too quiet to
    /// work with" from "signal present but pitch tracking got confused".
    pub rms: f64,
    pub raw_hz: Option<f64>,
    pub raw_confidence: Option<f64>,
    pub in_range: bool,
}

/// Wires raw audio ingestion through windowing, pitch detection, and gating
/// to produce note instances. Both session modes share this so the
/// buffer/detector/gate plumbing exists in exactly one place.
pub struct Pipeline {
    buffer: WindowBuffer,
    detector: Detector,
    gate: Gate,
}

impl Pipeline {
    pub fn new(config: &SessionConfig) -> Self {
        Pipeline {
            buffer: WindowBuffer::new(config.window_size, config.hop_size, config.sample_rate),
            detector: Detector::new(
                config.algorithm,
                config.window_size,
                config.sample_rate,
                config.frequency_range,
                config.gate.power_threshold,
                // Same knob Gate uses downstream -- see the doc comment on
                // GateThresholds::clarity_threshold for why these are
                // deliberately the same value rather than two separate
                // ones a caller has to reason about the interaction of.
                config.gate.clarity_threshold,
            ),
            gate: Gate::new(config.gate, config.reference_pitch),
        }
    }

    /// Feed a block of raw samples straight from the capture backend.
    /// Usually returns zero or one note instance; more than one only if
    /// this block completed several detection windows at once.
    pub fn push_samples(&mut self, samples: &[f32]) -> Vec<NoteInstance> {
        self.push_samples_debug(samples).0
    }

    /// Same as `push_samples`, but also returns a diagnostic snapshot of
    /// every window processed, including ones that got filtered out.
    pub fn push_samples_debug(&mut self, samples: &[f32]) -> (Vec<NoteInstance>, Vec<WindowDiagnostic>) {
        let mut instances = Vec::new();
        let mut diagnostics = Vec::new();
        for window in self.buffer.push(samples) {
            let rms = {
                let sum_sq: f64 = window.samples.iter().map(|s| s * s).sum();
                (sum_sq / window.samples.len() as f64).sqrt()
            };
            let raw = self.detector.detect_raw(&window.samples);
            let in_range = raw.as_ref().is_some_and(|p| self.detector.in_range(p.hz));
            let filtered = raw.filter(|_| in_range);

            let closed = match &filtered {
                Some(pitch) => self.gate.push(RawFrame {
                    hz: pitch.hz,
                    confidence: pitch.confidence,
                    timestamp: window.timestamp,
                }),
                None => self.gate.push_silence(),
            };
            instances.extend(closed);
            diagnostics.push(WindowDiagnostic {
                timestamp: window.timestamp,
                rms,
                raw_hz: raw.map(|p| p.hz),
                raw_confidence: raw.map(|p| p.confidence),
                in_range,
            });
        }
        (instances, diagnostics)
    }

    pub fn flush(&mut self) -> Option<NoteInstance> {
        self.gate.flush()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::SessionConfig;
    use crate::note::PitchClass;
    use std::f64::consts::PI;

    #[test]
    fn raw_a4_sine_samples_produce_a_gated_note_instance() {
        let config = SessionConfig::default();
        let mut pipeline = Pipeline::new(&config);

        // 0.5s of A4, delivered in 128-sample AudioWorklet-sized blocks.
        let total_samples = config.sample_rate / 2;
        let sine: Vec<f32> = (0..total_samples)
            .map(|i| (2.0 * PI * 440.0 * i as f64 / config.sample_rate as f64).sin() as f32)
            .collect();

        let mut instances = Vec::new();
        for chunk in sine.chunks(128) {
            instances.extend(pipeline.push_samples(chunk));
        }
        instances.extend(pipeline.flush());

        assert!(!instances.is_empty(), "sustained A4 tone should survive the full pipeline");
        for instance in &instances {
            assert_eq!(instance.note.pitch_class, PitchClass::A);
            assert_eq!(instance.note.octave, 4);
        }
    }

    #[test]
    fn debug_surfaces_a_raw_out_of_range_detection_that_push_samples_hides() {
        // A larger-than-default window: at the (correct) 0.9 peak-selection
        // threshold, 146.83Hz needs more than the default 2048-sample
        // window's ~6.8 periods to clear it confidently -- low frequencies
        // need proportionally more periods per window for a clean peak.
        // 8192 samples gives ~27 periods, comfortably enough.
        let config = SessionConfig { window_size: 8192, hop_size: 2048, ..SessionConfig::default() };
        let mut pipeline = Pipeline::new(&config);

        // Simulates a pitch-halving error: D3 (~146.83Hz) is exactly what a
        // real D4 (~293.66Hz) would read as if the detector locked onto
        // half the true frequency. It's below the D-flute frequency floor,
        // so `push_samples` sees nothing -- but `push_samples_debug` should
        // still show the raw candidate and flag it as out of range.
        let d3_hz = 146.83;
        let sample_rate = config.sample_rate;
        let total_samples = sample_rate; // 1s, plenty for an 8192-sample window
        let sine: Vec<f32> = (0..total_samples)
            .map(|i| (2.0 * PI * d3_hz * i as f64 / sample_rate as f64).sin() as f32)
            .collect();

        let mut instances = Vec::new();
        let mut saw_out_of_range_candidate = false;
        for chunk in sine.chunks(128) {
            let (chunk_instances, diagnostics) = pipeline.push_samples_debug(chunk);
            instances.extend(chunk_instances);
            for d in diagnostics {
                if let Some(hz) = d.raw_hz {
                    assert!(!d.in_range, "D3 ({hz}Hz) should be flagged out of range");
                    assert!((hz - d3_hz).abs() < 2.0, "detected {hz}Hz, expected ~{d3_hz}Hz");
                    saw_out_of_range_candidate = true;
                }
            }
        }

        assert!(saw_out_of_range_candidate, "expected at least one raw D3 candidate to surface");
        assert!(instances.is_empty(), "the filtered path should still report nothing, same as push_samples");
    }

    #[test]
    fn silence_produces_no_note_instances() {
        let config = SessionConfig::default();
        let mut pipeline = Pipeline::new(&config);

        let silence = vec![0.0f32; config.sample_rate]; // 1s of digital silence
        let mut instances = Vec::new();
        for chunk in silence.chunks(128) {
            instances.extend(pipeline.push_samples(chunk));
        }
        instances.extend(pipeline.flush());

        assert!(instances.is_empty());
    }
}
