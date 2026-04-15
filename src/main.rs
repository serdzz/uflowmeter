#![deny(unsafe_code)]
#![deny(warnings)]
#![allow(dead_code)]
#![allow(unused_imports)]
#![no_main]
#![no_std]

extern crate alloc;
extern crate stm32l1xx_hal as hal;

mod apps;
mod calibration;
mod gui;
mod hardware;
mod history;
mod mbus;
mod modbus;
mod modbus_handler;
mod options;
mod shell;
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
use hal::pwr::PwrExt;
use hal::rcc::Config;
use hal::rtc::{Event, Rtc};
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
use rand_core::{RngCore, SeedableRng};
use rand_pcg::Pcg32;
use rtic::app;
use shared_bus_rtic::SharedBus;
use systick_monotonic::{fugit::ExtU64, Systick};
use time::Duration;
use time::{
    macros::{date, time},
    PrimitiveDateTime,
};
use ui::*;

impl CharacterDisplay for hardware::Lcd {
    fn set_position(&mut self, col: u8, row: u8) {
        self.set_position(col, row);
    }

    fn reset_custom_chars(&mut self) {
        self.reset_custom_chars();
    }

    fn clear(&mut self) {
        self.clear();
    }
}

type BusType = spi::Spi<hal::stm32::SPI2, (SpiSck, SpiMiso, SpiMosi)>;
type MyStorage = Storage<SharedBus<BusType>, MemoryEn, MemoryWp, MemoryHold>;
type Tdc1000Dev = TDC1000<SharedBus<BusType>, Tdc1000Cs, Tdc1000Res, Tdc1000En>;
type Tdc7200Dev = Tdc7200<SharedBus<BusType>, Tdc7200Cs>;
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
        hour_history: HourHistory,
        day_history: DayHistory,
        month_history: MonthHistory,
        storage: MyStorage,
        app: App,
        ui: MenuController,
        modbus_handler: modbus_handler::ModbusHandler,
        serial: hal::serial::Serial<hal::stm32::USART1>,
        modbus_rx_buf: heapless::Vec<u8, 256>,
        modbus_last_rx: u64,
        shell_line_buf: heapless::Vec<u8, 80>,
        options: Options,
        tdc1000: Tdc1000Dev,
        tdc7200: Tdc7200Dev,
        comm_mode: options::CommType,
    }

    #[local]
    struct Local {
        keyboard: Keyboard,
        timer: Timer<hal::stm32::TIM2>,
        ui_timer: Timer<hal::stm32::TIM3>,
        handle: Option<__rtic_internal_app_request_MonoTimer_SpawnHandle>,
        adc: hal::adc::Adc,
        photo_r: PhotoR,
        iwdg: hal::watchdog::IndependedWatchdog,
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
            hal::rcc::PLLSource::HSE(24.mhz()),
            hal::rcc::PLLMul::Mul4,
            hal::rcc::PLLDiv::Div4,
        ));
        defmt::info!("rcc freeze");
        rcc.enable_power();
        defmt::info!("enable_power");
        let mut rtc = Rtc::new(p.RTC, &mut p.PWR);

        defmt::info!("rtc");
        let mono = Systick::new(cx.core.SYST, 24_000_000);

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
        let mut tdc7200_en = tdc7200_en;
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
        let eeprom25x = Eeprom25x::new(bus.acquire(), memory_en, memory_wp, memory_hold)
            .unwrap_or_else(|_| {
                defmt::error!("EEPROM init failed");
                panic!("EEPROM init failed")
            });

        let mut storage = microchip_eeprom_25lcxx::Storage::new(eeprom25x);

        let mut opt = Options::load(&mut storage).unwrap_or_else(|_e| {
            defmt::error!("Options load failed");
            Options::default()
        });
        let reg = [
            0x31_u8, 0x32, 0x33, 0x34, 0x35, 0x36, 0x37, 0x38, 0x39, 0x30, 0, 0, 0, 0, 0, 0,
        ];

        opt.set_tdc7200_regs(u128::from_le_bytes(reg));
        defmt::info!("opt: {:x}", opt.into_bytes());

        let mut tdc1000 = TDC1000::new(bus.acquire(), tdc1000_cs, tdc1000_res, tdc1000_en);
        let mut cfg = Config0::default();
        cfg.set_num_tx(31);
        cfg.set_tx_freq_div(FrequencyDividerForTx::Div16);
        let bytes = cfg.into_bytes();
        tdc1000.set_config0(cfg).ok();
        defmt::info!("tdc1000_regs: {:x}", bytes);

        // Initialize TDC7200
        let mut tdc7200 = Tdc7200::new(bus.acquire(), tdc7200_cs);
        tdc7200_en.set_high().ok(); // Enable TDC7200
        tdc7200.reset().ok();
        tdc7200
            .init(
                hardware::tdc7200::Config1::MEASUREMENT_MODE_1
                    | hardware::tdc7200::Config1::START_MEASUREMENT
                    | hardware::tdc7200::Config1::SCLK_DIVIDER_1,
                hardware::tdc7200::Config2::CLOCK_IN_EN
                    | hardware::tdc7200::Config2::CALIBRATION_MODE_CONT
                    | hardware::tdc7200::Config2::CALIBRATION_FREQ_4MHZ
                    | hardware::tdc7200::Config2::NUM_STOP_1,
                hardware::tdc7200::MainControl::empty(),
            )
            .ok();
        // Enable measurement complete interrupt
        tdc7200
            .set_interrupt_mask(
                hardware::tdc7200::InterruptStatus::MEASUREMENT_COMPLETE.bits()
                    | hardware::tdc7200::InterruptStatus::COARSE_COUNTER_OVERFLOW.bits()
                    | hardware::tdc7200::InterruptStatus::TIMEOUT_ERROR.bits(),
            )
            .ok();
        defmt::info!("TDC7200 initialized");

        let mut asd = HourHistory::new(&mut storage).unwrap_or_else(|_e| {
            defmt::error!("HourHistory init failed");
            // Return default empty history — will start fresh
            HourHistory {
                data: ServiceData::default(),
            }
        });
        defmt::info!(
            "read data.size: {:?} {:?} {:?}",
            asd.data.size(),
            asd.first_stored_timestamp(),
            asd.last_stored_timestamp()
        );

        rs_power_en.set_low().ok();

        let mut serial = p
            .USART1
            .usart(
                (tx, rx),
                serial::Config::default().baudrate(hal::time::Bps(115200)),
                &mut rcc,
            )
            .unwrap_or_else(|_e| {
                defmt::error!("USART1 init failed");
                panic!("USART1 init failed")
            });

        serial.listen(hal::serial::Event::Rxne); // Enable RX interrupt for Modbus
        writeln!(serial, "Hello world\r").ok();
        block!(serial.flush()).ok();

        defmt::info!("{}", compile_time::datetime_str!());
        defmt::info!("{}", compile_time::rustc_version_str!());
        let datetime = compile_time::datetime!().saturating_add(Duration::HOUR * 2);
        let asd = datetime.unix_timestamp();
        defmt::info!("unix_timestamp {}", asd);
        if let Err(_e) = rtc.set_datetime(&PrimitiveDateTime::new(datetime.date(), datetime.time()))
        {
            defmt::error!("RTC set datetime failed");
        }
        defmt::info!("rtc init");

        rtc.enable_wakeup(5);
        rtc.listen(&mut p.EXTI, Event::Wakeup);
        // Clear any stale pending flags before entering STOP the first time
        rtc.unpend(Event::Wakeup);
        let mut exti = ExtiExt::new(p.EXTI);

        rcc.apb2enr.modify(|_, w| w.syscfgen().set_bit());

        exti.listen_gpio(&mut p.SYSCFG, 1, 6, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 7, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 8, TriggerEdge::Falling);
        exti.listen_gpio(&mut p.SYSCFG, 1, 9, TriggerEdge::Falling);
        // TDC7200 INT on PB0 (falling edge)
        exti.listen_gpio(&mut p.SYSCFG, 1, 0, TriggerEdge::Falling);

        let mut timer = p.TIM2.timer(20.hz(), &mut rcc);
        timer.listen();
        let mut ui_timer = p.TIM3.timer(10.hz(), &mut rcc);
        ui_timer.listen();

        let power = Power::new(gpio_power, rcc, p.PWR, cx.core.SCB);

        // IWDG: 5-second timeout — if main loop hangs, device resets
        let mut iwdg = p.IWDG.watchdog();
        iwdg.start(1_u32.hz()); // 1 Hz = ~5s timeout with default LSI prescaler
        defmt::info!("IWDG started");
        app_request::spawn(AppRequest::DeepSleep).ok();

        defmt::info!("init end");
        (
            Shared {
                power,
                rtc,
                lcd,
                hour_history: HourHistory::new(&mut storage).unwrap_or_else(|_e| {
                    defmt::error!("HourHistory init");
                    HourHistory {
                        data: ServiceData::default(),
                    }
                }),
                day_history: DayHistory::new(&mut storage).unwrap_or_else(|_e| {
                    defmt::error!("DayHistory init");
                    DayHistory {
                        data: ServiceData::default(),
                    }
                }),
                month_history: MonthHistory::new(&mut storage).unwrap_or_else(|_e| {
                    defmt::error!("MonthHistory init");
                    MonthHistory {
                        data: ServiceData::default(),
                    }
                }),
                storage,
                app: App::default(),
                ui: MenuController::new(),
                modbus_handler: modbus_handler::ModbusHandler::new(1), // Slave address 1
                serial,
                modbus_rx_buf: heapless::Vec::new(),
                modbus_last_rx: 0,
                shell_line_buf: heapless::Vec::new(),
                options: opt,
                tdc1000,
                tdc7200,
                comm_mode: options::CommType::from_u8(opt.comm_type()),
            },
            Local {
                keyboard,
                timer,
                ui_timer,
                handle: None,
                adc,
                photo_r,
                iwdg,
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
                    if let Some(req) = ui.event(event, app) {
                        app_request::spawn(req).ok();
                    }
                }

                if lcd.init() {
                    defmt::info!("lcd init");
                    ui.invalidate();
                }
                ui.update(app);
                ui.get_active();
                ui.render(app, lcd);
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
                ui.get_active();
                ui.render(app, lcd);
            });
            let chan_val: u16 = ctx.local.adc.read(ctx.local.photo_r).unwrap_or(1000u16);
            if chan_val < 500 && *ctx.local.led {
                *ctx.local.led = false;
                app_request::spawn(AppRequest::LcdLed(*ctx.local.led)).ok();
            } else if chan_val > 500 && !*ctx.local.led {
                *ctx.local.led = true;
                app_request::spawn(AppRequest::LcdLed(*ctx.local.led)).ok();
            }
        }
    }

    #[task(capacity = 8, priority = 1, shared = [power, lcd, rtc, app, tdc1000, hour_history, day_history, month_history, storage])]
    fn app_request(ctx: app_request::Context, req: AppRequest) {
        let app_request::SharedResources {
            power,
            mut lcd,
            mut rtc,
            mut app,
            mut tdc1000,
            hour_history,
            day_history,
            month_history,
            mut storage,
        } = ctx.shared;
        match req {
            AppRequest::Process => {
                defmt::info!("Process");
                let datetime = rtc.lock(|rtc| rtc.get_datetime());

                // Trigger real measurement via TDC1000
                let flow = tdc1000.lock(|tdc| {
                    // Set channel 1 (downstream) and start measurement
                    // TDC1000 sends ultrasonic pulses on selected channel
                    if let Err(_e) = tdc.set_channel(false) {
                        defmt::error!("TDC1000 set_channel failed");
                    }
                    // Clear any previous error flags
                    let _ = tdc.clear_error_flags();
                    // TODO: Read TDC7200 measurement results when TDC7200 ISR is connected
                    // TDC7200 INT pin will signal completion — for now return 0.0
                    // until TDC7200 driver is integrated into RTIC
                    0.0f32
                });

                let (hour_flow, day_flow, month_flow) = app.lock(|app| {
                    app.flow = flow;
                    defmt::info!("flow: {}", app.flow);
                    app.hour_flow += app.flow;
                    app.day_flow += app.flow;
                    app.month_flow += app.flow;
                    (app.hour_flow, app.day_flow, app.month_flow)
                });
                if datetime.time().second() < 5 {
                    let timestamp = datetime.as_utc().unix_timestamp();
                    if datetime.time().minute() == 0 {
                        if let Err(_e) =
                            (hour_history, &mut storage).lock(|hour_history, storage| {
                                hour_history.add(storage, hour_flow as i32, timestamp as u32)
                            })
                        {
                            defmt::error!("Failed to log hour flow:");
                        } else {
                            defmt::info!("Hour flow logged: {} at {}", hour_flow, timestamp);
                            // Reset hour accumulator after successful save
                            app.lock(|app| app.hour_flow = 0.0);
                        }

                        if datetime.time().hour() == 0 {
                            if let Err(_e) =
                                (day_history, &mut storage).lock(|day_history, storage| {
                                    day_history.add(storage, day_flow as i32, timestamp as u32)
                                })
                            {
                                defmt::error!("Failed to log day flow:");
                            } else {
                                defmt::info!("Day flow logged: {} at {}", day_flow, timestamp);
                                // Reset day accumulator after successful save
                                app.lock(|app| app.day_flow = 0.0);
                            }

                            if datetime.date().day() == 1 {
                                if let Err(_e) =
                                    (month_history, &mut storage).lock(|month_history, storage| {
                                        month_history.add(
                                            storage,
                                            month_flow as i32,
                                            timestamp as u32,
                                        )
                                    })
                                {
                                    defmt::error!("Failed to log month flow:");
                                } else {
                                    defmt::info!(
                                        "Month flow logged: {} at {}",
                                        month_flow,
                                        timestamp
                                    );
                                    // Reset month accumulator after successful save
                                    app.lock(|app| app.month_flow = 0.0);
                                }
                            }
                        }
                    }
                }
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
                defmt::debug!("DeepSleep");
                (power, lcd).lock(|power, lcd| {
                    power.enter_sleep(|| {
                        // #[cfg(not(feature = "swd"))]
                        lcd.led_off();
                        lcd.off();
                    });
                });
            }
            AppRequest::SetHistory(history_type, timestamp) => {
                defmt::info!("SetHistory");
                match history_type {
                    HistoryType::Hour => {
                        (app, hour_history, storage).lock(|app, hour_history, storage| {
                            if let Ok(Some(flow)) = hour_history.find(storage, timestamp) {
                                app.history_state.flow = Some(flow as f32);
                            } else {
                                app.history_state.flow = None;
                            }
                        });
                    }
                    HistoryType::Day => {
                        (app, day_history, storage).lock(|app, day_history, storage| {
                            if let Ok(Some(flow)) = day_history.find(storage, timestamp) {
                                app.history_state.flow = Some(flow as f32);
                            } else {
                                app.history_state.flow = None;
                            }
                        });
                    }
                    HistoryType::Month => {
                        (app, month_history, storage).lock(|app, month_history, storage| {
                            if let Ok(Some(flow)) = month_history.find(storage, timestamp) {
                                app.history_state.flow = Some(flow as f32);
                            } else {
                                app.history_state.flow = None;
                            }
                        });
                    }
                };
            }
            AppRequest::SetCommType(_idx) => {
                defmt::info!("SetCommType");
                // TODO: save to config, update communication mode
            }
            AppRequest::SetAddress(_addr) => {
                defmt::info!("SetAddress");
                // TODO: save to config, update slave address
            }
            AppRequest::SetMuster(_on) => {
                defmt::info!("SetMuster");
                // TODO: enable/disable muster mode
            }
            AppRequest::SetNegative(_on) => {
                defmt::info!("SetNegative");
                // TODO: enable/disable negative values
            }
            AppRequest::ExitShell => {
                defmt::info!("ExitShell");
                // TODO: exit shell mode
            }
            AppRequest::SystemReset => {
                defmt::info!("SystemReset");
                // TODO: NVIC_SystemReset()
            }
            AppRequest::EnterCalibration => {
                defmt::info!("EnterCalibration");
                // TODO: switch to calibration menu + shell
            }
        }
    }

    /// USART1 RX interrupt — receives bytes for Modbus RTU or Shell
    #[task(binds = USART1, priority = 3, shared = [serial, modbus_rx_buf, modbus_last_rx, shell_line_buf])]
    fn usart1_irq(ctx: usart1_irq::Context) {
        let (mut serial, mut modbus_rx_buf, mut modbus_last_rx, mut shell_line_buf) = (
            ctx.shared.serial,
            ctx.shared.modbus_rx_buf,
            ctx.shared.modbus_last_rx,
            ctx.shared.shell_line_buf,
        );
        serial.lock(|serial| {
            while let Ok(byte) = serial.read() {
                // If byte is printable ASCII or newline, try shell line buffer
                if byte == b'\n' || byte == b'\r' {
                    // End of line — try shell command
                    shell_line_buf.lock(|buf| {
                        if !buf.is_empty() {
                            // Check if this looks like a shell command
                            let is_shell = buf.iter().all(|&b| b.is_ascii());
                            if is_shell {
                                shell_cmd::spawn().ok();
                            } else {
                                // Not a shell command — move to Modbus buffer
                                modbus_rx_buf.lock(|mbuf| {
                                    for &b in buf.iter() {
                                        let _ = mbuf.push(b);
                                    }
                                });
                                buf.clear();
                            }
                        }
                    });
                } else if byte.is_ascii() && byte >= b' ' {
                    // Printable ASCII — accumulate in shell line buffer
                    shell_line_buf.lock(|buf| {
                        if buf.push(byte).is_err() {
                            buf.clear(); // overflow, reset
                        }
                    });
                } else {
                    // Binary byte — Modbus mode, clear shell buffer if any
                    shell_line_buf.lock(|buf| buf.clear());
                    modbus_rx_buf.lock(|mbuf| {
                        if mbuf.len() >= 255 {
                            mbuf.clear();
                        }
                        if mbuf.push(byte).is_err() {
                            mbuf.clear();
                        }
                    });
                }
            }
        });
        modbus_last_rx.lock(|last| *last = monotonics::now().ticks());

        // Check if we have enough for a Modbus frame
        let len = modbus_rx_buf.lock(|buf| buf.len());
        if len >= 8 {
            modbus_poll::spawn().ok();
        }
    }

    /// Process shell command from USART1 line buffer
    #[task(priority = 1, shared = [serial, shell_line_buf])]
    fn shell_cmd(ctx: shell_cmd::Context) {
        let (mut serial, mut shell_line_buf) = (ctx.shared.serial, ctx.shared.shell_line_buf);

        // Take the line buffer contents
        let line = shell_line_buf.lock(|buf| {
            let l = buf.clone();
            buf.clear();
            l
        });

        if line.is_empty() {
            return;
        }

        // Try shell command
        match shell::process_line(&line) {
            shell::ShellResult::Ok(response) => {
                serial.lock(|serial| {
                    for byte in response.as_bytes().iter() {
                        nb::block!(serial.write(*byte)).ok();
                    }
                    // Send prompt
                    nb::block!(serial.write(b'>')).ok();
                    nb::block!(serial.write(b' ')).ok();
                    nb::block!(serial.flush()).ok();
                });
            }
            shell::ShellResult::Error(msg) => {
                serial.lock(|serial| {
                    for byte in b"Error: " {
                        nb::block!(serial.write(*byte)).ok();
                    }
                    for byte in msg.as_bytes() {
                        nb::block!(serial.write(*byte)).ok();
                    }
                    nb::block!(serial.write(b'\r')).ok();
                    nb::block!(serial.write(b'\n')).ok();
                    nb::block!(serial.write(b'>')).ok();
                    nb::block!(serial.write(b' ')).ok();
                    nb::block!(serial.flush()).ok();
                });
            }
            shell::ShellResult::NotAShellCommand => {
                // Not a shell command — ignore (Modbus handles binary separately)
            }
        }
    }

    /// Process complete Modbus RTU frame after 3.5-char silence
    #[task(priority = 1, shared = [serial, modbus_handler, app, options, storage, hour_history, day_history, month_history, modbus_rx_buf, modbus_last_rx])]
    fn modbus_poll(mut ctx: modbus_poll::Context) {
        let mut modbus_last_rx = ctx.shared.modbus_last_rx;
        let now = monotonics::now().ticks();
        let last = modbus_last_rx.lock(|l| *l);
        if now - last < 1 {
            modbus_poll::spawn_after(1_u64.millis()).ok();
            return;
        }

        let frame = ctx.shared.modbus_rx_buf.lock(|buf| {
            let f = buf.clone();
            buf.clear();
            f
        });

        if frame.is_empty() {
            return;
        }

        let (
            modbus_handler,
            app,
            options,
            storage,
            hour_history,
            day_history,
            month_history,
            mut serial,
        ) = (
            ctx.shared.modbus_handler,
            ctx.shared.app,
            ctx.shared.options,
            ctx.shared.storage,
            ctx.shared.hour_history,
            ctx.shared.day_history,
            ctx.shared.month_history,
            ctx.shared.serial,
        );

        (
            modbus_handler,
            options,
            app,
            storage,
            hour_history,
            day_history,
            month_history,
        )
            .lock(
                |modbus_handler,
                 options,
                 app,
                 storage,
                 hour_history,
                 day_history,
                 month_history| {
                    let result = modbus_handler.handle_request(
                        &frame,
                        options,
                        storage,
                        app.flow,
                        app.hour_flow,
                        app.day_flow,
                        app.month_flow,
                        hour_history,
                        day_history,
                        month_history,
                    );

                    if let Ok(response) = result {
                        serial.lock(|serial| {
                            for byte in response.iter() {
                                nb::block!(serial.write(*byte)).ok();
                            }
                            nb::block!(serial.flush()).ok();
                        });
                    }
                },
            );
    }

    /// TDC7200 INT interrupt on PB0 (EXTI0)
    /// Signals that a measurement is complete
    #[task(binds = EXTI0, priority = 4, shared = [tdc7200])]
    fn tdc7200_irq(ctx: tdc7200_irq::Context) {
        // Clear EXTI pending bit for line 0
        ExtiExt::unpend(0);

        let mut tdc7200 = ctx.shared.tdc7200;
        tdc7200.lock(|tdc| {
            // Read interrupt status to determine what happened
            match tdc.get_interrupt_status() {
                Ok(status) => {
                    if status.contains(hardware::tdc7200::InterruptStatus::MEASUREMENT_COMPLETE) {
                        defmt::info!("TDC7200 measurement complete");
                        tdc7200_result::spawn().ok();
                    }
                    if status.contains(hardware::tdc7200::InterruptStatus::TIMEOUT_ERROR) {
                        defmt::warn!("TDC7200 timeout error");
                    }
                    if status.contains(hardware::tdc7200::InterruptStatus::COARSE_COUNTER_OVERFLOW)
                    {
                        defmt::warn!("TDC7200 coarse counter overflow");
                    }
                    let _ = tdc.clear_interrupt_status(status);
                }
                Err(_) => {
                    defmt::error!("TDC7200 SPI read failed");
                }
            }
        });
    }

    /// Read TDC7200 measurement results and calculate flow
    #[task(priority = 2, shared = [tdc7200, app])]
    fn tdc7200_result(ctx: tdc7200_result::Context) {
        let (mut tdc7200, mut app) = (ctx.shared.tdc7200, ctx.shared.app);

        tdc7200.lock(|tdc| {
            // Read measurement results
            let m1 = tdc.get_measurement1();
            let m2 = tdc.get_measurement2();
            let ref_clk = tdc.get_reference_clock_counter();

            match (m1, m2, ref_clk) {
                (Ok(m1_val), Ok(m2_val), Ok(ref_val)) => {
                    defmt::info!("TDC7200: m1={}, m2={}, ref={}", m1_val, m2_val, ref_val);
                    // TODO: Calculate actual flow from TDC measurements
                    // For now, store raw values and mark measurement as done
                    // The flow calculation requires calibration data from Options
                    // and the physical formula: v = L²/(2*m1) * (1/m2 - 1/m1)
                    // where L = distance between transducers
                    app.lock(|app| {
                        app.flow = 0.0; // Placeholder until calculation is implemented
                    });
                }
                _ => {
                    defmt::error!("TDC7200 read failed");
                }
            }
        });
    }

    #[idle(local = [iwdg])]
    fn idle(cx: idle::Context) -> ! {
        let iwdg = cx.local.iwdg;
        loop {
            iwdg.feed();
            cortex_m::asm::wfi();
        }
    }
}
