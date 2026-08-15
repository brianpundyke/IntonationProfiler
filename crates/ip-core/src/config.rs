use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ReferencePitch(pub f64);

impl Default for ReferencePitch {
    fn default() -> Self {
        ReferencePitch(440.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FrequencyRange {
    pub min_hz: f64,
    pub max_hz: f64,
}

impl FrequencyRange {
    pub fn contains(&self, hz: f64) -> bool {
        hz >= self.min_hz && hz <= self.max_hz
    }

    /// D4 to C6 -- the nominal playable range on a simple-system D flute --
    /// padded by a whole tone (200 cents) on each side. The padding matters:
    /// this range exists to guard against octave errors (a 1200-cent jump),
    /// not to demand the instrument be in tune, and measuring how far out
    /// of tune it is is the whole point of this tool. Without slack, a
    /// flute running flat would have its real D4 pushed below the nominal
    /// floor and get silently rejected as noise instead of measured. 200
    /// cents covers realistic mistuning with room to spare before it could
    /// be confused with a genuine octave error.
    pub fn d_flute() -> Self {
        const MARGIN_CENTS: f64 = 200.0;
        let factor = 2f64.powf(MARGIN_CENTS / 1200.0);
        FrequencyRange { min_hz: 293.66 / factor, max_hz: 1046.50 * factor }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    McLeod,
    Yin,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateThresholds {
    pub min_note_duration: Duration,
    pub transient_trim: Duration,
    pub tail_trim: Duration,
    /// Does double duty: `Pipeline` passes this to the detector as its
    /// internal peak-selection threshold (how strong a periodicity match
    /// has to be to count as a candidate fundamental at all -- this is
    /// what actually determines whether a weak-fundamental note gets
    /// found), and `Gate` also applies it as a downstream filter on
    /// whatever confidence the detector reports. In practice the second
    /// check is close to a no-op once the first is set correctly, since a
    /// reported detection's confidence is bounded below by the threshold
    /// that let it through -- but both read the same knob so there's one
    /// number to tune, not two with confusing interaction.
    pub clarity_threshold: f64,
    pub power_threshold: f64,
}

impl Default for GateThresholds {
    fn default() -> Self {
        GateThresholds {
            min_note_duration: Duration::from_millis(100),
            transient_trim: Duration::from_millis(50),
            tail_trim: Duration::from_millis(50),
            // 0.8-0.9 is what both McLeod's and YIN's own papers recommend
            // for reliable fundamental-vs-noise/harmonic discrimination --
            // see the doc comment above for why this must not be set low
            // "to be permissive": that breaks fundamental selection itself.
            clarity_threshold: 0.9,
            power_threshold: 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SessionConfig {
    pub reference_pitch: ReferencePitch,
    pub frequency_range: FrequencyRange,
    pub gate: GateThresholds,
    pub algorithm: Algorithm,
    pub sample_rate: usize,
    /// Detection window size, in samples.
    pub window_size: usize,
    /// Stride between successive detection windows, in samples. Smaller
    /// than `window_size` means overlapping windows and finer-grained
    /// frames for the gate's transient/tail trimming to work with.
    pub hop_size: usize,
}

impl Default for SessionConfig {
    fn default() -> Self {
        SessionConfig {
            reference_pitch: ReferencePitch::default(),
            frequency_range: FrequencyRange::d_flute(),
            gate: GateThresholds::default(),
            algorithm: Algorithm::McLeod,
            sample_rate: 44_100,
            // Low notes need proportionally more periods within the window
            // for the detector to find a confident, clean periodicity
            // match -- 4096 gives a 293Hz note (D4) ~27 periods to work
            // with instead of ~13 at the smaller size this used to be.
            window_size: 4096,
            hop_size: 441, // ~10ms at 44.1kHz
        }
    }
}
