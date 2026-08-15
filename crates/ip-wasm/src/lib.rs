mod report;

use std::time::Duration;

use ip_core::{CaptureSession, ReferencePitch, RollingWindowSession, SessionConfig};
use wasm_bindgen::prelude::*;

use report::{diagnostics_to_js_value, to_js_value};

/// `sample_rate` must match the capturing `AudioContext`'s actual rate
/// (commonly 44100 or 48000 depending on device/browser, never assumed).
/// `clarity_threshold`/`power_threshold`/`window_size` override `ip-core`'s
/// defaults when provided -- real instruments and mics vary enough that
/// these need to be tunable without a rebuild (window size in particular:
/// weak-fundamental low notes need more periods per window than a strong
/// note like A4 does). Instrument frequency range and detection algorithm
/// still use `ip-core`'s defaults for now.
fn build_config(
    sample_rate: u32,
    reference_hz: f64,
    clarity_threshold: Option<f64>,
    power_threshold: Option<f64>,
    window_size: Option<u32>,
) -> SessionConfig {
    let mut config = SessionConfig {
        sample_rate: sample_rate as usize,
        reference_pitch: ReferencePitch(reference_hz),
        ..SessionConfig::default()
    };
    if let Some(c) = clarity_threshold {
        config.gate.clarity_threshold = c;
    }
    if let Some(p) = power_threshold {
        config.gate.power_threshold = p;
    }
    if let Some(w) = window_size {
        config.window_size = w as usize;
    }
    config
}

#[wasm_bindgen]
pub struct RollingSession {
    inner: RollingWindowSession,
}

#[wasm_bindgen]
impl RollingSession {
    #[wasm_bindgen(constructor)]
    pub fn new(
        sample_rate: u32,
        reference_hz: f64,
        window_seconds: f64,
        clarity_threshold: Option<f64>,
        power_threshold: Option<f64>,
        detection_window_size: Option<u32>,
    ) -> RollingSession {
        let config =
            build_config(sample_rate, reference_hz, clarity_threshold, power_threshold, detection_window_size);
        let window = Duration::from_secs_f64(window_seconds);
        RollingSession { inner: RollingWindowSession::new(&config, window) }
    }

    /// Feed raw PCM samples straight from an AudioWorklet block.
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.inner.push_samples(samples);
    }

    /// Debug variant of `push_samples`: also returns a JSON array of every
    /// detection window's raw pre-filter result -- for diagnosing why a
    /// note isn't showing up in the report (rejected as low-confidence?
    /// found but filtered as out of range, e.g. a pitch-halving error?
    /// never detected as periodic at all?) rather than guessing.
    pub fn push_samples_debug(&mut self, samples: &[f32]) -> JsValue {
        diagnostics_to_js_value(self.inner.push_samples_debug(samples))
    }

    /// Current rolling-window report, or `null` if nothing has been gated
    /// in yet.
    pub fn current_report(&self) -> JsValue {
        self.inner.current_report().map(to_js_value).unwrap_or(JsValue::NULL)
    }
}

#[wasm_bindgen]
pub struct Capture {
    inner: CaptureSession,
}

#[wasm_bindgen]
impl Capture {
    #[wasm_bindgen(constructor)]
    pub fn new(
        sample_rate: u32,
        reference_hz: f64,
        clarity_threshold: Option<f64>,
        power_threshold: Option<f64>,
        detection_window_size: Option<u32>,
    ) -> Capture {
        let config =
            build_config(sample_rate, reference_hz, clarity_threshold, power_threshold, detection_window_size);
        Capture { inner: CaptureSession::new(&config) }
    }

    /// Feed raw PCM samples straight from an AudioWorklet block.
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.inner.push_samples(samples);
    }

    /// Debug variant of `push_samples`: also returns a JSON array of every
    /// detection window's raw pre-filter result. See
    /// `RollingSession::push_samples_debug` for why this exists.
    pub fn push_samples_debug(&mut self, samples: &[f32]) -> JsValue {
        diagnostics_to_js_value(self.inner.push_samples_debug(samples))
    }

    /// Consumes the capture and returns the full-passage report, or `null`
    /// if no note survived gating. The JS object is unusable after this.
    pub fn finish(self) -> JsValue {
        self.inner.finish().map(to_js_value).unwrap_or(JsValue::NULL)
    }
}
