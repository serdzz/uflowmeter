//! NVIC helpers. Wraps the unsafe `NVIC::unmask` so callers can stay
//! inside `#![deny(unsafe_code)]` scopes (notably `main.rs`).

use cortex_m::peripheral::NVIC;
use hal::stm32::Interrupt;

/// Re-enable the EXTI0 interrupt (TDC7200 INT line on PB0).
///
/// Call sites:
/// 1. STOP-mode exit in `app_request` — pairs with the matching `NVIC::mask`
///    before sleep entry; the TDC7200 cannot fire while clocks are gated.
/// 2. `tdc7200_result` bottom half — after `clear_interrupt_status` has
///    de-asserted the TDC INT pin and the SharedBus is no longer locked.
///
/// In both cases the call site is outside any RTIC `lock()` and after
/// peripheral state has settled, so re-arming the line cannot race a
/// half-completed transaction.
pub fn unmask_exti0() {
    // SAFETY: NVIC::unmask is `unsafe` because enabling an interrupt at the
    // wrong moment can break a critical-section invariant. See the
    // call-site notes above — both sites are race-free by construction.
    #[allow(unsafe_code)]
    unsafe {
        NVIC::unmask(Interrupt::EXTI0);
    }
}
