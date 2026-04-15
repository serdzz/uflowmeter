#![allow(warnings)]
#![allow(dead_code)]
use super::gpio_power::*;
use crate::app::*;
use defmt::info;
use defmt_rtt as _;
use hal::mco::*;
use hal::rcc::SysClkSource;
use systick_monotonic::{fugit::Duration, fugit::ExtU64};

pub struct Power {
    gpio_power: GpioPower,
    rcc: hal::rcc::Rcc,
    pwr: hal::stm32::PWR,
    scb: cortex_m::peripheral::SCB,
    sleep: bool,
    active_mode: u64,
}

impl Power {
    pub const IDLE_TIMEOUT: u64 = 15_000u64;

    pub fn new(
        gpio_power: GpioPower,
        rcc: hal::rcc::Rcc,
        pwr: hal::stm32::PWR,
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
        self.active_mode = monotonics::now().ticks();
        self.sleep = false;

        defmt::trace!("active ");
    }

    pub fn is_active(&mut self) -> bool {
        if self.sleep {
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
                self.pwr.cr.modify(|_, w| {
                    w.fwu()
                        .set_bit()
                        .ulp()
                        .set_bit()
                        .pvde()
                        .clear_bit()
                        .pdds()
                        .clear_bit()
                        .lpsdsr()
                        .set_bit()
                        .cwuf()
                        .set_bit()
                });
                while self.pwr.csr.read().wuf().bit_is_set() {}
                self.gpio_power.down();
                self.scb.set_sleepdeep();
            }
            // Use cortex_m WFI directly instead of rtic::export::wfi()
            // which is an internal API and may break on version updates
            cortex_m::asm::wfi();
            // WFI is handled by RTIC idle (outside any lock)
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
                self.scb.clear_sleepdeep();
                self.gpio_power.up();
                self.rcc.update();
                self.rcc.update_mco(MCOSel::Hse, MCODiv::Div1);
                // ADC HSI Enable
                self.rcc.cr.write(|w| w.hsion().set_bit());
                while self.rcc.cr.read().hsirdy().bit_is_clear() {}
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
