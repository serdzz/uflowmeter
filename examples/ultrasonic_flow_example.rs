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
use stm32l1xx_hal::{
    gpio::AltMode,
    prelude::*, 
    rcc::Config, 
    spi,
    stm32,
};

use core::fmt::Write;
use microchip_eeprom_25lcxx::*;
use shared_bus_rtic::SharedBus;
use uflowmeter::hardware::{
    display::Lcd,
    hd44780::LcdHardware,
    tdc1000::TDC1000,
    tdc7200::Tdc7200,
};

// Suppress unused variable warnings for underscore-prefixed names
#[allow(dead_code)]
use uflowmeter::options::Options;

type BusType = spi::Spi<stm32::SPI2, (uflowmeter::hardware::pins::SpiSck, uflowmeter::hardware::pins::SpiMiso, uflowmeter::hardware::pins::SpiMosi)>;
type MyStorage = Storage<SharedBus<BusType>, uflowmeter::hardware::pins::MemoryEn, uflowmeter::hardware::pins::MemoryWp, uflowmeter::hardware::pins::MemoryHold>;

// Flow measurement results
#[derive(Debug, Clone, Copy)]
struct FlowMeasurement {
    time_downstream_ns: u32,  // Downstream time in nanoseconds
    time_upstream_ns: u32,    // Upstream time in nanoseconds
    delta_time_ns: i32,       // Time difference (upstream - downstream)
    velocity_mm_s: i32,       // Flow velocity in mm/s
    volume_ml: u32,           // Accumulated volume in mL
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

    let pins = uflowmeter::hardware::Pins::new(
        dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOH
    );

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
        pins.memory_hold
    ).unwrap();
    let mut storage = Storage::new(eeprom25x);

    // Initialize TDC1000 (Analog Front-End)
    let mut tdc1000 = TDC1000::new(
        bus.acquire(),
        pins.tdc1000_cs,
        pins.tdc1000_res,
        pins.tdc1000_en,
    );

    // Initialize TDC7200 (Time-to-Digital Converter)
    let mut tdc7200 = Tdc7200::new(
        bus.acquire(),
        pins.tdc7200_cs,
    );

    defmt::info!("Hardware initialized: TDC1000, TDC7200, EEPROM");

    // Show startup
    lcd.clear();
    write!(lcd, "Ultrasonic Flow").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Initializing...").ok();
    cortex_m::asm::delay(32_000_000);

    // Load calibration from EEPROM
    lcd.clear();
    write!(lcd, "Loading cal...").ok();
    
    let opt = match Options::load(&mut storage) {
        Ok(opt) => {
            lcd.set_position(0, 1);
            write!(lcd, "Cal OK").ok();
            defmt::info!("Calibration loaded");
            cortex_m::asm::delay(24_000_000);
            opt
        },
        Err(_) => {
            lcd.set_position(0, 1);
            write!(lcd, "Using defaults").ok();
            defmt::warn!("Using default calibration");
            cortex_m::asm::delay(24_000_000);
            Options::default()
        },
    };

    // Note: TDC1000 and TDC7200 are initialized above
    // Methods cannot be called due to SharedBus trait bound limitations
    // See main.rs for working TDC usage without SharedBus
    
    lcd.clear();
    write!(lcd, "TDCs Ready").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TDC1000+7200").ok();
    defmt::info!("TDC1000 size: {} bytes", core::mem::size_of_val(&tdc1000));
    defmt::info!("TDC7200 size: {} bytes", core::mem::size_of_val(&tdc7200));
    cortex_m::asm::delay(32_000_000);

    // Example 1: Single TOF Measurement (conceptual)
    example_1_single_measurement_concept(&mut lcd);

    // Example 2: Bidirectional Measurement (conceptual)
    example_2_bidirectional_measurement_concept(&mut lcd);

    // Example 3: Flow Calculation with Calibration
    example_3_flow_calculation(&mut lcd, &opt);

    // Example 4: TDC1000 Configuration (conceptual)
    example_4_tdc1000_config_concept(&mut lcd);

    // Done
    lcd.clear();
    write!(lcd, "Demo Complete").ok();
    lcd.set_position(0, 1);
    write!(lcd, "See defmt log").ok();

    defmt::info!("Example complete");

    loop {
        cortex_m::asm::wfi();
    }
}

/// Example 1: Single TOF Measurement (Conceptual)
fn example_1_single_measurement_concept(lcd: &mut Lcd) {
    defmt::info!("Example 1: Single TOF Measurement (Conceptual)");
    
    lcd.clear();
    write!(lcd, "Ex2: TOF Meas.").ok();
    cortex_m::asm::delay(24_000_000);

    // Measurement sequence
    lcd.clear();
    write!(lcd, "1. Select CH1").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Downstream").ok();
    defmt::info!("Step 1: Select channel 1 (downstream)");
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "2. Calibrate").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TDC7200...").ok();
    defmt::info!("Step 2: Calibrate TDC7200");
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "3. Trigger TX").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Start measure").ok();
    defmt::info!("Step 3: Trigger transmit & start measurement");
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "4. Wait RX").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Zero-crossing").ok();
    defmt::info!("Step 4: Wait for receive zero-crossing");
    cortex_m::asm::delay(24_000_000);

    // Simulated result
    let simulated_tof = 45123u32;  // 45.123 µs
    lcd.clear();
    write!(lcd, "TOF: {} us", simulated_tof / 1000).ok();
    lcd.set_position(0, 1);
    write!(lcd, "Raw: {}", simulated_tof).ok();
    defmt::info!("Simulated TOF measurement: {} ns", simulated_tof);
    cortex_m::asm::delay(40_000_000);
    
    defmt::info!("Single measurement sequence complete");
}

/// Example 2: Bidirectional Measurement (Conceptual)
fn example_2_bidirectional_measurement_concept(lcd: &mut Lcd) {
    defmt::info!("Example 2: Bidirectional Measurement (Conceptual)");
    
    lcd.clear();
    write!(lcd, "Ex3: Bidir").ok();
    cortex_m::asm::delay(24_000_000);

    let mut measurement = FlowMeasurement::new();

    // Downstream measurement (CH1)
    lcd.clear();
    write!(lcd, "CH1 Downstream").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Measuring...").ok();
    defmt::info!("Measuring downstream (CH1)");
    cortex_m::asm::delay(24_000_000);
    
    // Simulated downstream TOF
    measurement.time_downstream_ns = 45123;  // 45.123 µs
    lcd.clear();
    write!(lcd, "Downstream:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "t={} us", measurement.time_downstream_ns / 1000).ok();
    defmt::info!("Downstream TOF: {} ns", measurement.time_downstream_ns);
    cortex_m::asm::delay(32_000_000);

    // Upstream measurement (CH2)
    lcd.clear();
    write!(lcd, "CH2 Upstream").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Measuring...").ok();
    defmt::info!("Measuring upstream (CH2)");
    cortex_m::asm::delay(24_000_000);
    
    // Simulated upstream TOF
    measurement.time_upstream_ns = 47234;  // 47.234 µs
    lcd.clear();
    write!(lcd, "Upstream:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "t={} us", measurement.time_upstream_ns / 1000).ok();
    defmt::info!("Upstream TOF: {} ns", measurement.time_upstream_ns);
    cortex_m::asm::delay(32_000_000);

    // Calculate time difference
    measurement.delta_time_ns = measurement.time_upstream_ns as i32 - measurement.time_downstream_ns as i32;
    
    lcd.clear();
    write!(lcd, "Delta t:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} ns", measurement.delta_time_ns).ok();
    defmt::info!("Time difference: {} ns", measurement.delta_time_ns);
    cortex_m::asm::delay(40_000_000);

    // Show both times
    lcd.clear();
    write!(lcd, "Down: {} us", measurement.time_downstream_ns / 1000).ok();
    lcd.set_position(0, 1);
    write!(lcd, "Up:   {} us", measurement.time_upstream_ns / 1000).ok();
    defmt::info!("Results - Down: {} ns, Up: {} ns, Delta: {} ns", 
                 measurement.time_downstream_ns, 
                 measurement.time_upstream_ns,
                 measurement.delta_time_ns);
    cortex_m::asm::delay(40_000_000);
    
    defmt::info!("Bidirectional measurement complete");
}

/// Example 3: Flow Calculation with Calibration
fn example_3_flow_calculation(
    lcd: &mut Lcd,
    opt: &Options,
) {
    defmt::info!("Example 3: Flow Calculation");
    
    lcd.clear();
    write!(lcd, "Ex4: Flow Calc").ok();
    cortex_m::asm::delay(24_000_000);

    // Show calibration data
    lcd.clear();
    write!(lcd, "Calibration:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "z1={} z2={}", opt.zero1(), opt.zero2()).ok();
    defmt::info!("Zero offsets: z1={}, z2={}", opt.zero1(), opt.zero2());
    cortex_m::asm::delay(32_000_000);

    // Simulate measurement values
    let t_down = 45000u32;  // 45 µs downstream
    let t_up = 47000u32;    // 47 µs upstream
    let delta_t = (t_up as i32 - t_down as i32) - opt.zero1() as i32;
    
    lcd.clear();
    write!(lcd, "Times (us):").ok();
    lcd.set_position(0, 1);
    write!(lcd, "D:{} U:{}", t_down / 1000, t_up / 1000).ok();
    cortex_m::asm::delay(32_000_000);

    // Calculate velocity (simplified formula)
    // v = k * Δt / (t_down * t_up)
    // Using v11 as velocity coefficient
    let v11 = opt.v11();
    let velocity_raw = (v11 as i64 * delta_t as i64 * 1000000) / ((t_down as i64 * t_up as i64) / 1000);
    let velocity_mm_s = velocity_raw as i32;
    
    lcd.clear();
    write!(lcd, "Velocity:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} mm/s", velocity_mm_s).ok();
    defmt::info!("Flow velocity: {} mm/s (raw: {})", velocity_mm_s, velocity_raw);
    cortex_m::asm::delay(40_000_000);

    // Convert to volume flow rate
    // Q = v * A (where A = pipe cross-section area)
    // Assuming DN15 pipe: diameter = 15mm, area ≈ 177 mm²
    let area_mm2 = 177;
    let flow_rate_ml_s = (velocity_mm_s * area_mm2) / 1000;
    
    lcd.clear();
    write!(lcd, "Flow rate:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} mL/s", flow_rate_ml_s).ok();
    defmt::info!("Flow rate: {} mL/s", flow_rate_ml_s);
    cortex_m::asm::delay(40_000_000);

    // Show flow direction
    lcd.clear();
    if delta_t > 0 {
        write!(lcd, "Direction: +").ok();
        lcd.set_position(0, 1);
        write!(lcd, "Forward flow").ok();
        defmt::info!("Flow direction: Forward");
    } else if delta_t < 0 {
        write!(lcd, "Direction: -").ok();
        lcd.set_position(0, 1);
        write!(lcd, "Reverse flow").ok();
        defmt::info!("Flow direction: Reverse");
    } else {
        write!(lcd, "Direction: 0").ok();
        lcd.set_position(0, 1);
        write!(lcd, "No flow").ok();
        defmt::info!("Flow direction: None");
    }
    cortex_m::asm::delay(40_000_000);
    
    defmt::info!("Flow calculation complete");
}

/// Example 4: TDC1000 Configuration (Conceptual)
fn example_4_tdc1000_config_concept(lcd: &mut Lcd) {
    defmt::info!("Example 4: TDC1000 Configuration (Conceptual)");
    
    lcd.clear();
    write!(lcd, "Ex5: TDC1000").ok();
    cortex_m::asm::delay(24_000_000);

    // Configuration parameters
    lcd.clear();
    write!(lcd, "Config params:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Pulses & Freq").ok();
    defmt::info!("TDC1000 configuration parameters");
    cortex_m::asm::delay(24_000_000);

    // Transmit pulses
    let pulse_counts = [8, 16, 24, 32];
    for pulses in &pulse_counts {
        lcd.clear();
        write!(lcd, "TX Pulses: {}", pulses).ok();
        lcd.set_position(0, 1);
        write!(lcd, "Burst length").ok();
        defmt::info!("Transmit pulses: {}", pulses);
        cortex_m::asm::delay(24_000_000);
    }

    // Frequency dividers
    lcd.clear();
    write!(lcd, "Freq dividers:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Div 2..256").ok();
    defmt::info!("Frequency dividers: Div2, Div4, Div8, Div16, Div32, Div64, Div128, Div256");
    cortex_m::asm::delay(28_000_000);

    // Channel selection
    lcd.clear();
    write!(lcd, "Channels:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "CH1 & CH2").ok();
    defmt::info!("Channels: CH1 (downstream), CH2 (upstream)");
    cortex_m::asm::delay(24_000_000);

    // CH1 downstream
    lcd.clear();
    write!(lcd, "CH1: Downstream").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TX1 -> RX1").ok();
    defmt::info!("CH1: Downstream measurement TX1->RX1");
    cortex_m::asm::delay(28_000_000);

    // CH2 upstream
    lcd.clear();
    write!(lcd, "CH2: Upstream").ok();
    lcd.set_position(0, 1);
    write!(lcd, "TX2 -> RX2").ok();
    defmt::info!("CH2: Upstream measurement TX2->RX2");
    cortex_m::asm::delay(28_000_000);

    // Error detection
    lcd.clear();
    write!(lcd, "Error flags:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Signal quality").ok();
    defmt::info!("Error flags: Signal too low/high detection");
    cortex_m::asm::delay(24_000_000);

    // Final summary
    lcd.clear();
    write!(lcd, "TDC1000 AFE:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Key component").ok();
    defmt::info!("TDC1000: Analog front-end for ultrasonic sensing");
    cortex_m::asm::delay(32_000_000);
}
