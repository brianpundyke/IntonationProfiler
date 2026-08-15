pub mod aggregate;
pub mod buffer;
pub mod config;
pub mod detector;
pub mod frame;
pub mod gate;
pub mod note;
pub mod pipeline;
pub mod session;

pub use aggregate::{IntonationReport, NoteProfile};
pub use buffer::{Window, WindowBuffer};
pub use config::{Algorithm, FrequencyRange, GateThresholds, ReferencePitch, SessionConfig};
pub use detector::{DetectedPitch, Detector};
pub use frame::{NoteInstance, RawFrame};
pub use gate::Gate;
pub use note::{NoteAndCents, NoteName, PitchClass};
pub use pipeline::{Pipeline, WindowDiagnostic};
pub use session::{CaptureSession, RollingWindowSession};
