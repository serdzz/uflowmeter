#![deny(unsafe_code)]
#![deny(warnings)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![no_main]
#![no_std]

extern crate alloc;
extern crate stm32l1xx_hal as hal;

mod apps;
mod gui;
mod hardware;
mod history;
mod options;
mod ui;

use apps::*;
use core::fmt::Write;
use defmt_rtt as _;
use embedded_hal::digital::v2::OutputPin;
use embedded_storage::ReadStorage;
use gui::*;
use hal::exti::ExtiExt;
use hal::gpio::AltMode;
use hal::mco::*;
use hal::prelude::*;
use hal::rcc::Config;
use hal::rtc::{Event, Rtc};
//se hal::rtc::RestoredOrNewRtc::{New, Restored};
use hal::serial;
use hal::serial::SerialExt;
use hal::spi;
use hal::timer::Timer;
use hardware::*;
use history::*;
use microchip_eeprom_25lcxx::*;
use nb::block;
use options::*;
use panic_probe as _;
use rtic::app;
use shared_bus_rtic::SharedBus;
use systick_monotonic::{fugit::ExtU64, Systick};
use time::Duration;
use time::{
    macros::{date, time},
    PrimitiveDateTime,
};
use ui::*;
//use crate::alloc::string::ToString;

type BusType = spi::Spi<hal::stm32::SPI2, (SpiSck, SpiMiso, SpiMosi)>;
type MyStorage = Storage<SharedBus<BusType>, MemoryEn, MemoryWp, MemoryHold>;
type HourHistory = RingStorage<0, 2160, 3600>;
type DayHistory = RingStorage<{ HourHistory::SIZE_ON_FLASH }, { 31 * 12 * 3 }, { 3600 * 24 }>;
type MonthHistory = RingStorage<
    { HourHistory::SIZE_ON_FLASH + DayHistory::SIZE_ON_FLASH },
    { 10 * 12 },
    { 3600 * 24 * 31 },
>;
#[global_allocator]
static ALLOCATOR: emballoc::Allocator<4096> = emballoc::Allocator::new();

#[app(device = hal::stm32, peripherals = true, dispatchers = [AES,COMP_ACQ])]
mod app {
    use super::*;
    use hal::exti::TriggerEdge;

    defmt::timestamp!("{=u64:tms}", { monotonics::now().ticks() });

    #[shared]
    struct Shared {
        power: hardware::Power,
        rtc: Rtc<hal::rtc::Lse>,
        lcd: Lcd,
        //storage: Storage<SharedBus<BusType>, MemoryEn, MemoryWp, MemoryHold>,
        app: App,
        ui: Viewport,
    }

    #[local]
    struct Local {
        keyboard: Keyboard,
        timer: Timer<hal::stm32::TIM2>,
        ui_timer: Timer<hal::stm32::TIM3>,
        handle: Option<__rtic_internal_app_request_MonoTimer_SpawnHandle>,
        adc: hal::adc::Adc,
        photo_r: PhotoR,
    }

    #[monotonic(binds = SysTick, default = true)]
    type MonoTimer = Systick<1000>;

    #[init]
    fn init(cx: init::Context) -> (Shared, Local, init::Monotonics) {
        defmt::info!("init");
        let mut p = cx.device;
        p.DBGMCU.cr.modify(|_, w| {
            w.dbg_sleep()
                .set_bit()
                .dbg_stop()
                .set_bit()
                .dbg_standby()
                .set_bit()
        });

        let mut rcc = p.RCC.freeze(Config::pll(
            hal::rcc::PLLSource::HSE(8.mhz()),
            hal::rcc::PLLMul::Mul6,
            hal::rcc::PLLDiv::Div3,
        ));
        defmt::info!("rcc freeze");
        rcc.enable_power();
        //let mut pwr = p.PWR;
        //let mut backup_domain = rcc.bkp.constrain(p.BKP, &mut pwr);

        defmt::info!("enable_power");
        let mut rtc = Rtc::new(p.RTC, &mut p.PWR);

        defmt::info!("rtc");
        let mono = Systick::new(cx.core.SYST, 16_000_000);

        defmt::info!("mono");

        let hardware::Pins {
            lcd_rs,
            lcd_rw,
            lcd_e,
            lcd_d4,
            lcd_d5,
            lcd_d6,
            lcd_d7,
            lcd_on,
            lcd_led,
            button_set,
            button_enter,
            button_down,
            button_up,
            spi_sck,
            spi_miso,
            spi_mosi,
            mco,
            osc_en,
            tx,
            rx,
            mut rs_power_en,
            memory_en,
            memory_hold,
            memory_wp,
            tdc1000_en,
            tdc1000_cs,
            tdc1000_res,
            tdc7200_en,
            tdc7200_cs,
            tdc7200_int,
            sw_en,
            sw_a0,
            sw_a1,
            photo_r,
            ext_in,
            ext_out,
            gpio_power,
        } = hardware::Pins::new(p.GPIOA, p.GPIOB, p.GPIOC, p.GPIOD, p.GPIOH);

        let _ = osc_en;
        //let _ = tdc1000_en;
        //let _ = tdc1000_cs;
        //let _ = tdc1000_res;
        let _ = tdc7200_en;
        let _ = tdc7200_cs;
        let _ = tdc7200_int;
        let _ = sw_en;
        let _ = sw_a0;
        let _ = sw_a1;
        let _ = ext_in;
        let _ = ext_out;

        rcc.configure_mco(MCOSel::Hse, MCODiv::Div1, mco);

        let hd44780: LcdHardware =
            LcdHardware::new(lcd_rs, lcd_e, lcd_d4, lcd_d5, lcd_d6, lcd_d7, lcd_rw);
        let mut lcd = Lcd::new(hd44780, lcd_on, lcd_led);
        lcd.init();

        let mut adc = p.ADC.adc(&mut rcc);
        adc.set_precision(hal::adc::Precision::B_12);

        let keyboard = Keyboard::new(button_set, button_enter, button_down, button_up);

        spi_sck.set_alt_mode(AltMode::SPI1_2);
        spi_miso.set_alt_mode(AltMode::SPI1_2);
        spi_mosi.set_alt_mode(AltMode::SPI1_2);

        let spi = p.SPI2.spi(
            (spi_sck, spi_miso, spi_mosi),
            spi::MODE_0,
            16.mhz(),
            &mut rcc,
        );
        let bus = shared_bus_rtic::new!(spi, BusType);
        defmt::info!("e25x");
        let eeprom25x = Eeprom25x::new(bus.acquire(), memory_en, memory_wp, memory_hold).unwrap();

        let mut storage = microchip_eeprom_25lcxx::Storage::new(eeprom25x);

        let mut opt = Options::load(&mut storage).unwrap();
        let reg = [
            0x31_u8, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0, 0, 0, 0, 0, 0,
        ];

        opt.set_tdc7200_regs(u128::from_le_bytes(reg));
        // opt.set_k11(k11 as u3u128::from_be_bytes(reg));2);
        // defmt::info!("k11: {:x}", k11);
        defmt::info!("opt: {:x}", opt.into_bytes());
        //opt.save(&mut storage);

        let mut tdc1000 = TDC1000::new(bus.acquire(), tdc1000_cs, tdc1000_res, tdc1000_en);
        // let mut data = [0u8; 16];
        let mut cfg = Config0::default();
        cfg.set_num_tx(31);
        cfg.set_tx_freq_div(FrequencyDividerForTx::Div16);
        let bytes = cfg.into_bytes();
        tdc1000.set_config0(cfg).ok();
        defmt::info!("tdc1000_regs: {:x}", bytes);
        // storage.read(254, &mut data).unwrap();
        // defmt::info!("read before: {:x}", data);

        // let mut data = [11u8, 12u8, 13u8, 14u8];
        // storage.write(254, &mut data).unwrap();

        // let mut data = [0u8; 256];
        // storage.read(0, &mut data).unwrap();
        // defmt::info!("read after: {:x}", data);
        let mut asd = HourHistory::new(&mut storage).unwrap();
        defmt::info!(
            "read data.size: {:?} {:?} {:?}",
            asd.data.size(),
            asd.first_stored_timestamp(),
            asd.last_stored_timestamp()
        );

        rs_power_en.set_low().unwrap();

        let mut serial = p
            .USART1
            .usart(
                (tx, rx),
                serial::Config::default().baudrate(hal::time::Bps(112500)),
                &mut rcc,
            )
            .unwrap();

        writeln!(serial, "Hello world\r").unwrap();
        block!(serial.flush()).ok();

        defmt::info!("{}", compile_time::datetime_str!());
        defmt::info!("{}", compile_time::rustc_version_str!());
        let datetime = compile_time::datetime!().saturating_add(Duration::HOUR * 2);
        let asd = datetime.unix_timestamp();
        defmt::info!("unix_timestamp {}", asd);
        rtc.set_datetime(&PrimitiveDateTime::new(datetime.date(), datetime.time()))
            .unwrap();
        defmt::info!("rtc init");

        rtc.enable_wakeup(5);
        rtc.listen(&mut p.EXTI, Event::Wakeup);

        let mut exti = ExtiExt::new(p.EXTI);

        rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit());

        exti.listen_gpio(&mut p.SYSCFG, 1, 6, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 7, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 8, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 9, TriggerEdge::Falling);

        let mut timer = p.TIM2.timer(20.hz(), &mut rcc);
        timer.listen();
        let mut ui_timer = p.TIM3.timer(10.hz(), &mut rcc);
        ui_timer.listen();

        // let mut a =
        //     alloc::collections::BTreeMap::<alloc::string::String, alloc::string::String>::new();
        // a.insert("key".to_string(), "ыаывафыва".to_string());
        // let val = a.get("key");
        // if let Some(s) = val {
        //     defmt::info!("{}", s.as_str());
        // }

        let power = Power::new(gpio_power, rcc, p.PWR, cx.core.SCB);
        app_request::spawn(AppRequest::DeepSleep).ok();

        defmt::info!("init end");
        (
            Shared {
                power,
                rtc,
                lcd,
                //storage,
                app: App::default(),
                ui: Viewport::default(),
            },
            Local {
                keyboard,
                timer,
                ui_timer,
                handle: None,
                adc,
                photo_r,
            },
            init::Monotonics(mono),
        )
    }

    #[task(binds = RTC_WKUP, priority = 2, shared = [power,rtc])]
    fn rtc_timer(ctx: rtc_timer::Context) {
        defmt::info!("rtc_timer");
        let rtc_timer::SharedResources { power, rtc } = ctx.shared;
        (power, rtc).lock(|power, rtc| {
            rtc.unpend(Event::Wakeup);
            power.exit_sleep();
        });
        app_request::spawn(AppRequest::Process).ok();
    }

    #[task(binds = EXTI9_5, priority = 2, shared = [power])]
    fn exti9_5(ctx: exti9_5::Context) {
        let mut power = ctx.shared.power;
        ExtiExt::unpend(6);
        ExtiExt::unpend(7);
        ExtiExt::unpend(8);
        ExtiExt::unpend(9);
        power.lock(|power| {
            power.active();
            power.exit_sleep();
        });
    }

    #[task(binds = TIM2, priority = 2, local = [handle,keyboard,timer], shared = [power,lcd,app,ui] )]
    fn timer(ctx: timer::Context) {
        let timer::SharedResources {
            mut power,
            lcd,
            app,
            ui,
        } = ctx.shared;
        ctx.local.timer.clear_irq();
        let is_active = power.lock(|power| power.is_active());
        if is_active {
            let event = ctx.local.keyboard.read_ui_keys();
            (app, lcd, ui).lock(|app, lcd, ui| {
                if let Some(event) = event {
                    app.handle_event(ui.event(event)).map(app_request::spawn);
                }
                if lcd.init() {
                    ui.invalidate();
                }
                ui.update(app);
                ui.render(lcd);
            });
            if event.is_some() {
                if let Some(h) = ctx.local.handle.take() {
                    h.cancel().ok();
                }
                *ctx.local.handle =
                    app_request::spawn_after(Power::IDLE_TIMEOUT.secs(), AppRequest::DeepSleep)
                        .ok();
            }
        }
    }

    #[task(binds = TIM3, priority = 2, local = [ui_timer,adc,photo_r,led: bool = false], shared = [power,lcd,app,ui,rtc] )]
    fn ui_timer(ctx: ui_timer::Context) {
        let ui_timer::SharedResources {
            mut power,
            lcd,
            app,
            ui,
            rtc,
        } = ctx.shared;
        ctx.local.ui_timer.clear_irq();
        let is_active = power.lock(|power| power.is_active());
        if is_active {
            (app, lcd, ui, rtc).lock(|app, lcd, ui, rtc| {
                app.datetime = rtc.get_datetime();
                if lcd.init() {
                    ui.invalidate();
                }
                ui.update(app);
                ui.render(lcd);
            });
            let chan_val: u16 = ctx.local.adc.read(ctx.local.photo_r).unwrap();
            if chan_val < 500 && *ctx.local.led {
                *ctx.local.led = false;
                app_request::spawn(AppRequest::LcdLed(*ctx.local.led)).ok();
            } else if chan_val > 500 && !*ctx.local.led {
                *ctx.local.led = true;
                app_request::spawn(AppRequest::LcdLed(*ctx.local.led)).ok();
            }
        }
    }

    #[task(capacity = 8, priority = 1, shared = [power, lcd, rtc])]
    fn app_request(ctx: app_request::Context, req: AppRequest) {
        let app_request::SharedResources {
            power,
            mut lcd,
            mut rtc,
        } = ctx.shared;
        match req {
            AppRequest::Process => {
                defmt::info!("Process");
                app_request::spawn_after(25_u64.millis(), AppRequest::DeepSleep).ok();
            }
            AppRequest::LcdLed(on) => {
                defmt::info!("LcdLed {}", on);
                lcd.lock(|lcd| lcd.led(on));
            }
            AppRequest::SetDateTime(dt) => {
                rtc.lock(|rtc| rtc.set_datetime(&dt).ok());
            }
            AppRequest::DeepSleep => {
                defmt::info!("DeepSleep");
                (power, lcd).lock(|power, lcd| {
                    power.enter_sleep(|| {
                        #[cfg(not(feature = "swd"))]
                        lcd.led_off();
                        lcd.off();
                    });
                });
            }
        }
    }
}

// fn crc(data: &mut [u8], seed: u16) -> u16 {
//     let mut crc = seed;
//     for i in data {
//         let c = *i as u16;
//         let mut x = (crc >> 8) ^ c;
//         x ^= x >> 4;
//         crc = (crc << 8) ^ (x << 12) ^ (x << 5) ^ (x);
//     }
//     crc
// }
