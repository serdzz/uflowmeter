#![deny(warnings)]
#![no_main]
#![no_std]

extern crate cortex_m;
extern crate cortex_m_rt as rt;
extern crate rtic;
extern crate stm32l1xx_hal as hal;

use cortex_m::peripheral::SCB;
use defmt_rtt as _;
use hal::prelude::*;
use hal::pwr::{Pwr, StopModeConfig};
use hal::rcc::{Config, PLLDiv, PLLMul, PLLSource, Rcc, SysClkSource};
use hal::rtc::{Event, Rtc};
use panic_probe as _;
use rtic::app;

// Magic number to detect if RTC was initialized
const MAGIC_NUMBER: u32 = 0x32F2;
// Wakeup counter register index
const WAKEUP_COUNTER_REG: usize = 1;
// RTC wakeup interval in seconds
const WAKEUP_INTERVAL: u32 = 5;

#[app(device = hal::stm32, peripherals = true)]
mod app {
    use super::*;

    #[shared]
    struct Shared {
        // Lock-free: only accessed from rtc_wkup task
        #[lock_free]
        wakeup_count: u32,
    }

    #[local]
    struct Local {
        // Resources for the rtc_wkup interrupt handler
        rtc: Rtc,
        rcc: Rcc,
        // Resources for the idle task
        pwr: Pwr,
        scb: SCB,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        let dp = cx.device;

        // Keep the debug domain (SWD/RTT) alive during Sleep and Stop modes.
        // DBGMCU_CR resets to 0 after every chip reset, so this must be set
        // explicitly — otherwise the MCU powers off the SWD interface the
        // moment it enters STOP, making probe-rs/RTT go dark.
        // dp.DBGMCU
        //     .cr
        //     .modify(|_, w| w.dbg_sleep().set_bit().dbg_stop().set_bit());
        dp.DBGMCU.cr.modify(|_, w| {
            w.dbg_sleep()
                .set_bit()
                .dbg_stop()
                .set_bit()
                .dbg_standby()
                .set_bit()
        });
        let mut pwr_raw = dp.PWR;
        let mut exti = dp.EXTI;

        // Configure system clock: HSE 24MHz -> PLL x4 / 4 = 24MHz
        let rcc_config = Config::pll(PLLSource::HSE(24.mhz()), PLLMul::Mul4, PLLDiv::Div4);
        let rcc = dp.RCC.freeze(rcc_config);

        defmt::info!("=== STM32L1 RTIC Advanced Low Power Example ===");
        defmt::info!(
            "Clock: {:?} ({} MHz)",
            defmt::Debug2Format(&rcc.get_sysclk_source()),
            rcc.clocks.sys_clk().0 / 1_000_000
        );

        // Initialize RTC with LSE
        let mut rtc = Rtc::new(dp.RTC, &mut pwr_raw);

        // Check if this is first boot or a resume from backup-domain retention
        let wakeup_count = if rtc.is_initialized(0, MAGIC_NUMBER) {
            let count = rtc.read_backup_register(WAKEUP_COUNTER_REG);
            defmt::info!("Resumed from backup. Previous wakeup count: {}", count);
            count
        } else {
            defmt::info!("First boot - initializing RTC...");
            use time::{Date, Month, PrimitiveDateTime, Time};
            let datetime = PrimitiveDateTime::new(
                Date::from_calendar_date(2025, Month::November, 26).unwrap(),
                Time::from_hms(21, 0, 0).unwrap(),
            );
            rtc.set_datetime(&datetime).unwrap();
            rtc.mark_initialized(0, MAGIC_NUMBER);
            rtc.write_backup_register(WAKEUP_COUNTER_REG, 0);
            defmt::info!("RTC initialized to: 2025-11-26 21:00:00");
            0
        };

        // Configure RTC wakeup timer and EXTI line 20
        // listen() sets both IMR[20] (for RTIC interrupt dispatch) and
        // EMR[20] (so WFE in idle can exit STOP mode without a dedicated ISR).
        rtc.enable_wakeup(WAKEUP_INTERVAL);
        rtc.listen(&mut exti, Event::Wakeup);
        // Clear any stale pending flags before entering STOP the first time
        rtc.unpend(Event::Wakeup);

        defmt::info!(
            "RTC wakeup configured for {} seconds. Entering STOP mode loop...",
            WAKEUP_INTERVAL
        );

        let pwr = pwr_raw.constrain();
        let scb = cx.core.SCB;

        (
            Shared { wakeup_count },
            Local { rtc, rcc, pwr, scb },
            init::Monotonics(),
        )
    }

    /// RTC wakeup interrupt handler.
    ///
    /// RTIC automatically enables NVIC for RTC_WKUP when this task is bound.
    /// Flow: WFE exits (EMR event) → NVIC interrupt pending → RTIC dispatches here.
    #[task(binds = RTC_WKUP, local = [rtc, rcc], shared = [wakeup_count])]
    fn rtc_wkup(cx: rtc_wkup::Context) {
        let rtc = cx.local.rtc;
        let rcc = cx.local.rcc;
        let wakeup_count = cx.shared.wakeup_count;

        defmt::info!(
            "---  Clock after STOP (before reconfig): {:?}  ---",
            defmt::Debug2Format(&rcc.get_sysclk_source()),
        );
        // Reconfigure clocks first: STOP mode falls back to MSI automatically.
        rcc.reconfigure_after_stop();

        // Clear RTC WUTF, EXTI PR[20], and PWR WUF flags.
        rtc.unpend(Event::Wakeup);

        defmt::info!(
            "--- Wakeup #{} | Clock: {:?} ({} MHz) ---",
            wakeup_count,
            defmt::Debug2Format(&rcc.get_sysclk_source()),
            rcc.clocks.sys_clk().0 / 1_000_000,
        );

        if rcc.get_sysclk_source() != SysClkSource::PLL {
            defmt::warn!("WARNING: Clock reconfiguration failed!");
        }

        *wakeup_count += 1;
        // Persist the count across power cycles via RTC backup register
        rtc.write_backup_register(WAKEUP_COUNTER_REG, *wakeup_count);
    }

    /// Idle task: configures STOP mode and suspends via WFI.
    ///
    /// WFI exits when the RTC wakeup interrupt fires via EXTI line 20 (IMR[20] set).
    /// RTIC preempts idle to run rtc_wkup and returns here afterwards.
    #[idle(local = [pwr, scb])]
    fn idle(cx: idle::Context) -> ! {
        loop {
            let stop_config = StopModeConfig::ultra_low_power();
            cx.local.pwr.stop_mode(stop_config, cx.local.scb);

            // ARMv7-M Architecture Reference Manual §B1.5.4:
            // DSB ensures all explicit data transfers (including any pending SWD
            // writes from probe-rs, e.g. updating RTT RdOff) are visible to the
            // bus fabric before the core enters STOP mode.  Without this there is
            // a race window where the SWD write commits after the AHB clock is
            // gated, causing probe-rs to see a stale RdOff on the next poll and
            // emit "RTT read pointer changed, re-attaching".
            cortex_m::asm::dsb();
            cortex_m::asm::wfi();

            // SLEEPDEEP persists across the wakeup.  Clear it here so that any
            // WFI executed before the next loop iteration (e.g. inside library
            // code) enters normal Sleep, not STOP mode.
            cx.local.scb.clear_sleepdeep();
        }
    }
}
