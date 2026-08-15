use ip_core::{IntonationReport, NoteName, PitchClass, WindowDiagnostic};
use serde::Serialize;
use wasm_bindgen::JsValue;

/// JS-friendly mirror of `IntonationReport`. `ip-core` keeps its report
/// keyed by the Rust-only `NoteName` type in a `HashMap`, neither of which
/// wasm-bindgen can hand across the FFI boundary directly -- that
/// conversion lives here rather than in `ip-core`, so the core stays free
/// of any wasm/serde dependency for the later JNI/egui builds.
#[derive(Serialize)]
struct JsNoteProfile {
    note: String,
    median_cents: f64,
    iqr_cents: f64,
    sample_count: usize,
}

#[derive(Serialize)]
struct JsReport {
    global_offset_cents: f64,
    per_note: Vec<JsNoteProfile>,
}

pub fn to_js_value(report: IntonationReport) -> JsValue {
    let mut per_note: Vec<JsNoteProfile> = report
        .per_note
        .into_iter()
        .map(|(note, profile)| JsNoteProfile {
            note: format_note_name(note),
            median_cents: profile.median_cents,
            iqr_cents: profile.iqr_cents,
            sample_count: profile.sample_count,
        })
        .collect();
    per_note.sort_by(|a, b| a.note.cmp(&b.note));

    let js_report = JsReport { global_offset_cents: report.global_offset_cents, per_note };
    serde_wasm_bindgen::to_value(&js_report).expect("IntonationReport is always representable as JSON")
}

#[derive(Serialize)]
struct JsWindowDiagnostic {
    timestamp_ms: f64,
    rms: f64,
    raw_hz: Option<f64>,
    raw_confidence: Option<f64>,
    in_range: bool,
}

pub fn diagnostics_to_js_value(diagnostics: Vec<WindowDiagnostic>) -> JsValue {
    let js_diagnostics: Vec<JsWindowDiagnostic> = diagnostics
        .into_iter()
        .map(|d| JsWindowDiagnostic {
            timestamp_ms: d.timestamp.as_secs_f64() * 1000.0,
            rms: d.rms,
            raw_hz: d.raw_hz,
            raw_confidence: d.raw_confidence,
            in_range: d.in_range,
        })
        .collect();
    serde_wasm_bindgen::to_value(&js_diagnostics).expect("WindowDiagnostic is always representable as JSON")
}

fn format_note_name(note: NoteName) -> String {
    let pitch_class = match note.pitch_class {
        PitchClass::C => "C",
        PitchClass::CSharp => "C#",
        PitchClass::D => "D",
        PitchClass::DSharp => "D#",
        PitchClass::E => "E",
        PitchClass::F => "F",
        PitchClass::FSharp => "F#",
        PitchClass::G => "G",
        PitchClass::GSharp => "G#",
        PitchClass::A => "A",
        PitchClass::ASharp => "A#",
        PitchClass::B => "B",
    };
    format!("{pitch_class}{}", note.octave)
}
