use std::collections::VecDeque;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq)]
pub struct Window {
    pub samples: Vec<f64>,
    pub timestamp: Duration,
}

/// Accumulates arbitrary-sized raw sample blocks (as delivered by an
/// AudioWorklet, cpal callback, JNI AudioRecord, etc.) into fixed-size,
/// overlapping windows suitable for pitch detection. Keeping this in Rust
/// means every capture backend shares one windowing implementation instead
/// of each platform reimplementing it.
pub struct WindowBuffer {
    window_size: usize,
    hop_size: usize,
    sample_rate: usize,
    ring: VecDeque<f32>,
    consumed: u64,
}

impl WindowBuffer {
    pub fn new(window_size: usize, hop_size: usize, sample_rate: usize) -> Self {
        assert!(hop_size > 0 && hop_size <= window_size, "hop_size must be in (0, window_size]");
        WindowBuffer {
            window_size,
            hop_size,
            sample_rate,
            ring: VecDeque::with_capacity(window_size * 2),
            consumed: 0,
        }
    }

    /// Push a block of raw samples. Returns zero or more completed windows:
    /// zero if not enough audio has accumulated yet, more than one if this
    /// block crossed several hop boundaries at once.
    pub fn push(&mut self, samples: &[f32]) -> Vec<Window> {
        self.ring.extend(samples.iter().copied());

        let mut windows = Vec::new();
        while self.ring.len() >= self.window_size {
            let window_samples: Vec<f64> =
                self.ring.iter().take(self.window_size).map(|&s| s as f64).collect();
            let end_sample = self.consumed + self.window_size as u64;
            let timestamp = Duration::from_secs_f64(end_sample as f64 / self.sample_rate as f64);
            windows.push(Window { samples: window_samples, timestamp });

            for _ in 0..self.hop_size {
                self.ring.pop_front();
            }
            self.consumed += self.hop_size as u64;
        }
        windows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic]
    fn rejects_hop_size_larger_than_window() {
        WindowBuffer::new(512, 1024, 44_100);
    }

    #[test]
    fn no_window_until_enough_samples_accumulate() {
        let mut buf = WindowBuffer::new(1024, 256, 44_100);
        assert!(buf.push(&vec![0.0; 128]).is_empty());
        assert!(buf.push(&vec![0.0; 128]).is_empty()); // 256 total, still short
    }

    #[test]
    fn emits_one_window_once_full() {
        let mut buf = WindowBuffer::new(1024, 256, 44_100);
        let windows = buf.push(&vec![0.0; 1024]);
        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].samples.len(), 1024);
    }

    #[test]
    fn non_overlapping_when_hop_equals_window_size() {
        let mut buf = WindowBuffer::new(512, 512, 44_100);
        let windows = buf.push(&vec![0.0; 1536]); // exactly 3 windows, no remainder
        assert_eq!(windows.len(), 3);
    }

    #[test]
    fn partial_trailing_samples_remain_buffered_for_next_push() {
        let mut buf = WindowBuffer::new(1024, 1024, 44_100);
        assert_eq!(buf.push(&vec![0.0; 1500]).len(), 1); // 1 window, 476 leftover
        let windows = buf.push(&vec![0.0; 1024]); // 476 + 1024 = 1500 -> another window, 476 leftover
        assert_eq!(windows.len(), 1);
    }

    #[test]
    fn overlapping_windows_carry_correct_content_and_timestamps() {
        // Mimics 128-sample AudioWorklet blocks trickling into a
        // window=1024, hop=256 buffer. Samples are index-valued so the
        // exact overlap slicing can be checked, not just the count.
        let mut buf = WindowBuffer::new(1024, 256, 44_100);
        let all_samples: Vec<f32> = (0..2048).map(|i| i as f32).collect();

        let mut windows = Vec::new();
        for chunk in all_samples.chunks(128) {
            windows.extend(buf.push(chunk));
        }

        // (2048 - 1024) / 256 + 1 = 5 windows.
        assert_eq!(windows.len(), 5);
        for (i, window) in windows.iter().enumerate() {
            let expected_start = i * 256;
            let expected: Vec<f64> =
                (expected_start..expected_start + 1024).map(|v| v as f64).collect();
            assert_eq!(window.samples, expected, "window {i} content mismatch");

            let expected_end_sample = (expected_start + 1024) as f64;
            let expected_ts = Duration::from_secs_f64(expected_end_sample / 44_100.0);
            assert_eq!(window.timestamp, expected_ts, "window {i} timestamp mismatch");
        }
    }
}
