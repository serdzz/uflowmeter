//! Software tick counter. Replaces `systick-monotonic` which corrupts
//! the RTIC timer queue across STM32L1 STOP cycles (CLAUDE.md bug #3).
//!
//! `tick()` is called from the TIM2 ISR at 20 Hz, advancing `NOW_MS` by
//! `TICK_MS = 50`. `now_ms()` returns milliseconds since boot, wrapping
//! at u32::MAX ≈ 49.7 days. All consumers use `now.wrapping_sub(prev)`
//! style comparisons that survive the wrap.
//!
//! STOP halts TIM2 so time does not advance while sleeping — same
//! behavior as the previous SysTick-based monotonic, kept intentionally:
//! all readers live in awake-only code paths (button repeat, modbus
//! idle, idle-timeout check).
//!
//! Cortex-M3 has LDREX/STREX so `AtomicU32` works natively without a
//! critical section — picked over a `Mutex<Cell<u64>>` because the
//! defmt::timestamp! formatter calls `now_ms()` from inside arbitrary
//! ISRs (including during a critical-section exit), and re-enabling
//! interrupts there can fault on STM32L1.

use core::sync::atomic::{AtomicU32, Ordering};

/// Milliseconds advanced per TIM2 tick. TIM2 fires at 20 Hz.
pub const TICK_MS: u32 = 50;

static NOW_MS: AtomicU32 = AtomicU32::new(0);

/// Advance the tick counter by one TIM2 period. Call once per TIM2 ISR.
#[inline]
pub fn tick() {
    NOW_MS.fetch_add(TICK_MS, Ordering::Relaxed);
}

/// Current monotonic time in milliseconds since boot. Wraps at ~49.7 d;
/// callers must use wrapping arithmetic for elapsed-time checks. Does
/// not advance during STOP mode.
#[inline]
pub fn now_ms() -> u32 {
    NOW_MS.load(Ordering::Relaxed)
}
