#![allow(warnings)]
#![allow(dead_code)]
use super::gpio_power::*;
use defmt::info;
use crate::app::*;
use defmt_rtt as _;
use hal::mco::*;
use hal::pwr::StopModeConfig;
use hal::rcc::SysClkSource;
use systick_monotonic::{fugit::Duration, fugit::ExtU64};

pub struct Power {
    gpio_power: GpioPower,
    rcc: hal::rcc::Rcc,
    pwr: hal::pwr::Pwr,
    scb: cortex_m::peripheral::SCB,
    sleep: bool,
    active_mode: u64,
}

impl Power {
    pub const IDLE_TIMEOUT: u64 = 15_000u64;

    pub fn new(
        gpio_power: GpioPower,
        rcc: hal::rcc::Rcc,
        pwr: hal::pwr::Pwr,
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
                let stop_config = StopModeConfig::ultra_low_power();
                self.pwr.stop_mode(stop_config, &mut self.scb);
                self.gpio_power.down();
            }
            // WFI is handled by RTIC idle (outside any lock)
        }
    }

    pub fn exit_sleep(&mut self) -> bool {
        let ret = self.sleep;
        if self.sleep {
            self.sleep = false;
            defmt::info!("-- Exit sleep mode --");
            #[cfg(feature = "low_power")]
            {
                info!(
                    "Clock after STOP (before reconfig): {}",
                    match self.rcc.get_sysclk_source() {
                        SysClkSource::HSI => "HSI",
                        SysClkSource::HSE => "HSE",
                        SysClkSource::PLL => "PLL",
                        SysClkSource::MSI => "MSI",
                    }
                );
                self.scb.clear_sleepdeep();
                self.rcc.reconfigure_after_stop();
                self.gpio_power.up();
                self.rcc.update_mco(MCOSel::Hse, MCODiv::Div1);
                info!(
                    "--- Wakeup | Clock: {} ({} MHz) ---",
                    match self.rcc.get_sysclk_source() {
                        SysClkSource::HSI => "HSI",
                        SysClkSource::HSE => "HSE",
                        SysClkSource::PLL => "PLL",
                        SysClkSource::MSI => "MSI",
                    },
                    self.rcc.clocks.sys_clk().0 / 1_000_000,
                );
            }
        }
        ret
    }
}
