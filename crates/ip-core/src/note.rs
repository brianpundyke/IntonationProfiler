use crate::config::ReferencePitch;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PitchClass {
    C,
    CSharp,
    D,
    DSharp,
    E,
    F,
    FSharp,
    G,
    GSharp,
    A,
    ASharp,
    B,
}

impl PitchClass {
    const ORDER: [PitchClass; 12] = [
        PitchClass::C,
        PitchClass::CSharp,
        PitchClass::D,
        PitchClass::DSharp,
        PitchClass::E,
        PitchClass::F,
        PitchClass::FSharp,
        PitchClass::G,
        PitchClass::GSharp,
        PitchClass::A,
        PitchClass::ASharp,
        PitchClass::B,
    ];

    fn from_semitone(semitone: i32) -> PitchClass {
        Self::ORDER[semitone.rem_euclid(12) as usize]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NoteName {
    pub pitch_class: PitchClass,
    pub octave: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NoteAndCents {
    pub note: NoteName,
    pub cents: f64,
}

/// Maps a frequency to the nearest note name and its cents deviation from
/// that note's equal-tempered pitch, given a reference A4 pitch.
pub fn hz_to_note(hz: f64, reference: ReferencePitch) -> NoteAndCents {
    let semitones_from_a4 = 12.0 * (hz / reference.0).log2();
    let nearest_semitone = semitones_from_a4.round() as i32;
    let cents = (semitones_from_a4 - nearest_semitone as f64) * 100.0;

    // A4 is semitone offset 0; MIDI-style numbering with C at the start of
    // the octave means A sits at semitone index 9 within its octave.
    let semitone_from_c0 = nearest_semitone + 9 + 4 * 12;
    let octave = semitone_from_c0.div_euclid(12);
    let pitch_class = PitchClass::from_semitone(semitone_from_c0);

    NoteAndCents { note: NoteName { pitch_class, octave }, cents }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f64, b: f64, tol: f64) {
        assert!((a - b).abs() < tol, "{a} not within {tol} of {b}");
    }

    #[test]
    fn a4_is_exact() {
        let result = hz_to_note(440.0, ReferencePitch::default());
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::A, octave: 4 });
        assert_close(result.cents, 0.0, 1e-6);
    }

    #[test]
    fn a3_one_octave_down() {
        let result = hz_to_note(220.0, ReferencePitch::default());
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::A, octave: 3 });
        assert_close(result.cents, 0.0, 1e-6);
    }

    #[test]
    fn d5_flute_open_note() {
        // D5 at equal temperament from A440 is ~587.33 Hz.
        let result = hz_to_note(587.33, ReferencePitch::default());
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::D, octave: 5 });
        assert_close(result.cents, 0.0, 1.0);
    }

    #[test]
    fn sharp_deviation_is_positive() {
        // 445 Hz is sharp of A4 (440 Hz) by roughly +19.6 cents.
        let result = hz_to_note(445.0, ReferencePitch::default());
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::A, octave: 4 });
        assert_close(result.cents, 19.56, 0.1);
    }

    #[test]
    fn flat_deviation_is_negative() {
        let result = hz_to_note(435.0, ReferencePitch::default());
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::A, octave: 4 });
        assert!(result.cents < 0.0);
    }

    #[test]
    fn respects_alternate_reference_pitch() {
        // With A=442, 442 Hz itself should read as dead-on A4.
        let result = hz_to_note(442.0, ReferencePitch(442.0));
        assert_eq!(result.note, NoteName { pitch_class: PitchClass::A, octave: 4 });
        assert_close(result.cents, 0.0, 1e-6);
    }

    #[test]
    fn octave_boundary_at_c() {
        // C5 is one semitone above B4.
        let c5 = hz_to_note(523.25, ReferencePitch::default());
        assert_eq!(c5.note, NoteName { pitch_class: PitchClass::C, octave: 5 });

        let b4 = hz_to_note(493.88, ReferencePitch::default());
        assert_eq!(b4.note, NoteName { pitch_class: PitchClass::B, octave: 4 });
    }
}
