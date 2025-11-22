//! Power Management Example
//!
//! Demonstrates the power management system including:
//! - GPIO state saving and restoration
//! - Sleep mode entry and exit
//! - Activity timeout tracking
//! - Low power mode configuration
//!
//! ## Hardware Requirements
//! - STM32L151RC microcontroller
//! - HD44780-compatible 16x2 LCD display (optional for status display)
//! - Button for wakeup interrupt
//!
//! ## Power Management Features
//!
//! ### GpioPower Module
//! Saves and restores GPIO register states when entering/exiting sleep:
//! - MODER (mode register)
//! - OTYPER (output type register)
//! - OSPEEDR (output speed register)
//! - AFRH/AFRL (alternate function registers)
//! - PUPDR (pull-up/pull-down register)
//! - ODR (output data register)
//!
//! ### Power Module
//! Manages system power states:
//! - Activity tracking with configurable timeout (default: 15 seconds)
//! - Automatic sleep mode entry
//! - Wake-on-interrupt support
//! - Clock reconfiguration on wake
//!
//! ## Usage
//! ```bash
//! # Build and run on hardware
//! cargo run --example power_management_example --release
//!
//! # Build with low power feature enabled
//! cargo run --example power_management_example --release --features low_power
//! ```
//!
//! ## Example Flow
//! 1. Initialize hardware and display status
//! 2. Show active mode with activity countdown
//! 3. Enter sleep mode after timeout
//! 4. Wake on button press
//! 5. Restore all GPIO states
//! 6. Repeat cycle

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use cortex_m_rt::entry;
use stm32l1xx_hal::{prelude::*, rcc::Config, stm32};

use core::fmt::Write;
use uflowmeter::hardware::display::Lcd;
use uflowmeter::hardware::hd44780::LcdHardware;
use uflowmeter::hardware::gpio_power::GpioPower;

// Mock Power structure for example (real one requires RTIC context)
struct SimplePower {
    gpio_power: GpioPower,
    sleep: bool,
    active_mode: u32,
}

impl SimplePower {
    const IDLE_TIMEOUT: u32 = 15_000; // 15 seconds in ms

    fn new(gpio_power: GpioPower) -> Self {
        Self {
            gpio_power,
            sleep: false,
            active_mode: 0,
        }
    }

    fn active(&mut self, time_ms: u32) {
        self.active_mode = time_ms;
        self.sleep = false;
        defmt::info!("Power: Active mode, timeout in {}ms", Self::IDLE_TIMEOUT);
    }

    fn is_active(&self, current_time_ms: u32) -> bool {
        if current_time_ms - self.active_mode >= Self::IDLE_TIMEOUT {
            return false;
        }
        true
    }

    fn enter_sleep(&mut self) {
        if !self.sleep {
            self.sleep = true;
            defmt::info!("Power: Entering sleep mode");
            
            // Save GPIO state and configure for low power
            self.gpio_power.down();
            
            // In real implementation with 'low_power' feature:
            // - Configure PWR registers
            // - Set ultra-low power mode
            // - Disable voltage detector
            // - Enter stop mode
            
            defmt::info!("Power: Sleep mode active (WFI)");
        }
    }

    fn exit_sleep(&mut self) -> bool {
        let was_sleeping = self.sleep;
        if self.sleep {
            self.sleep = false;
            defmt::info!("Power: Exiting sleep mode");
            
            // Restore GPIO state
            self.gpio_power.up();
            
            // In real implementation with 'low_power' feature:
            // - Clear sleep deep flag
            // - Restore clocks (HSE, HSI)
            // - Reconfigure MCO
            // - Update RCC
            
            defmt::info!("Power: Normal mode restored");
        }
        was_sleeping
    }
}

#[entry]
fn main() -> ! {
    defmt::info!("Power Management Example - Starting");

    // Initialize hardware
    let dp = stm32::Peripherals::take().unwrap();
    let cp = cortex_m::Peripherals::take().unwrap();
    
    let _rcc = dp.RCC.freeze(Config::pll(
        stm32l1xx_hal::rcc::PLLSource::HSE(8.mhz()),
        stm32l1xx_hal::rcc::PLLMul::Mul6,
        stm32l1xx_hal::rcc::PLLDiv::Div3,
    ));

    let pins = uflowmeter::hardware::Pins::new(
        dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOH
    );

    // Initialize LCD for status display
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

    // Show startup message
    lcd.clear();
    write!(lcd, "Power Mgmt Demo").ok();
    cortex_m::asm::delay(32_000_000); // 2 seconds

    // Initialize GPIO power management
    defmt::info!("Initializing GPIO power management");
    let gpio_power = GpioPower::new();
    let mut power = SimplePower::new(gpio_power);

    // Example 1: Show GPIO state saving
    example_1_gpio_state_saving(&mut lcd);

    // Example 2: Activity timeout demonstration
    example_2_activity_timeout(&mut lcd, &mut power, &cp);

    // Example 3: Sleep/Wake cycle
    example_3_sleep_wake_cycle(&mut lcd, &mut power);

    // Example 4: Power consumption comparison
    example_4_power_comparison(&mut lcd);

    // Continuous operation loop
    lcd.clear();
    write!(lcd, "Demo Complete").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Looping...").ok();

    defmt::info!("Example complete, entering loop");

    let mut counter = 0u32;
    loop {
        cortex_m::asm::delay(16_000_000); // 1 second
        counter += 1;
        
        // Simulate activity every 10 seconds
        if counter % 10 == 0 {
            power.active(counter * 1000);
            lcd.clear();
            write!(lcd, "Active: {}s", counter).ok();
        }
        
        // Check if should sleep
        if !power.is_active(counter * 1000) && !power.sleep {
            lcd.clear();
            write!(lcd, "Entering Sleep").ok();
            cortex_m::asm::delay(16_000_000);
            lcd.led_off();
            power.enter_sleep();
        }
    }
}

/// Example 1: GPIO State Saving
fn example_1_gpio_state_saving(lcd: &mut Lcd) {
    defmt::info!("Example 1: GPIO State Saving");
    
    lcd.clear();
    write!(lcd, "Ex1: GPIO Save").ok();
    cortex_m::asm::delay(24_000_000);

    lcd.clear();
    write!(lcd, "Saving states:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "A,B,C,D,H").ok();
    cortex_m::asm::delay(32_000_000);

    // GpioPower automatically saves state on creation
    lcd.clear();
    write!(lcd, "Registers saved:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "MODER,OTYPER...").ok();
    cortex_m::asm::delay(32_000_000);

    lcd.clear();
    write!(lcd, "Saved:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "- MODE").ok();
    cortex_m::asm::delay(16_000_000);
    
    lcd.set_position(0, 1);
    write!(lcd, "- OUTPUT TYPE").ok();
    cortex_m::asm::delay(16_000_000);
    
    lcd.set_position(0, 1);
    write!(lcd, "- SPEED      ").ok();
    cortex_m::asm::delay(16_000_000);
    
    lcd.set_position(0, 1);
    write!(lcd, "- ALT FUNC   ").ok();
    cortex_m::asm::delay(16_000_000);
    
    lcd.set_position(0, 1);
    write!(lcd, "- PULL UP/DN ").ok();
    cortex_m::asm::delay(16_000_000);
    
    lcd.set_position(0, 1);
    write!(lcd, "- OUTPUT DATA").ok();
    cortex_m::asm::delay(16_000_000);

    lcd.clear();
    write!(lcd, "Ready for").ok();
    lcd.set_position(0, 1);
    write!(lcd, "low power mode").ok();
    cortex_m::asm::delay(32_000_000);
}

/// Example 2: Activity Timeout
fn example_2_activity_timeout(
    lcd: &mut Lcd, 
    power: &mut SimplePower,
    _cp: &cortex_m::Peripherals
) {
    defmt::info!("Example 2: Activity Timeout");
    
    lcd.clear();
    write!(lcd, "Ex2: Timeout").ok();
    cortex_m::asm::delay(24_000_000);

    // Set initial activity
    power.active(0);

    lcd.clear();
    write!(lcd, "Timeout: 15s").ok();
    cortex_m::asm::delay(24_000_000);

    // Simulate time passing
    for i in 0..16 {
        let current_time = i * 1000;
        let remaining = if power.is_active(current_time) {
            (SimplePower::IDLE_TIMEOUT - (current_time - power.active_mode)) / 1000
        } else {
            0
        };

        lcd.clear();
        if power.is_active(current_time) {
            write!(lcd, "Active: {}s", i).ok();
            lcd.set_position(0, 1);
            write!(lcd, "Sleep in: {}s", remaining).ok();
        } else {
            write!(lcd, "IDLE TIMEOUT!").ok();
            lcd.set_position(0, 1);
            write!(lcd, "Ready to sleep").ok();
        }
        
        cortex_m::asm::delay(16_000_000); // ~1 second
        
        defmt::info!("Time: {}s, Active: {}, Remaining: {}s", 
            i, power.is_active(current_time), remaining);
    }

    cortex_m::asm::delay(16_000_000);
}

/// Example 3: Sleep/Wake Cycle
fn example_3_sleep_wake_cycle(lcd: &mut Lcd, power: &mut SimplePower) {
    defmt::info!("Example 3: Sleep/Wake Cycle");
    
    lcd.clear();
    write!(lcd, "Ex3: Sleep/Wake").ok();
    cortex_m::asm::delay(24_000_000);

    // Show pre-sleep state
    lcd.clear();
    write!(lcd, "Before Sleep:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "LCD ON, LED ON").ok();
    cortex_m::asm::delay(32_000_000);

    // Enter sleep
    lcd.clear();
    write!(lcd, "Entering Sleep").ok();
    lcd.set_position(0, 1);
    write!(lcd, "GPIO saved...").ok();
    cortex_m::asm::delay(24_000_000);

    defmt::info!("Entering sleep mode...");
    lcd.led_off();
    power.enter_sleep();
    
    // Simulate sleep time
    cortex_m::asm::delay(48_000_000); // 3 seconds
    
    // Wake up
    defmt::info!("Waking up...");
    let was_sleeping = power.exit_sleep();
    
    lcd.led_on();
    lcd.clear();
    if was_sleeping {
        write!(lcd, "Woke from sleep").ok();
    } else {
        write!(lcd, "Was not asleep").ok();
    }
    lcd.set_position(0, 1);
    write!(lcd, "GPIO restored").ok();
    cortex_m::asm::delay(32_000_000);

    lcd.clear();
    write!(lcd, "After Wake:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "All restored!").ok();
    cortex_m::asm::delay(32_000_000);
}

/// Example 4: Power Consumption Info
fn example_4_power_comparison(lcd: &mut Lcd) {
    defmt::info!("Example 4: Power Consumption Comparison");
    
    lcd.clear();
    write!(lcd, "Ex4: Power Info").ok();
    cortex_m::asm::delay(24_000_000);

    // Show different power modes
    let modes = [
        ("Run Mode", "~8mA @ 16MHz"),
        ("Sleep Mode", "~4mA"),
        ("Stop Mode", "~2uA"),
        ("Standby", "~0.3uA"),
    ];

    for (mode, current) in &modes {
        lcd.clear();
        write!(lcd, "{}", mode).ok();
        lcd.set_position(0, 1);
        write!(lcd, "{}", current).ok();
        cortex_m::asm::delay(32_000_000);
    }

    // Show savings
    lcd.clear();
    write!(lcd, "Stop vs Run:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "~4000x savings").ok();
    cortex_m::asm::delay(32_000_000);

    lcd.clear();
    write!(lcd, "Wake sources:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "EXTI, RTC, WDG").ok();
    cortex_m::asm::delay(32_000_000);
}
