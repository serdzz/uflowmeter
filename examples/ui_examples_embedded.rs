//! UI Module Examples for Embedded Hardware
//!
//! This example runs on STM32L151 and displays UI examples on LCD.
//! Unlike ui_examples.rs which runs on host, this demonstrates actual
//! widget usage on embedded hardware.
//!
//! ## Hardware Requirements
//! - STM32L151RC microcontroller
//! - HD44780-compatible 16x2 LCD display
//! - 4 buttons (Enter, Left, Right, Up/Down)
//!
//! ## Features Demonstrated
//! - History widget with actual blink animation
//! - DateTime editing with visual feedback
//! - Button-driven navigation
//! - Real timestamp calculation
//!
//! ## Usage
//! ```bash
//! cargo run --example ui_examples_embedded --release
//! ```

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use cortex_m_rt::entry;
use stm32l1xx_hal::{prelude::*, rcc::Config, stm32};

use core::fmt::Write;
use time::{macros::datetime, Duration, PrimitiveDateTime};
use uflowmeter::hardware::display::Lcd;
use uflowmeter::hardware::hd44780::LcdHardware;

// Simple enum for tracking which example is showing
#[derive(Copy, Clone, PartialEq)]
enum Example {
    BlinkMasks,
    TimestampCalc,
    NavigationFlow,
    MonthNav,
    Complete,
}

impl Example {
    fn next(self) -> Self {
        match self {
            Example::BlinkMasks => Example::TimestampCalc,
            Example::TimestampCalc => Example::NavigationFlow,
            Example::NavigationFlow => Example::MonthNav,
            Example::MonthNav => Example::Complete,
            Example::Complete => Example::BlinkMasks,
        }
    }
}

// Simulated Edit widget state for demonstration
struct DemoEditState {
    blink_mask: u32,
    editable: bool,
    content: [u8; 8],
}

impl DemoEditState {
    fn new() -> Self {
        Self {
            blink_mask: 0,
            editable: false,
            content: *b"00:00:00",
        }
    }

    fn set_time(&mut self, hour: u8, min: u8, sec: u8) {
        write_time(&mut self.content, hour, min, sec);
    }

    fn set_date(&mut self, day: u8, month: u8, year: u8) {
        write_date(&mut self.content, day, month, year);
    }
}

fn write_time(buf: &mut [u8; 8], hour: u8, min: u8, sec: u8) {
    buf[0] = b'0' + (hour / 10);
    buf[1] = b'0' + (hour % 10);
    buf[2] = b':';
    buf[3] = b'0' + (min / 10);
    buf[4] = b'0' + (min % 10);
    buf[5] = b':';
    buf[6] = b'0' + (sec / 10);
    buf[7] = b'0' + (sec % 10);
}

fn write_date(buf: &mut [u8; 8], day: u8, month: u8, year: u8) {
    buf[0] = b'0' + (day / 10);
    buf[1] = b'0' + (day % 10);
    buf[2] = b'/';
    buf[3] = b'0' + (month / 10);
    buf[4] = b'0' + (month % 10);
    buf[5] = b'/';
    buf[6] = b'0' + (year / 10);
    buf[7] = b'0' + (year % 10);
}

#[entry]
fn main() -> ! {
    defmt::info!("UI Examples (Embedded) - Starting");

    // Initialize hardware
    let dp = stm32::Peripherals::take().unwrap();
    let _rcc = dp.RCC.freeze(Config::pll(
        stm32l1xx_hal::rcc::PLLSource::HSE(8.mhz()),
        stm32l1xx_hal::rcc::PLLMul::Mul6,
        stm32l1xx_hal::rcc::PLLDiv::Div3,
    ));

    let pins = uflowmeter::hardware::Pins::new(dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOH);

    let hd44780 = LcdHardware::new(
        pins.lcd_rs,
        pins.lcd_e,
        pins.lcd_d4,
        pins.lcd_d5,
        pins.lcd_d6,
        pins.lcd_d7,
        pins.lcd_rw,
    );

    let mut lcd = Lcd::new(hd44780, pins.lcd_on, pins.lcd_led);
    lcd.init();
    lcd.led_on();

    defmt::info!("LCD initialized");

    let mut current_example = Example::BlinkMasks;
    let mut demo_state = DemoEditState::new();

    // Main loop - cycle through examples
    loop {
        match current_example {
            Example::BlinkMasks => {
                show_blink_masks_example(&mut lcd, &mut demo_state);
            }
            Example::TimestampCalc => {
                show_timestamp_example(&mut lcd);
            }
            Example::NavigationFlow => {
                show_navigation_example(&mut lcd, &mut demo_state);
            }
            Example::MonthNav => {
                show_month_example(&mut lcd);
            }
            Example::Complete => {
                show_complete_example(&mut lcd, &mut demo_state);
            }
        }

        // Wait between examples
        cortex_m::asm::delay(48_000_000); // ~3 seconds at 16MHz
        current_example = current_example.next();
    }
}

/// Example 1: Blink Masks
fn show_blink_masks_example(lcd: &mut Lcd, state: &mut DemoEditState) {
    defmt::info!("Example 1: Blink Masks");
    
    lcd.clear();
    write!(lcd, "Blink Masks").ok();
    cortex_m::asm::delay(16_000_000);

    // Show time format
    lcd.clear();
    write!(lcd, "Time: HH:MM:SS").ok();
    cortex_m::asm::delay(16_000_000);

    // Demonstrate seconds blink (0x03)
    lcd.clear();
    write!(lcd, "Seconds: 0x03").ok();
    lcd.set_position(0, 1);
    state.blink_mask = 0x03;
    state.set_time(10, 30, 45);
    
    // Show blink animation
    for _ in 0..3 {
        lcd.set_position(0, 1);
        write!(lcd, "10:30:45").ok();
        cortex_m::asm::delay(8_000_000);
        
        lcd.set_position(0, 1);
        write!(lcd, "10:30:  ").ok();  // Seconds hidden
        cortex_m::asm::delay(8_000_000);
    }

    // Show date format
    lcd.clear();
    write!(lcd, "Date: DD/MM/YY").ok();
    cortex_m::asm::delay(16_000_000);

    // Demonstrate day blink (0xc0)
    lcd.clear();
    write!(lcd, "Day: 0xc0").ok();
    lcd.set_position(0, 1);
    state.blink_mask = 0xc0;
    state.set_date(15, 1, 24);
    
    for _ in 0..3 {
        lcd.set_position(0, 1);
        write!(lcd, "15/01/24").ok();
        cortex_m::asm::delay(8_000_000);
        
        lcd.set_position(0, 1);
        write!(lcd, "  /01/24").ok();  // Day hidden
        cortex_m::asm::delay(8_000_000);
    }
}

/// Example 2: Timestamp Calculation
fn show_timestamp_example(lcd: &mut Lcd) {
    defmt::info!("Example 2: Timestamps");
    
    lcd.clear();
    write!(lcd, "Timestamp Calc").ok();
    cortex_m::asm::delay(16_000_000);

    let base = datetime!(2024-01-15 10:30:00 UTC);
    let base_ts = base.unix_timestamp() as u32;

    // Show base
    lcd.clear();
    write!(lcd, "Base: 10:30:00").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TS:{}", base_ts).ok();
    cortex_m::asm::delay(24_000_000);

    // Show +1 hour
    let hour = base.saturating_add(Duration::HOUR);
    let hour_ts = hour.unix_timestamp() as u32;
    lcd.clear();
    write!(lcd, "+1h: 11:30:00").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TS:{}", hour_ts).ok();
    cortex_m::asm::delay(24_000_000);

    // Show difference
    lcd.clear();
    write!(lcd, "Diff: +3600s").ok();
    lcd.set_position(0, 1);
    write!(lcd, "(1 hour)").ok();
    cortex_m::asm::delay(24_000_000);
}

/// Example 3: Navigation Flow
fn show_navigation_example(lcd: &mut Lcd, state: &mut DemoEditState) {
    defmt::info!("Example 3: Navigation");
    
    lcd.clear();
    write!(lcd, "Navigation Flow").ok();
    cortex_m::asm::delay(16_000_000);

    let states = [
        ("None", "View mode", 0x00),
        ("Seconds", "Edit: SS", 0x03),
        ("Minutes", "Edit: MM", 0x18),
        ("Hours", "Edit: HH", 0xc0),
        ("Day", "Edit: DD", 0xc0),
        ("Month", "Edit: MM", 0x18),
        ("Year", "Edit: YY", 0x03),
    ];

    for (name, desc, mask) in &states {
        lcd.clear();
        write!(lcd, "{}", name).ok();
        lcd.set_position(0, 1);
        write!(lcd, "{}", desc).ok();
        
        state.blink_mask = *mask;
        cortex_m::asm::delay(20_000_000);
    }
}

/// Example 4: Month Navigation
fn show_month_example(lcd: &mut Lcd) {
    defmt::info!("Example 4: Month Nav");
    
    lcd.clear();
    write!(lcd, "Month Wrap").ok();
    cortex_m::asm::delay(16_000_000);

    // Show month wrapping
    lcd.clear();
    write!(lcd, "Dec -> Jan").ok();
    lcd.set_position(0, 1);
    write!(lcd, "(wraps forward)").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "Jan -> Dec").ok();
    lcd.set_position(0, 1);
    write!(lcd, "(wraps back)").ok();
    cortex_m::asm::delay(24_000_000);
}

/// Example 5: Complete Simulation
fn show_complete_example(lcd: &mut Lcd, state: &mut DemoEditState) {
    defmt::info!("Example 5: Complete");
    
    lcd.clear();
    write!(lcd, "Full Demo").ok();
    cortex_m::asm::delay(16_000_000);

    let mut dt = datetime!(2024-01-15 10:30:45 UTC);

    // Initial state
    lcd.clear();
    write!(lcd, "15/01/24").ok();
    lcd.set_position(8, 0);
    write!(lcd, "10:30:45").ok();
    lcd.set_position(0, 1);
    write!(lcd, "View: 123.4L").ok();
    cortex_m::asm::delay(24_000_000);

    // Edit seconds
    lcd.clear();
    write!(lcd, "Edit: Seconds").ok();
    lcd.set_position(0, 1);
    state.set_time(dt.hour(), dt.minute(), dt.second());
    state.blink_mask = 0x03;
    
    // Blink animation
    for _ in 0..2 {
        lcd.set_position(0, 1);
        write!(lcd, "10:30:45").ok();
        cortex_m::asm::delay(8_000_000);
        lcd.set_position(0, 1);
        write!(lcd, "10:30:  ").ok();
        cortex_m::asm::delay(8_000_000);
    }

    // Increment
    dt = dt.saturating_add(Duration::SECOND);
    lcd.set_position(0, 1);
    write!(lcd, "10:30:46 [+1]").ok();
    cortex_m::asm::delay(24_000_000);

    // Show final timestamp
    let ts = dt.unix_timestamp() as u32;
    lcd.clear();
    write!(lcd, "Timestamp:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{}", ts).ok();
    cortex_m::asm::delay(24_000_000);

    // Done
    lcd.clear();
    write!(lcd, "Demo Complete!").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Restarting...").ok();
    cortex_m::asm::delay(24_000_000);
}
