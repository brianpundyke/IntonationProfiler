use std::collections::VecDeque;
use std::time::Duration;

use crate::aggregate::{aggregate, IntonationReport};
use crate::config::SessionConfig;
use crate::frame::NoteInstance;
use crate::pipeline::{Pipeline, WindowDiagnostic};

/// Maintains a sliding window of recent note instances and recomputes the
/// aggregate report on demand. Notes older than `window` are dropped.
pub struct RollingWindowSession {
    pipeline: Pipeline,
    window: Duration,
    instances: VecDeque<NoteInstance>,
}

impl RollingWindowSession {
    pub fn new(config: &SessionConfig, window: Duration) -> Self {
        RollingWindowSession { pipeline: Pipeline::new(config), window, instances: VecDeque::new() }
    }

    /// Feed raw samples from the capture backend (AudioWorklet block, etc.).
    pub fn push_samples(&mut self, samples: &[f32]) {
        for instance in self.pipeline.push_samples(samples) {
            self.evict_before(instance.start);
            self.instances.push_back(instance);
        }
    }

    /// Same as `push_samples`, but also returns per-window diagnostics
    /// (raw pre-filter detection, whether it was in range) -- for
    /// diagnosing why a note isn't showing up in the report at all.
    pub fn push_samples_debug(&mut self, samples: &[f32]) -> Vec<WindowDiagnostic> {
        let (instances, diagnostics) = self.pipeline.push_samples_debug(samples);
        for instance in instances {
            self.evict_before(instance.start);
            self.instances.push_back(instance);
        }
        diagnostics
    }

    pub fn current_report(&self) -> Option<IntonationReport> {
        let instances: Vec<NoteInstance> = self.instances.iter().cloned().collect();
        aggregate(&instances)
    }

    fn evict_before(&mut self, now: Duration) {
        let cutoff = now.saturating_sub(self.window);
        while let Some(front) = self.instances.front() {
            if front.start < cutoff {
                self.instances.pop_front();
            } else {
                break;
            }
        }
    }
}

/// Accumulates every gated note instance for the duration of a capture,
/// then produces a single full-passage report.
pub struct CaptureSession {
    pipeline: Pipeline,
    instances: Vec<NoteInstance>,
}

impl CaptureSession {
    pub fn new(config: &SessionConfig) -> Self {
        CaptureSession { pipeline: Pipeline::new(config), instances: Vec::new() }
    }

    /// Feed raw samples from the capture backend (AudioWorklet block, etc.).
    pub fn push_samples(&mut self, samples: &[f32]) {
        self.instances.extend(self.pipeline.push_samples(samples));
    }

    /// Same as `push_samples`, but also returns per-window diagnostics
    /// (raw pre-filter detection, whether it was in range) -- for
    /// diagnosing why a note isn't showing up in the report at all.
    pub fn push_samples_debug(&mut self, samples: &[f32]) -> Vec<WindowDiagnostic> {
        let (instances, diagnostics) = self.pipeline.push_samples_debug(samples);
        self.instances.extend(instances);
        diagnostics
    }

    pub fn finish(mut self) -> Option<IntonationReport> {
        if let Some(instance) = self.pipeline.flush() {
            self.instances.push(instance);
        }
        aggregate(&self.instances)
    }
}
