use std::collections::HashMap;

use crate::frame::NoteInstance;
use crate::note::NoteName;

#[derive(Debug, Clone, PartialEq)]
pub struct NoteProfile {
    pub median_cents: f64,
    pub iqr_cents: f64,
    pub sample_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntonationReport {
    /// Median of per-note medians: the "push the slide in / pull it out"
    /// verdict, unbiased by which notes happened to occur most often.
    pub global_offset_cents: f64,
    pub per_note: HashMap<NoteName, NoteProfile>,
}

/// Two-stage aggregation: median+IQR per note bin, then median across those
/// per-note medians for the global offset. `None` if no notes survived
/// gating.
pub fn aggregate(instances: &[NoteInstance]) -> Option<IntonationReport> {
    if instances.is_empty() {
        return None;
    }

    let mut bins: HashMap<NoteName, Vec<f64>> = HashMap::new();
    for instance in instances {
        bins.entry(instance.note).or_default().extend(&instance.cents_samples);
    }

    let mut per_note = HashMap::with_capacity(bins.len());
    let mut medians = Vec::with_capacity(bins.len());
    for (note, mut samples) in bins {
        samples.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let median_cents = percentile(&samples, 0.5);
        let iqr_cents = percentile(&samples, 0.75) - percentile(&samples, 0.25);
        let sample_count = samples.len();
        medians.push(median_cents);
        per_note.insert(note, NoteProfile { median_cents, iqr_cents, sample_count });
    }

    medians.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let global_offset_cents = percentile(&medians, 0.5);

    Some(IntonationReport { global_offset_cents, per_note })
}

/// Linear-interpolation percentile (0.0..=1.0) over an already-sorted slice.
fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.len() == 1 {
        return sorted[0];
    }
    let idx = p * (sorted.len() - 1) as f64;
    let lo = idx.floor() as usize;
    let hi = idx.ceil() as usize;
    if lo == hi {
        return sorted[lo];
    }
    let frac = idx - lo as f64;
    sorted[lo] * (1.0 - frac) + sorted[hi] * frac
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::note::PitchClass;

    fn note(pitch_class: PitchClass, octave: i32) -> NoteName {
        NoteName { pitch_class, octave }
    }

    fn instance(n: NoteName, cents_samples: Vec<f64>) -> NoteInstance {
        NoteInstance {
            note: n,
            cents_samples,
            start: std::time::Duration::ZERO,
            duration: std::time::Duration::from_millis(200),
        }
    }

    #[test]
    fn empty_input_yields_no_report() {
        assert!(aggregate(&[]).is_none());
    }

    #[test]
    fn median_and_iqr_on_a_single_note() {
        let d5 = note(PitchClass::D, 5);
        let samples: Vec<f64> = (1..=9).map(|n| n as f64).collect(); // 1..9
        let report = aggregate(&[instance(d5, samples)]).unwrap();

        let profile = &report.per_note[&d5];
        assert_eq!(profile.sample_count, 9);
        assert_eq!(profile.median_cents, 5.0);
        assert_eq!(profile.iqr_cents, 4.0); // Q3=7, Q1=3
        assert_eq!(report.global_offset_cents, 5.0); // only one note bin
    }

    #[test]
    fn even_count_median_interpolates() {
        let a4 = note(PitchClass::A, 4);
        let report = aggregate(&[instance(a4, vec![1.0, 2.0, 3.0, 4.0])]).unwrap();
        assert_eq!(report.per_note[&a4].median_cents, 2.5);
    }

    #[test]
    fn multiple_instances_of_the_same_note_pool_into_one_bin() {
        let d5 = note(PitchClass::D, 5);
        let report = aggregate(&[
            instance(d5, vec![0.0, 2.0]),
            instance(d5, vec![4.0, 6.0]),
        ])
        .unwrap();
        assert_eq!(report.per_note[&d5].sample_count, 4);
        assert_eq!(report.per_note[&d5].median_cents, 3.0);
    }

    #[test]
    fn global_offset_is_unbiased_by_how_often_a_note_occurs() {
        // A common, perfectly-in-tune note (100 frames) alongside two rare
        // problem notes (5 frames each). A frame-weighted mean would be
        // dragged toward the common note; the two-stage median must not be.
        let d5_common = note(PitchClass::D, 5);
        let f_sharp5_rare = note(PitchClass::FSharp, 5);
        let d4_rare = note(PitchClass::D, 4);

        let instances = vec![
            instance(d5_common, vec![0.0; 100]),
            instance(f_sharp5_rare, vec![30.0; 5]),
            instance(d4_rare, vec![10.0; 5]),
        ];

        let report = aggregate(&instances).unwrap();

        // Median of the three per-note medians [0, 10, 30] is 10.
        assert_eq!(report.global_offset_cents, 10.0);

        let naive_frame_weighted_mean: f64 = {
            let total: f64 = instances.iter().flat_map(|i| i.cents_samples.iter()).sum();
            let count: usize = instances.iter().map(|i| i.cents_samples.len()).sum();
            total / count as f64
        };
        assert!(
            (report.global_offset_cents - naive_frame_weighted_mean).abs() > 5.0,
            "two-stage median ({}) should diverge sharply from the naive frame-weighted mean ({})",
            report.global_offset_cents,
            naive_frame_weighted_mean
        );
    }

    #[test]
    fn iqr_reflects_spread_not_just_center() {
        let a4 = note(PitchClass::A, 4);
        let tight = note(PitchClass::A, 5);

        let report = aggregate(&[
            instance(a4, vec![-15.0, -10.0, 0.0, 10.0, 15.0]),
            instance(tight, vec![-1.0, -0.5, 0.0, 0.5, 1.0]),
        ])
        .unwrap();

        assert_eq!(report.per_note[&a4].median_cents, 0.0);
        assert_eq!(report.per_note[&tight].median_cents, 0.0);
        assert!(report.per_note[&a4].iqr_cents > report.per_note[&tight].iqr_cents);
    }
}
