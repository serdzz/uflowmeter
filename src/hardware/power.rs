#![allow(warnings)]
#![allow(dead_code)]
use super::gpio_power::*;
use crate::app::*;
use defmt_rtt as _;
use hal::mco::*;
use systick_monotonic::{fugit::Duration, fugit::ExtU64};

pub struct Power {
    gpio_power: GpioPower,
    rcc: hal::rcc::Rcc,
    pwr: hal::stm32::PWR,
    scb: hal::stm32::SCB,
    sleep: bool,
    active_mode: Duration<u64, 1, 1000>,
}

impl Power {
    pub const IDLE_TIMEOUT: u64 = 15_u64;

    pub fn new(
        gpio_power: GpioPower,
        rcc: hal::rcc::Rcc,
        pwr: hal::stm32::PWR,
        scb: hal::stm32::SCB,
    ) -> Self {
        Self {
            gpio_power,
            rcc,
            pwr,
            scb,
            sleep: false,
            active_mode: monotonics::now().duration_since_epoch(),
        }
    }

    pub fn active(&mut self) {
        self.active_mode = monotonics::now().duration_since_epoch();
    }

    pub fn is_active(&mut self) -> bool {
        if monotonics::now().duration_since_epoch() - self.active_mode
            >= Self::IDLE_TIMEOUT.secs::<1, 1000>()
            && !self.sleep
        {
            return false;
        }
        true
    }

    pub fn enter_sleep(&mut self, f: impl FnOnce()) {
        if !self.is_active() || self.active_mode == 0_u64.secs::<1, 1000>() {
            self.sleep = true;
            defmt::info!("-- Enter sleep mode --");
            defmt::info!("-- Enter sleep mode --");
            #[cfg(not(feature = "swd"))]
            {
                f();
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
                self.scb.set_sleepdeep();
                self.gpio_power.down();
            }
            rtic::export::wfi();
        }
    }

    pub fn exit_sleep(&mut self) -> bool {
        let ret = self.sleep;
        if self.sleep {
            self.sleep = false;
            defmt::info!("-- Exit sleep mode --");
            #[cfg(not(feature = "swd"))]
            {
                self.scb.clear_sleepdeep();
                self.gpio_power.up();
                self.rcc.update();
                self.rcc.update_mco(MCOSel::Hse, MCODiv::Div1);
                // ADC HSI Enable
                self.rcc.cr.write(|w| w.hsion().set_bit());
                while self.rcc.cr.read().hsirdy().bit_is_clear() {}
            }
        }
        ret
    }
}
