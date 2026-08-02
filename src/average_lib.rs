//! Fixed-size moving average over the most recent flow readings.
//!
//! Port of the C++ `AverageBuffer<real_t, 10>`
//! (`UFlowMeter_c++/UFlowMeter/Inc/measure.hpp:8`), which the meter
//! keeps between measurement cycles so the displayed consumption is a
//! window mean rather than whichever single cycle just finished.
//!
//! One deliberate difference: the C++ `push_back` seeds `count_` and
//! `buff_[0]` from RTC backup registers DR6/DR7 on the first push
//! after a reset, reconstructing an approximate previous average from
//! a stored count and total. That is not replicated here — it makes
//! the first window after every reset a fabricated value, and with the
//! watchdog now running, resets are no longer rare enough for that to
//! be harmless. Ask if the old behaviour is wanted.

/// Readings kept in the window. Matches the C++ template argument.
pub const WINDOW: usize = 10;

#[derive(Clone, Copy)]
pub struct AverageBuffer {
    buf: [f32; WINDOW],
    /// Populated slots, saturating at `WINDOW`.
    count: usize,
    /// Next slot to overwrite.
    next: usize,
}

impl Default for AverageBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl AverageBuffer {
    pub const fn new() -> Self {
        Self {
            buf: [0.0; WINDOW],
            count: 0,
            next: 0,
        }
    }

    /// Push one reading, evicting the oldest once the window is full.
    pub fn push(&mut self, value: f32) {
        self.buf[self.next] = value;
        self.next = (self.next + 1) % WINDOW;
        if self.count < WINDOW {
            self.count += 1;
        }
    }

    /// Push the mean of two readings as a single sample — the C++
    /// two-argument `push_back`, used when both transducer pairs
    /// reported a usable signal.
    pub fn push_pair(&mut self, a: f32, b: f32) {
        self.push((a + b) / 2.0);
    }

    /// Sum of the populated slots.
    pub fn total(&self) -> f32 {
        self.buf[..self.count].iter().sum()
    }

    /// Mean of the populated slots, or `None` while empty — the C++
    /// divides by `count_` unguarded and returns NaN on an empty
    /// buffer.
    pub fn average(&self) -> Option<f32> {
        if self.count == 0 {
            return None;
        }
        Some(self.total() / self.count as f32)
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }
}
