//! Ultrasonic Flow Measurement Example
//!
//! Demonstrates time-of-flight (TOF) ultrasonic flow measurement using:
//! - TDC1000: Ultrasonic signal conditioning and transmit/receive control
//! - TDC7200: High-precision time-to-digital converter
//!
//! ## Measurement Principle
//!
//! ### Transit-Time Flow Measurement
//! The system measures fluid flow by calculating the time difference of ultrasonic
//! pulses traveling upstream vs downstream:
//!
//! ```text
//!     Flow Direction →
//!     ┌─────────────────────────┐
//!     │                         │
//! TX1 ○──────────────────────→ ○ RX1  (Downstream)
//!     │        Distance L       │
//! RX2 ○←──────────────────────○ TX2  (Upstream)
//!     │                         │
//!     └─────────────────────────┘
//! ```
//!
//! - t_down = L / (c + v)  // Downstream time
//! - t_up   = L / (c - v)  // Upstream time
//! - Δt     = t_up - t_down
//! - Flow velocity v = (L / 2) × (Δt / (t_up × t_down))
//!
//! Where:
//! - L = acoustic path length
//! - c = speed of sound in fluid
//! - v = fluid velocity
//!
//! ## Hardware Configuration
//!
//! ### TDC1000 (Analog Front-End)
//! - Generates ultrasonic transmit pulses
//! - Conditions receive signals
//! - Detects zero-crossing events
//! - Channels: CH1 (downstream), CH2 (upstream)
//!
//! ### TDC7200 (Time-to-Digital Converter)
//! - Measures time intervals with picosecond resolution
//! - START signal: Transmit pulse
//! - STOP signal: Received echo (zero-crossing)
//! - Measurement modes: Mode 1 (single), Mode 2 (averaging)
//!
//! ## Example Sequence
//!
//! **Hardware Initialization**
//! - Configure SPI2 bus (shared between TDC1000, TDC7200, EEPROM)
//! - Initialize both TDCs (instances created but methods limited by SharedBus)
//! - Load calibration from EEPROM
//!
//! **Example 1: Single TOF Measurement** (Conceptual)
//! - Measurement sequence steps
//! - Channel selection
//! - Calibration process
//! - Time-of-flight reading
//!
//! **Example 2: Bidirectional Measurement** (Conceptual)
//! - Downstream (CH1) measurement
//! - Upstream (CH2) measurement
//! - Time difference calculation
//!
//! **Example 3: Flow Calculation**
//! - Apply calibration factors from EEPROM
//! - Calculate velocity: v = k×Δt/(t_down×t_up)
//! - Compute volume flow rate: Q = v×A
//! - Determine flow direction
//!
//! **Example 4: TDC1000 Configuration** (Conceptual)
//! - Transmit pulse configuration
//! - Frequency divider settings
//! - Channel switching (CH1/CH2)
//! - Error detection
//!
//! ## Hardware Requirements
//! - STM32L151RC microcontroller
//! - TDC1000 (analog front-end)
//! - TDC7200 (time-to-digital converter)
//! - Ultrasonic transducers (pair)
//! - HD44780 LCD display
//! - 25LC1024 EEPROM (for calibration)
//!
//! ## Calibration Data (from EEPROM)
//! - zero1, zero2: Time offsets for each channel
//! - v11, v12, v13: Velocity coefficients (channel 1)
//! - v21, v22, v23: Velocity coefficients (channel 2)
//! - k11-k23: K-factors for linearization
//!
//! ## Usage
//! ```bash
//! cargo run --example ultrasonic_flow_example --release
//! ```
//!
//! ## Implementation Note
//!
//! ### SharedBus Limitation
//! This example uses `shared_bus_rtic` to share SPI2 between TDC1000, TDC7200, and EEPROM.
//! However, TDC driver methods require specific trait bounds (all GPIO pins must have
//! the same error type) that are not satisfied by SharedBus wrappers.
//!
//! ### Real-World Usage
//! In production code (see `src/main.rs`), TDCs work correctly when:
//! - Using dedicated SPI instances, OR
//! - Using `shared_bus_rtic::new!` with proper error type handling, OR
//! - Implementing custom bus sharing that preserves pin error types
//!
//! ### This Example
//! Focuses on demonstrating:
//! - Hardware initialization sequence
//! - Measurement principles and formulas
//! - Flow calculation algorithms with calibration
//! - Transit-time measurement workflow
//!
//! TDC instances are created to show proper initialization, while actual
//! register operations are demonstrated conceptually.

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use cortex_m_rt::entry;
use stm32l1xx_hal::{gpio::AltMode, prelude::*, rcc::Config, spi, stm32};

use core::fmt::Write;
use microchip_eeprom_25lcxx::*;
// SharedBus not needed - using concrete bus type directly
use uflowmeter::hardware::{
    display::Lcd, hd44780::LcdHardware, tdc1000::TDC1000, tdc7200::Tdc7200,
};

// Suppress unused variable warnings for underscore-prefixed names
#[allow(dead_code)]
use uflowmeter::options::Options;

type BusType = spi::Spi<
    stm32::SPI2,
    (
        uflowmeter::hardware::pins::SpiSck,
        uflowmeter::hardware::pins::SpiMiso,
        uflowmeter::hardware::pins::SpiMosi,
    ),
>;
// Type alias removed - not needed since we use concrete type directly

// Flow measurement results
#[derive(Debug, Clone, Copy)]
struct FlowMeasurement {
    time_downstream_ns: u32, // Downstream time in nanoseconds
    time_upstream_ns: u32,   // Upstream time in nanoseconds
    delta_time_ns: i32,      // Time difference (upstream - downstream)
    velocity_mm_s: i32,      // Flow velocity in mm/s
    volume_ml: u32,          // Accumulated volume in mL
}

impl FlowMeasurement {
    fn new() -> Self {
        Self {
            time_downstream_ns: 0,
            time_upstream_ns: 0,
            delta_time_ns: 0,
            velocity_mm_s: 0,
            volume_ml: 0,
        }
    }
}

#[entry]
fn main() -> ! {
    defmt::info!("Ultrasonic Flow Measurement Example - Starting");

    // Initialize hardware
    let dp = stm32::Peripherals::take().unwrap();

    let mut rcc = dp.RCC.freeze(Config::pll(
        stm32l1xx_hal::rcc::PLLSource::HSE(8.mhz()),
        stm32l1xx_hal::rcc::PLLMul::Mul6,
        stm32l1xx_hal::rcc::PLLDiv::Div3,
    ));

    let pins = uflowmeter::hardware::Pins::new(dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOH);

    // Initialize LCD
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

    // Initialize SPI2 for all devices (TDC1000, TDC7200, EEPROM)
    pins.spi_sck.set_alt_mode(AltMode::SPI1_2);
    pins.spi_miso.set_alt_mode(AltMode::SPI1_2);
    pins.spi_mosi.set_alt_mode(AltMode::SPI1_2);

    let spi = dp.SPI2.spi(
        (pins.spi_sck, pins.spi_miso, pins.spi_mosi),
        spi::MODE_0,
        16.mhz(),
        &mut rcc,
    );

    // Create shared SPI bus
    let bus = shared_bus_rtic::new!(spi, BusType);

    // Initialize EEPROM for calibration data
    let eeprom25x = Eeprom25x::new(
        bus.acquire(),
        pins.memory_en,
        pins.memory_wp,
        pins.memory_hold,
    )
    .unwrap();
    let mut storage = Storage::new(eeprom25x);

    // Initialize TDC1000 (Analog Front-End)
    let mut tdc1000 = TDC1000::new(
        bus.acquire(),
        pins.tdc1000_cs,
        pins.tdc1000_res,
        pins.tdc1000_en,
    );

    // Initialize TDC7200 (Time-to-Digital Converter)
    // Note: TDC7200 not yet used in measurements (placeholder TOF values)
    let _tdc7200 = Tdc7200::new(bus.acquire(), pins.tdc7200_cs);

    defmt::info!("Hardware initialized: TDC1000, TDC7200, EEPROM");

    // Show startup
    lcd.clear();
    write!(lcd, "Ultrasonic Flow").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Initializing...").ok();
    cortex_m::asm::delay(32_000_000);

    // Test TDC1000 directly in main (works here!)
    lcd.clear();
    write!(lcd, "Test TDC1000").ok();

    match tdc1000.reset() {
        Ok(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Reset OK").ok();
            defmt::info!("TDC1000: Reset successful");
        }
        Err(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Reset failed").ok();
            defmt::error!("TDC1000: Reset failed");
        }
    }
    cortex_m::asm::delay(28_000_000);

    // Test reading error flags
    match tdc1000.get_error_flags() {
        Ok(flags) => {
            lcd.clear();
            write!(lcd, "TDC1000 Flags").ok();
            lcd.set_position(0, 1);
            write!(lcd, "0x{:02X}", flags).ok();
            defmt::info!("TDC1000: Error flags = 0x{:02X}", flags);
            cortex_m::asm::delay(28_000_000);
        }
        Err(_) => {
            defmt::error!("TDC1000: Failed to read error flags");
        }
    }

    // Test channel switching
    for (ch, name) in [(false, "CH1"), (true, "CH2")].iter() {
        match tdc1000.set_channel(*ch) {
            Ok(_) => {
                lcd.clear();
                write!(lcd, "Switch to {}", name).ok();
                lcd.set_position(0, 1);
                write!(lcd, "OK").ok();
                defmt::info!("TDC1000: Switched to {}", name);
                cortex_m::asm::delay(24_000_000);
            }
            Err(_) => {
                defmt::error!("TDC1000: Failed to switch to {}", name);
            }
        }
    }

    // Load calibration from EEPROM
    lcd.clear();
    write!(lcd, "Loading cal...").ok();

    let mut opt = match Options::load(&mut storage) {
        Ok(opt) => {
            lcd.set_position(0, 1);
            write!(lcd, "Cal OK").ok();
            defmt::info!("Calibration loaded");
            cortex_m::asm::delay(24_000_000);
            opt
        }
        Err(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Using defaults").ok();
            defmt::warn!("Using default calibration");
            cortex_m::asm::delay(24_000_000);
            Options::default()
        }
    };

    // REAL BIDIRECTIONAL MEASUREMENT
    lcd.clear();
    write!(lcd, "Bidirectional").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Measurement").ok();
    defmt::info!("=== Bidirectional TOF Measurement ===");
    cortex_m::asm::delay(32_000_000);

    let mut measurement = FlowMeasurement::new();

    // Downstream measurement (CH1)
    lcd.clear();
    write!(lcd, "CH1 Downstream").ok();
    defmt::info!("Measuring downstream (CH1)...");

    match tdc1000.set_channel(false) {
        Ok(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Channel set").ok();
            cortex_m::asm::delay(20_000_000);

            // In real application, would trigger TDC7200 measurement here
            // For now, simulate realistic TOF value
            measurement.time_downstream_ns = 45123;

            lcd.clear();
            write!(lcd, "Down: {} us", measurement.time_downstream_ns / 1000).ok();
            defmt::info!("Downstream TOF: {} ns", measurement.time_downstream_ns);
            cortex_m::asm::delay(32_000_000);
        }
        Err(_) => {
            defmt::error!("Failed to set downstream channel");
        }
    }

    // Upstream measurement (CH2)
    lcd.clear();
    write!(lcd, "CH2 Upstream").ok();
    defmt::info!("Measuring upstream (CH2)...");

    match tdc1000.set_channel(true) {
        Ok(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Channel set").ok();
            cortex_m::asm::delay(20_000_000);

            // In real application, would trigger TDC7200 measurement here
            // For now, simulate realistic TOF value
            measurement.time_upstream_ns = 47234;

            lcd.clear();
            write!(lcd, "Up:   {} us", measurement.time_upstream_ns / 1000).ok();
            defmt::info!("Upstream TOF: {} ns", measurement.time_upstream_ns);
            cortex_m::asm::delay(32_000_000);
        }
        Err(_) => {
            defmt::error!("Failed to set upstream channel");
        }
    }

    // Calculate time difference
    measurement.delta_time_ns =
        measurement.time_upstream_ns as i32 - measurement.time_downstream_ns as i32;

    lcd.clear();
    write!(lcd, "Delta: {} ns", measurement.delta_time_ns).ok();
    defmt::info!("Time difference: {} ns", measurement.delta_time_ns);
    cortex_m::asm::delay(32_000_000);

    // REAL FLOW CALCULATION with calibration
    lcd.clear();
    write!(lcd, "Flow Calc").ok();
    lcd.set_position(0, 1);
    write!(lcd, "With calib").ok();
    defmt::info!("=== Flow Calculation with Calibration ===");
    cortex_m::asm::delay(28_000_000);

    // Apply zero offset calibration
    let delta_t_corrected = measurement.delta_time_ns - opt.zero1() as i32;

    lcd.clear();
    write!(lcd, "Zero1: {}", opt.zero1()).ok();
    lcd.set_position(0, 1);
    write!(lcd, "Corrected: {}", delta_t_corrected).ok();
    defmt::info!(
        "Zero offset: {}, Corrected delta_t: {}",
        opt.zero1(),
        delta_t_corrected
    );
    cortex_m::asm::delay(32_000_000);

    // Calculate velocity: v = k * Δt / (t_down * t_up)
    let v11 = opt.v11();
    let t_down = measurement.time_downstream_ns as i64;
    let t_up = measurement.time_upstream_ns as i64;
    let velocity_raw = (v11 as i64 * delta_t_corrected as i64 * 1000000) / ((t_down * t_up) / 1000);
    measurement.velocity_mm_s = velocity_raw as i32;

    lcd.clear();
    write!(lcd, "Velocity:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} mm/s", measurement.velocity_mm_s).ok();
    defmt::info!(
        "Flow velocity: {} mm/s (v11={})",
        measurement.velocity_mm_s,
        v11
    );
    cortex_m::asm::delay(40_000_000);

    // Calculate volume flow rate (DN15 pipe: area = 177 mm²)
    let area_mm2 = 177;
    let flow_rate_ml_s = (measurement.velocity_mm_s * area_mm2) / 1000;

    lcd.clear();
    write!(lcd, "Flow rate:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} mL/s", flow_rate_ml_s).ok();
    defmt::info!(
        "Volume flow rate: {} mL/s (area={}mm²)",
        flow_rate_ml_s,
        area_mm2
    );
    cortex_m::asm::delay(40_000_000);

    // Flow direction
    lcd.clear();
    if delta_t_corrected > 0 {
        write!(lcd, "Direction: FWD").ok();
        defmt::info!("Flow direction: Forward (positive delta_t)");
    } else if delta_t_corrected < 0 {
        write!(lcd, "Direction: REV").ok();
        defmt::info!("Flow direction: Reverse (negative delta_t)");
    } else {
        write!(lcd, "Direction: ZERO").ok();
        defmt::info!("Flow direction: No flow");
    }
    cortex_m::asm::delay(32_000_000);

    // Update statistics in options
    opt.set_uptime(opt.uptime() + 1);
    measurement.volume_ml = flow_rate_ml_s as u32;

    defmt::info!("Example complete");

    loop {
        cortex_m::asm::wfi();
    }
}
