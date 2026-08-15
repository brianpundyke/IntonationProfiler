use std::time::Duration;

use crate::config::{GateThresholds, ReferencePitch};
use crate::frame::{NoteInstance, RawFrame};
use crate::note::{hz_to_note, NoteAndCents, NoteName};

/// Turns a stream of raw pitch-detector frames into gated [`NoteInstance`]s:
/// rejects low-confidence frames, rejects runs shorter than the ornament
/// threshold, and trims the attack transient and phrase-end tail off
/// whatever survives.
pub struct Gate {
    thresholds: GateThresholds,
    reference: ReferencePitch,
    running: Option<RunningNote>,
}

struct RunningNote {
    note: NoteName,
    start: Duration,
    last_timestamp: Duration,
    samples: Vec<(Duration, f64)>,
}

impl Gate {
    pub fn new(thresholds: GateThresholds, reference: ReferencePitch) -> Self {
        Gate { thresholds, reference, running: None }
    }

    /// Feed one detector frame. Returns a completed note instance whenever
    /// this frame closes out the previous run (note change, confidence
    /// drop) and that run survived gating.
    pub fn push(&mut self, frame: RawFrame) -> Option<NoteInstance> {
        if frame.confidence < self.thresholds.clarity_threshold {
            return self.end_run();
        }

        let NoteAndCents { note, cents } = hz_to_note(frame.hz, self.reference);

        match &mut self.running {
            Some(running) if running.note == note => {
                running.last_timestamp = frame.timestamp;
                running.samples.push((frame.timestamp, cents));
                None
            }
            _ => {
                let finished = self.end_run();
                self.running = Some(RunningNote {
                    note,
                    start: frame.timestamp,
                    last_timestamp: frame.timestamp,
                    samples: vec![(frame.timestamp, cents)],
                });
                finished
            }
        }
    }

    /// Close out whatever note is in progress, e.g. at end of a capture.
    pub fn flush(&mut self) -> Option<NoteInstance> {
        self.end_run()
    }

    /// No pitch was detected at all for a window (silence, or below the
    /// detector's power threshold). Same effect as a low-confidence frame:
    /// whatever note was running gets closed out rather than left dangling
    /// across the gap.
    pub fn push_silence(&mut self) -> Option<NoteInstance> {
        self.end_run()
    }

    fn end_run(&mut self) -> Option<NoteInstance> {
        let running = self.running.take()?;
        self.finalize(running)
    }

    fn finalize(&self, running: RunningNote) -> Option<NoteInstance> {
        let duration = running.last_timestamp.saturating_sub(running.start);
        if duration < self.thresholds.min_note_duration {
            return None;
        }

        let head_cutoff = running.start + self.thresholds.transient_trim;
        let tail_cutoff = running.start + duration - self.thresholds.tail_trim.min(duration);

        let cents_samples: Vec<f64> = running
            .samples
            .iter()
            .filter(|(ts, _)| *ts >= head_cutoff && *ts <= tail_cutoff)
            .map(|(_, cents)| *cents)
            .collect();

        if cents_samples.is_empty() {
            return None;
        }

        Some(NoteInstance { note: running.note, cents_samples, start: running.start, duration })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::PitchClass;

    const D5_HZ: f64 = 587.33;
    const A4_HZ: f64 = 440.0;

    fn frame(hz: f64, confidence: f64, ts_ms: u64) -> RawFrame {
        RawFrame { hz, confidence, timestamp: Duration::from_millis(ts_ms) }
    }

    fn new_gate() -> Gate {
        Gate::new(GateThresholds::default(), ReferencePitch::default())
    }

    #[test]
    fn rejects_run_shorter_than_ornament_threshold() {
        let mut gate = new_gate();
        // 50ms run: a cut or tap, not a sustained note.
        for ts in [0, 10, 20, 30, 40, 50] {
            assert!(gate.push(frame(D5_HZ, 0.9, ts)).is_none());
        }
        // Low-confidence frame ends the run; still must not surface it.
        assert!(gate.push(frame(D5_HZ, 0.1, 60)).is_none());
        assert!(gate.flush().is_none());
    }

    #[test]
    fn trims_transient_and_tail_from_sustained_note() {
        let mut gate = new_gate();
        let mut result = None;

        for ts_ms in (0..=300).step_by(10) {
            let cents = if ts_ms < 50 {
                40.0 // attack transient, should be trimmed
            } else if ts_ms > 250 {
                -40.0 // phrase-end tail, should be trimmed
            } else {
                5.0 // steady body of the note
            };
            let hz = A4_HZ * 2f64.powf(cents / 1200.0);
            if let Some(instance) = gate.push(frame(hz, 0.9, ts_ms)) {
                result = Some(instance);
            }
        }
        result = gate.flush().or(result);

        let instance = result.expect("sustained note should survive gating");
        assert_eq!(instance.note.pitch_class, PitchClass::A);
        assert_eq!(instance.note.octave, 4);
        assert_eq!(instance.duration, Duration::from_millis(300));
        assert!(!instance.cents_samples.is_empty());
        for cents in instance.cents_samples {
            assert!((cents - 5.0).abs() < 1.0, "transient/tail sample leaked through: {cents}");
        }
    }

    #[test]
    fn note_change_finalizes_previous_run() {
        let mut gate = new_gate();
        for ts in [0, 50, 100, 150] {
            assert!(gate.push(frame(D5_HZ, 0.9, ts)).is_none());
        }
        // A different pitch class closes out the D5 run.
        let finished = gate.push(frame(A4_HZ, 0.9, 160));
        let instance = finished.expect("note change should finalize the prior run");
        assert_eq!(instance.note.pitch_class, PitchClass::D);
        assert_eq!(instance.note.octave, 5);
    }

    #[test]
    fn low_confidence_frames_never_start_a_run() {
        let mut gate = new_gate();
        for ts in [0, 10, 20, 30] {
            assert!(gate.push(frame(D5_HZ, 0.2, ts)).is_none());
        }
        assert!(gate.flush().is_none());
    }

    #[test]
    fn silence_ends_a_running_note_like_low_confidence_does() {
        let mut gate = new_gate();
        for ts in [0, 50, 100, 150] {
            assert!(gate.push(frame(D5_HZ, 0.9, ts)).is_none());
        }
        let instance = gate.push_silence().expect("silence should close out the run");
        assert_eq!(instance.note.pitch_class, PitchClass::D);
        assert_eq!(instance.note.octave, 5);
    }

    #[test]
    fn silence_with_nothing_running_is_a_no_op() {
        let mut gate = new_gate();
        assert!(gate.push_silence().is_none());
    }

    #[test]
    fn exact_boundary_duration_is_accepted() {
        let mut gate = new_gate();
        // 100ms exactly meets the >= min_note_duration bar.
        assert!(gate.push(frame(A4_HZ, 0.9, 0)).is_none());
        assert!(gate.push(frame(A4_HZ, 0.9, 50)).is_none());
        assert!(gate.push(frame(A4_HZ, 0.9, 100)).is_none());

        let instance = gate.flush().expect("100ms run should be accepted, not rejected");
        assert_eq!(instance.duration, Duration::from_millis(100));
        // transient_trim=50, tail_trim=50 on a 100ms note leaves only the t=50 sample.
        assert_eq!(instance.cents_samples.len(), 1);
    }
}
