use std::time::Duration;

use crate::note::NoteName;

/// One pitch-detector output, timestamped relative to session start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RawFrame {
    pub hz: f64,
    pub confidence: f64,
    pub timestamp: Duration,
}

/// A gated, trimmed run of frames recognized as a single sustained note.
#[derive(Debug, Clone, PartialEq)]
pub struct NoteInstance {
    pub note: NoteName,
    pub cents_samples: Vec<f64>,
    pub start: Duration,
    pub duration: Duration,
}
