#![allow(warnings)]
#![allow(dead_code)]
use super::gpio_power::*;
use crate::app::*;
use defmt::info;
use defmt_rtt as _;
use hal::mco::*;
use hal::pwr::{Pwr, StopModeConfig};
use hal::rcc::SysClkSource;
use systick_monotonic::{fugit::Duration, fugit::ExtU64};

/// Power-management state. STOP entry uses the HAL's typed `Pwr` handle
/// (`pwr.stop_mode(StopModeConfig::ultra_low_power(), &mut scb)`) and the
/// wake path uses `rcc.reconfigure_after_stop()` to restore the PLL,
/// matching `examples/rtic_low_power_advanced.rs`.
pub struct Power {
    gpio_power: GpioPower,
    rcc: hal::rcc::Rcc,
    pwr: Pwr,
    scb: cortex_m::peripheral::SCB,
    sleep: bool,
    active_mode: u64,
}

impl Power {
    pub const IDLE_TIMEOUT: u64 = 15_000u64;

    pub fn new(
        gpio_power: GpioPower,
        rcc: hal::rcc::Rcc,
        pwr: Pwr,
        scb: cortex_m::peripheral::SCB,
    ) -> Self {
        Self {
            gpio_power,
            rcc,
            pwr,
            scb,
            sleep: false,
            active_mode: 0_u64,
        }
    }

    pub fn active(&mut self) {
        // Use max(1, now) so that active_mode == 0 always means "no user
        // activity" — see is_active() below.
        let now = monotonics::now().ticks();
        self.active_mode = if now == 0 { 1 } else { now };
        self.sleep = false;

        defmt::trace!("active ");
    }

    pub fn is_active(&mut self) -> bool {
        if self.sleep {
            return false;
        }
        // active_mode == 0 means no button has ever been pressed (boot) or the
        // last sleep cycle reset it. The LCD must stay off in this state — RTC
        // wakes for periodic measurement should NOT re-init the display.
        if self.active_mode == 0 {
            return false;
        }
        if monotonics::now().ticks() - self.active_mode >= Self::IDLE_TIMEOUT {
            return false;
        }
        true
    }

    pub fn is_sleep(&self) -> bool {
        self.sleep
    }

    pub fn enter_sleep(&mut self, f: impl FnOnce()) {
        if !self.is_active() || self.active_mode == 0_u64 {
            self.sleep = true;
            self.active_mode = 0_u64;
            defmt::info!("-- Enter sleep mode --");
            f();
            #[cfg(feature = "low_power")]
            {
                // Drain pending wakeup flag, then enter STOP via the HAL.
                // pwr.stop_mode() sets lpsdsr / ulp / pdds(clear) /
                // SLEEPDEEP — equivalent to the manual PWR_CR modify we
                // had here before. The HAL doesn't expose FWU (fast
                // wakeup) — acceptable for our wake latency budget.
                // gpio_power.down() intentionally skipped: reconfiguring
                // SPI pins to floating lets a pending EXTI line (e.g.
                // TDC7200 INT on EXTI0) preempt mid-sleep-entry and
                // fault on SPI access. STOP itself gates peripheral
                // clocks, which is the main saving.
                self.gpio_power.down();
                self.pwr.clear_wakeup_flag();
                while self.pwr.is_wakeup_flag_set() {}
                self.pwr
                    .stop_mode(StopModeConfig::ultra_low_power(), &mut self.scb);
            }
            // WFI is NOT called here — the caller must do WFI outside the RTIC lock.
            // Previously, calling WFI inside a lock corrupted the task context.
        }
    }

    /// Prepare for sleep (set flags, call callback) without entering WFI.
    /// Callers should call cortex_m::asm::wfi() AFTER releasing the RTIC lock.
    pub fn prepare_sleep(&mut self, f: impl FnOnce()) {
        defmt::info!("prepare_sleep enter");
        if !self.is_active() || self.active_mode == 0_u64 {
            self.sleep = true;
            self.active_mode = 0_u64;
            defmt::info!("-- Enter sleep mode --");
            f();
            defmt::info!("callback done");
            #[cfg(feature = "low_power")]
            {
                // Drain pending wakeup flag, then enter STOP via the HAL.
                // pwr.stop_mode() sets lpsdsr / ulp / pdds(clear) /
                // SLEEPDEEP — equivalent to the manual PWR_CR modify we
                // had here before. The HAL doesn't expose FWU (fast
                // wakeup) — acceptable for our wake latency budget.
                // gpio_power.down() intentionally skipped: reconfiguring
                // SPI pins to floating lets a pending EXTI line (e.g.
                // TDC7200 INT on EXTI0) preempt mid-sleep-entry and
                // fault on SPI access. STOP itself gates peripheral
                // clocks, which is the main saving.
                self.gpio_power.down();
                self.pwr.clear_wakeup_flag();
                while self.pwr.is_wakeup_flag_set() {}
                self.pwr
                    .stop_mode(StopModeConfig::ultra_low_power(), &mut self.scb);
            }
            defmt::info!("prepare_sleep done");
        } else {
            defmt::info!("prepare_sleep: still active, skip");
        }
    }

    pub fn exit_sleep(&mut self) -> bool {
        let ret = self.sleep;
        if self.sleep {
            self.sleep = false;
            #[cfg(feature = "low_power")]
            {
                info!(
                    "Clock after STOP (before reconfig): {}",
                    defmt::Debug2Format(&self.rcc.get_sysclk_source()),
                );
                // Restore the configured PLL clock — STOP falls back to MSI
                // on wake. rcc.reconfigure_after_stop() handles this (calls
                // self.update() internally). MCO output and ADC HSI need a
                // separate touch because the HAL doesn't restore them.
                self.gpio_power.up();
                self.rcc.reconfigure_after_stop();
                self.rcc.update_mco(MCOSel::Hse, MCODiv::Div1);
                // ADC uses HSI; re-enable and wait for ready.
                self.rcc.cr.write(|w| w.hsion().set_bit());
                while self.rcc.cr.read().hsirdy().bit_is_clear() {}
                // SLEEPDEEP persists across wake; clear it so any later WFI
                // (e.g. in idle, before the next prepare_sleep) does normal
                // Sleep, not STOP.
                self.scb.clear_sleepdeep();
                // gpio_power.up() skipped — see prepare_sleep.
                info!(
                    "--- Wakeup | Clock: {} ({} MHz) ---",
                    defmt::Debug2Format(&self.rcc.get_sysclk_source()),
                    self.rcc.clocks.sys_clk().0 / 1_000_000,
                );
            }
        }
        ret
    }
}
