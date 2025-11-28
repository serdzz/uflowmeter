//! Display (LCD HD44780) Example
//!
//! This example demonstrates how to use the LCD display module with:
//! - Basic LCD initialization and control
//! - Text display with Russian character support
//! - Custom character loading
//! - Backlight and power control
//!
//! ## Hardware Requirements
//! - STM32L151RC microcontroller
//! - HD44780-compatible 16x2 LCD display
//! - GPIO connections as defined in hardware/pins.rs
//!
//! ## LCD Features
//! - 4-bit parallel interface
//! - Dynamic Russian character loading (8 custom character slots)
//! - Backlight LED control
//! - Power management

#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

use cortex_m_rt::entry;
use stm32l1xx_hal::{prelude::*, rcc::Config, stm32};

use core::fmt::Write;
use uflowmeter::hardware::display::Lcd;
use uflowmeter::hardware::hd44780::LcdHardware;

#[entry]
fn main() -> ! {
    defmt::info!("Display Example - Starting");

    // Get device peripherals
    let dp = stm32::Peripherals::take().unwrap();

    // Configure clocks
    let _rcc = dp.RCC.freeze(Config::pll(
        stm32l1xx_hal::rcc::PLLSource::HSE(8.mhz()),
        stm32l1xx_hal::rcc::PLLMul::Mul6,
        stm32l1xx_hal::rcc::PLLDiv::Div3,
    ));

    // Initialize pins
    let pins = uflowmeter::hardware::Pins::new(dp.GPIOA, dp.GPIOB, dp.GPIOC, dp.GPIOD, dp.GPIOH);

    defmt::info!("Initializing LCD...");

    // Create LCD hardware interface
    let hd44780 = LcdHardware::new(
        pins.lcd_rs,
        pins.lcd_e,
        pins.lcd_d4,
        pins.lcd_d5,
        pins.lcd_d6,
        pins.lcd_d7,
        pins.lcd_rw,
    );

    // Create LCD display controller
    let mut lcd = Lcd::new(hd44780, pins.lcd_on, pins.lcd_led);

    // Initialize the LCD
    lcd.init();
    defmt::info!("LCD initialized");

    // Turn on backlight
    lcd.led_on();

    // Example 1: Display simple ASCII text
    defmt::info!("Example 1: ASCII text");
    lcd.clear();
    write!(lcd, "Hello World!").ok();
    lcd.set_position(0, 1); // Move to second line
    write!(lcd, "STM32 LCD Demo").ok();

    cortex_m::asm::delay(32_000_000); // Delay ~2 seconds at 16MHz

    // Example 2: Display Russian text
    // Важно: HD44780 может хранить только 8 пользовательских символов,
    // поэтому используем короткие строки с не более 8 уникальных букв
    defmt::info!("Example 2: Russian characters");
    lcd.clear();
    write!(lcd, "Привет!").ok(); // 6 уникальных символов
    lcd.set_position(0, 1);
    write!(lcd, "LCD HD44780").ok(); // ASCII текст

    cortex_m::asm::delay(32_000_000);

    // Example 2.1: Другие русские слова
    defmt::info!("Example 2.1: More Russian words");
    lcd.clear();
    write!(lcd, "Доброе утро!").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Как дела?").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 2.2: Русский текст - время
    defmt::info!("Example 2.2: Time in Russian");
    lcd.clear();
    write!(lcd, "Время: 12:34").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Дата: 28.11").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 2.3: Статус
    defmt::info!("Example 2.3: Status");
    lcd.clear();
    write!(lcd, "Система").ok();
    lcd.set_position(0, 1);
    write!(lcd, "работает").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 2.4: Счетчик
    defmt::info!("Example 2.4: Counter");
    lcd.clear();
    write!(lcd, "Счетчик:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Литров: 123.4").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 3: Custom character management
    defmt::info!("Example 3: Custom chars");
    lcd.clear();

    // Display info about custom character slots
    let loaded_count = lcd.get_loaded_chars_count();
    defmt::info!("Loaded custom chars: {}", loaded_count);

    write!(lcd, "Custom: ").ok();
    write!(lcd, "{}", loaded_count).ok();
    lcd.set_position(0, 1);
    write!(lcd, "chars loaded").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 4: Reset and clear
    defmt::info!("Example 4: Reset");
    lcd.reset_custom_chars();
    lcd.clear();

    write!(lcd, "Reset done").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Chars cleared").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 5: Backlight control
    defmt::info!("Example 5: Backlight control");
    lcd.clear();
    write!(lcd, "Backlight test").ok();

    // Blink backlight
    for i in 0..5 {
        cortex_m::asm::delay(8_000_000); // 0.5s
        lcd.led_off();
        cortex_m::asm::delay(8_000_000); // 0.5s
        lcd.led_on();
        defmt::info!("Blink {}", i + 1);
    }

    // Example 6: Mixed content with Russian
    defmt::info!("Example 6: Mixed content");
    lcd.clear();
    write!(lcd, "Темп: 23.5C").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Влага: 65%").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 6.1: Меню
    defmt::info!("Example 6.1: Menu");
    lcd.clear();
    write!(lcd, "Меню:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "1.Настройки").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 6.2: Давление
    defmt::info!("Example 6.2: Pressure");
    lcd.clear();
    write!(lcd, "Давление:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "1.2 бар").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 6.3: Расход
    defmt::info!("Example 6.3: Flow rate");
    lcd.clear();
    write!(lcd, "Расход:").ok();
    lcd.set_position(0, 1);
    write!(lcd, "5.7 л/мин").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 6.4: Сообщения
    defmt::info!("Example 6.4: Messages");
    lcd.clear();
    write!(lcd, "Внимание!").ok();
    lcd.set_position(0, 1);
    write!(lcd, "Ошибка!").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 6.5: Состояние
    defmt::info!("Example 6.5: State");
    lcd.clear();
    write!(lcd, "Готово").ok();
    lcd.set_position(0, 1);
    write!(lcd, "к работе").ok();

    cortex_m::asm::delay(32_000_000);

    // Example 7: Power off
    defmt::info!("Example 7: Power off");
    lcd.clear();
    write!(lcd, "Powering off...").ok();
    lcd.set_position(0, 1);
    write!(lcd, "in 2 seconds").ok();

    cortex_m::asm::delay(32_000_000);

    lcd.off();
    defmt::info!("LCD powered off");

    // End of examples
    defmt::info!("Display Example - Complete");

    loop {
        cortex_m::asm::nop();
    }
}
