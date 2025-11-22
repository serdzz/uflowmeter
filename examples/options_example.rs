//! Options/Configuration Management Example
//!
//! Demonstrates persistent configuration storage using EEPROM:
//! - Loading and saving configuration with CRC validation
//! - Dual-page redundancy (primary + secondary backup)
//! - Sensor calibration parameters
//! - Device settings (serial number, slave address, etc.)
//! - Usage statistics (uptime, totals)
//!
//! ## Hardware Requirements
//! - STM32L151RC microcontroller
//! - 25LC1024 EEPROM (128KB SPI)
//! - HD44780 LCD display for status
//!
//! ## Configuration Structure
//!
//! ### Device Identity
//! - Serial number (32-bit)
//! - Sensor type (8-bit)
//! - Slave address for Modbus (8-bit)
//!
//! ### TDC Register Presets
//! - TDC1000 registers (10 bytes / 80 bits)
//! - TDC7200 registers (10 bytes / 80 bits)
//!
//! ### Calibration Data
//! - Zero offsets (zero1, zero2)
//! - Velocity coefficients (v11, v12, v13, v21, v22, v23)
//! - K-factors (k11, k12, k13, k21, k22, k23)
//!
//! ### Usage Statistics
//! - Uptime counter
//! - Total flow accumulator
//! - Hour/Day/Month totals
//!
//! ### Communication Settings
//! - Enable negative flow
//! - Communication type
//! - Modbus mode
//!
//! ## Storage Details
//! - Size: 1024 bytes per copy
//! - Primary offset: 0x0000
//! - Secondary offset: 0x0400 (backup)
//! - CRC-16/CCITT-FALSE for validation
//!
//! ## Usage
//! ```bash
//! cargo run --example options_example --release
//! ```

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use cortex_m_rt::entry;
use stm32l1xx_hal::{gpio::AltMode, prelude::*, rcc::Config, spi, stm32};

use core::fmt::Write;
use microchip_eeprom_25lcxx::*;
use shared_bus_rtic::SharedBus;
use uflowmeter::hardware::display::Lcd;
use uflowmeter::hardware::hd44780::LcdHardware;
use uflowmeter::options::Options;

type BusType = spi::Spi<
    stm32::SPI2,
    (
        uflowmeter::hardware::pins::SpiSck,
        uflowmeter::hardware::pins::SpiMiso,
        uflowmeter::hardware::pins::SpiMosi,
    ),
>;
type MyStorage = Storage<
    SharedBus<BusType>,
    uflowmeter::hardware::pins::MemoryEn,
    uflowmeter::hardware::pins::MemoryWp,
    uflowmeter::hardware::pins::MemoryHold,
>;

#[entry]
fn main() -> ! {
    defmt::info!("Options Management Example - Starting");

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

    // Initialize SPI2 for EEPROM communication
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

    // Initialize 25LC1024 EEPROM
    let eeprom25x = Eeprom25x::new(
        bus.acquire(),
        pins.memory_en,
        pins.memory_wp,
        pins.memory_hold,
    )
    .unwrap();

    let mut storage = Storage::new(eeprom25x);

    defmt::info!("LCD initialized");
    defmt::info!("SPI and EEPROM initialized");

    // Show startup
    lcd.clear();
    write!(lcd, "Options Demo").ok();
    cortex_m::asm::delay(32_000_000);

    // Load current options from EEPROM
    lcd.clear();
    write!(lcd, "Loading options").ok();
    lcd.set_position(0, 1);
    write!(lcd, "from EEPROM...").ok();

    let options_result = Options::load(&mut storage);
    let mut opt = match options_result {
        Ok(opt) => {
            lcd.clear();
            write!(lcd, "Load OK!").ok();
            lcd.set_position(0, 1);
            write!(lcd, "SN: {}", opt.serial_number()).ok();
            cortex_m::asm::delay(32_000_000);
            defmt::info!("Options loaded successfully");
            defmt::info!("Serial number: {}", opt.serial_number());
            defmt::info!("Sensor type: {}", opt.sensor_type());
            opt
        }
        Err(_e) => {
            lcd.clear();
            write!(lcd, "Load Error").ok();
            lcd.set_position(0, 1);
            write!(lcd, "Using defaults").ok();
            cortex_m::asm::delay(32_000_000);
            #[cfg(not(test))]
            defmt::warn!("Failed to load options, using defaults");
            Options::default()
        }
    };

    // Example 1: Configuration Structure
    example_1_structure_overview(&mut lcd, &opt);

    // Example 2: CRC Validation
    example_2_crc_validation(&mut lcd);

    // Example 3: Dual-Page Redundancy
    example_3_redundancy(&mut lcd);

    // Example 4: Configuration Fields
    example_4_config_fields(&mut lcd, &opt);

    // Example 5: Usage Statistics
    example_5_usage_stats(&mut lcd, &opt);

    // Example 6: Save/Load Cycle - with real hardware
    example_6_save_load_cycle(&mut lcd, &mut opt, &mut storage);

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

/// Example 1: Configuration Structure Overview
fn example_1_structure_overview(lcd: &mut Lcd, opt: &Options) {
    defmt::info!("Example 1: Structure Overview");

    lcd.clear();
    write!(lcd, "Ex1: Structure").ok();
    cortex_m::asm::delay(24_000_000);

    // Show size
    lcd.clear();
    write!(lcd, "Options size:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "~100 bytes").ok();
    cortex_m::asm::delay(32_000_000);

    // Storage size
    lcd.clear();
    write!(lcd, "Storage: 1024B").ok();
    lcd.set_position(0, 1);
    write!(lcd, "x2 (redundant)").ok();
    cortex_m::asm::delay(32_000_000);

    // Field groups
    let groups = [
        "Identity",
        "TDC Registers",
        "Calibration",
        "Statistics",
        "Communication",
    ];

    for group in &groups {
        lcd.clear();
        write!(lcd, "Group:").ok();
        lcd.set_position(0, 1);
        write!(lcd, "{}", group).ok();
        cortex_m::asm::delay(24_000_000);
    }

    defmt::info!("Options structure:");
    defmt::info!("  - CRC: 16-bit");
    defmt::info!(
        "  - Serial number: 32-bit (current: {})",
        opt.serial_number()
    );
    defmt::info!("  - Sensor type: 8-bit (current: {})", opt.sensor_type());
    defmt::info!("  - TDC1000 regs: 80-bit");
    defmt::info!("  - TDC7200 regs: 80-bit");
    defmt::info!("  - Calibration: 12x 32-bit");
    defmt::info!("  - Statistics: 6x 32-bit");
    defmt::info!("  - Communication: 4x 8-bit");
}

/// Example 2: CRC Validation
fn example_2_crc_validation(lcd: &mut Lcd) {
    defmt::info!("Example 2: CRC Validation");

    lcd.clear();
    write!(lcd, "Ex2: CRC Check").ok();
    cortex_m::asm::delay(24_000_000);

    // CRC algorithm
    lcd.clear();
    write!(lcd, "Algorithm:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "CRC-16/CCITT").ok();
    cortex_m::asm::delay(32_000_000);

    // Validation flow
    lcd.clear();
    write!(lcd, "1. Load data").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "2. Calculate CRC").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "3. Compare").ok();
    cortex_m::asm::delay(20_000_000);

    // Show validation logic
    lcd.clear();
    write!(lcd, "If CRC match:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Data OK").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "If CRC fail:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Try backup").ok();
    cortex_m::asm::delay(24_000_000);

    defmt::info!("CRC validation process:");
    defmt::info!("  1. Read 1024 bytes from primary (0x0000)");
    defmt::info!("  2. Calculate CRC on bytes[2..]");
    defmt::info!("  3. Compare with stored CRC");
    defmt::info!("  4. If fail, read secondary (0x0400)");
    defmt::info!("  5. If both fail, return WrongCrc error");
}

/// Example 3: Dual-Page Redundancy
fn example_3_redundancy(lcd: &mut Lcd) {
    defmt::info!("Example 3: Dual-Page Redundancy");

    lcd.clear();
    write!(lcd, "Ex3: Redundancy").ok();
    cortex_m::asm::delay(24_000_000);

    // Primary location
    lcd.clear();
    write!(lcd, "Primary:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Addr: 0x0000").ok();
    cortex_m::asm::delay(28_000_000);

    // Secondary location
    lcd.clear();
    write!(lcd, "Secondary:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Addr: 0x0400").ok();
    cortex_m::asm::delay(28_000_000);

    // Save process
    lcd.clear();
    write!(lcd, "On Save:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Write both!").ok();
    cortex_m::asm::delay(28_000_000);

    // Load process
    lcd.clear();
    write!(lcd, "On Load:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Try primary").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "If fail:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Use backup").ok();
    cortex_m::asm::delay(28_000_000);

    // Benefits
    lcd.clear();
    write!(lcd, "Benefit:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Data safety!").ok();
    cortex_m::asm::delay(28_000_000);

    defmt::info!("Redundancy protects against:");
    defmt::info!("  - Write interruption");
    defmt::info!("  - Memory corruption");
    defmt::info!("  - Partial writes");
}

/// Example 4: Configuration Fields
fn example_4_config_fields(lcd: &mut Lcd, opt: &Options) {
    defmt::info!("Example 4: Configuration Fields");

    lcd.clear();
    write!(lcd, "Ex4: Fields").ok();
    cortex_m::asm::delay(24_000_000);

    // Device identity - show real values
    lcd.clear();
    write!(lcd, "Serial: {}", opt.serial_number()).ok();
    lcd.set_position(0, 1);
    write!(lcd, "Type: {}", opt.sensor_type()).ok();
    cortex_m::asm::delay(28_000_000);

    // Calibration
    lcd.clear();
    write!(lcd, "Calibration:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Zero1, Zero2").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "Velocity:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "v11-v13,v21-v23").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "K-factors:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "k11-k13,k21-k23").ok();
    cortex_m::asm::delay(24_000_000);

    // Communication
    lcd.clear();
    write!(lcd, "Modbus Addr: 1").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Mode: RTU").ok();
    cortex_m::asm::delay(28_000_000);

    defmt::info!("Configuration fields:");
    defmt::info!("  Device:");
    defmt::info!("    - serial_number: {} (u32)", opt.serial_number());
    defmt::info!("    - sensor_type: {} (u8)", opt.sensor_type());
    defmt::info!("  Calibration:");
    defmt::info!("    - zero1: {} (u32)", opt.zero1());
    defmt::info!("    - zero2: {} (u32)", opt.zero2());
    defmt::info!("    - v11..v23: 6x 32-bit");
    defmt::info!("    - k11..k23: 6x 32-bit");
    defmt::info!("  Comm:");
    defmt::info!("    - slave_address: {} (u8)", opt.slave_address());
    defmt::info!("    - modbus_mode: u8");
}

/// Example 5: Usage Statistics
fn example_5_usage_stats(lcd: &mut Lcd, opt: &Options) {
    defmt::info!("Example 5: Usage Statistics");

    lcd.clear();
    write!(lcd, "Ex5: Statistics").ok();
    cortex_m::asm::delay(24_000_000);

    // Uptime - show real value
    lcd.clear();
    write!(lcd, "Uptime:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} seconds", opt.uptime()).ok();
    cortex_m::asm::delay(28_000_000);

    // Totals - show real value
    lcd.clear();
    write!(lcd, "Total Flow:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} units", opt.total()).ok();
    cortex_m::asm::delay(28_000_000);

    // Period totals
    lcd.clear();
    write!(lcd, "Hour: 12.5 L").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Day: 234.8 L").ok();
    cortex_m::asm::delay(28_000_000);

    lcd.clear();
    write!(lcd, "Month: 7654 L").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Rest: 100 L").ok();
    cortex_m::asm::delay(28_000_000);

    // Update frequency
    lcd.clear();
    write!(lcd, "Updates:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Periodic save").ok();
    cortex_m::asm::delay(28_000_000);

    defmt::info!("Statistics tracked:");
    defmt::info!("  - uptime: {} seconds", opt.uptime());
    defmt::info!("  - total: {} (cumulative flow)", opt.total());
    defmt::info!("  - hour_total: {} (current hour)", opt.hour_total());
    defmt::info!("  - day_total: {} (current day)", opt.day_total());
    defmt::info!("  - month_total: {} (current month)", opt.month_total());
    defmt::info!("  - rest: {} (remaining quota)", opt.rest());
}

/// Example 6: Save/Load Cycle - with real EEPROM operations
fn example_6_save_load_cycle(lcd: &mut Lcd, opt: &mut Options, storage: &mut MyStorage) {
    defmt::info!("Example 6: Save/Load Cycle (Real Hardware)");

    lcd.clear();
    write!(lcd, "Ex6: Save/Load").ok();
    cortex_m::asm::delay(24_000_000);

    // Load sequence
    lcd.clear();
    write!(lcd, "LOAD Process:").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "1. Read primary").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "2. Check CRC").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "3. If OK: Done").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "4. Else: backup").ok();
    cortex_m::asm::delay(20_000_000);

    // Save sequence
    lcd.clear();
    write!(lcd, "SAVE Process:").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "1. Calc CRC").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "2. Update field").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "3. Write primary").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "4. Write backup").ok();
    cortex_m::asm::delay(20_000_000);

    lcd.clear();
    write!(lcd, "5. Verify").ok();
    cortex_m::asm::delay(20_000_000);

    // Example usage
    lcd.clear();
    write!(lcd, "Usage:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "let opt=load()").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "opt.serial=123").ok();
    lcd.set_position(0, 1);
    write!(lcd, "opt.save()").ok();
    cortex_m::asm::delay(28_000_000);

    // Demonstrate real modification and save
    lcd.clear();
    write!(lcd, "Testing...").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Modify uptime").ok();
    cortex_m::asm::delay(24_000_000);

    // Store old values
    let old_uptime = opt.uptime();
    let old_serial = opt.serial_number();

    // Modify uptime
    opt.set_uptime(old_uptime + 1);

    lcd.clear();
    write!(lcd, "Uptime:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "{} -> {}", old_uptime, opt.uptime()).ok();
    cortex_m::asm::delay(32_000_000);

    // Save to EEPROM
    lcd.clear();
    write!(lcd, "Saving to").ok();
    lcd.set_position(0, 1);
    write!(lcd, "EEPROM...").ok();

    defmt::info!("Saving modified options to EEPROM");
    defmt::info!("  Old uptime: {}", old_uptime);
    defmt::info!("  New uptime: {}", opt.uptime());

    match opt.save(storage) {
        Ok(_) => {
            lcd.clear();
            write!(lcd, "Save OK!").ok();
            lcd.set_position(0, 1);
            write!(lcd, "Verified").ok();
            cortex_m::asm::delay(28_000_000);
            defmt::info!("Save successful!");
        }
        Err(_e) => {
            lcd.clear();
            write!(lcd, "Save Error").ok();
            lcd.set_position(0, 1);
            write!(lcd, "Check EEPROM").ok();
            cortex_m::asm::delay(28_000_000);
            defmt::error!("Save failed");
        }
    }

    // Verify by reading back
    lcd.clear();
    write!(lcd, "Verifying...").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Read back").ok();
    cortex_m::asm::delay(24_000_000);

    match Options::load(storage) {
        Ok(loaded_opt) => {
            lcd.clear();
            if loaded_opt.uptime() == opt.uptime() && loaded_opt.serial_number() == old_serial {
                write!(lcd, "Verify OK!").ok();
                lcd.set_position(0, 1);
                write!(lcd, "Data matches").ok();
                defmt::info!("Verification successful!");
                defmt::info!("  Loaded uptime: {}", loaded_opt.uptime());
            } else {
                write!(lcd, "Verify FAIL").ok();
                lcd.set_position(0, 1);
                write!(lcd, "Mismatch").ok();
                defmt::warn!("Verification mismatch!");
            }
            cortex_m::asm::delay(32_000_000);
        }
        Err(_e) => {
            lcd.clear();
            write!(lcd, "Verify Error").ok();
            lcd.set_position(0, 1);
            write!(lcd, "Read failed").ok();
            cortex_m::asm::delay(28_000_000);
            defmt::error!("Verification read failed");
        }
    }

    defmt::info!("Complete save/load cycle demonstrated");
    defmt::info!("Load:");
    defmt::info!("  let mut opt = Options::load(&mut storage)?;");
    defmt::info!("Modify:");
    defmt::info!("  opt.set_uptime(opt.uptime() + 1);");
    defmt::info!("Save:");
    defmt::info!("  opt.save(&mut storage)?;");
    defmt::info!("Verify:");
    defmt::info!("  let loaded = Options::load(&mut storage)?;");
}
