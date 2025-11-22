# Examples

This directory contains examples demonstrating various hardware components and features of the uFlowmeter project.

## Available Examples

### Hardware Examples

- **`display_example.rs`** - LCD HD44780 display usage
  - Run on hardware: `cargo run --example display_example --release`
  - See: `lcd_display_usage.md` for detailed documentation

### UI Examples

- **`ui_examples.rs`** - Runnable UI examples (host-only)
  - Run with: `make ui-examples` or `cargo run --example ui_examples`
  - 6 interactive examples demonstrating blink masks, timestamps, and navigation
  - Includes unit tests

- **`ui_examples.md`** - UI module documentation with code examples
  - History widget navigation
  - DateTime editing with blink masks
  - Timestamp calculation
  - Complete flow diagrams

## Available Examples

### 1. `hd44780_basic.rs`
**HD44780 LCD Display Basic Operations**

This is a documentation example showing how to initialize and use the HD44780 LCD display module.

**Topics covered:**
- LCD initialization
- Text display
- Cursor positioning
- Display modes
- Power control
- Custom characters
- Backlight control

**Key API methods:**
- `LcdHardware::new()` - Create hardware interface
- `Lcd::new()` - Create LCD controller
- `lcd.init()` - Initialize display
- `lcd.write_str()` - Display text
- `lcd.set_cursor_pos()` - Position cursor
- `lcd.power_on()`/`lcd.power_off()` - Control backlight

**Hardware requirements:**
- STM32L1xx microcontroller
- HD44780 LCD display (16x2 or 20x4)
- GPIO pins for RS, E, D4-D7, RW
- Control pins for backlight and LED

### 2. `lcd_display_usage.md`
**Comprehensive HD44780 LCD Usage Guide**

Detailed guide with:
- Architecture overview
- Pin configuration
- Initialization procedures
- Common tasks
- Hardware timing specifications
- Pin connection reference
- Addressing map for different display sizes
- Performance considerations
- Troubleshooting guide

## How to Use

### For Documentation Examples
These examples serve as reference implementations. They show the typical usage patterns and can be adapted to your specific hardware configuration.

### For Running Examples (when embedded)
These examples are designed for the embedded environment (STM32L1xx). To run:

1. Ensure your hardware is connected according to the pin configuration
2. The examples can be adapted and included in your firmware

## Project Context

The uflowmeter project is an ultrasonic flow meter running on STM32L1xx microcontroller with:
- HD44780 LCD display for user interface
- SPI interface for sensors (TDC1000, TDC7200)
- EEPROM storage (25LCxx series)
- Real-time clock (RTC)
- ADC for analog measurements

## LCD Architecture in uflowmeter

```
User Code
    ↓
Lcd (lcd crate) - High-level API
    ↓
LcdHardware (src/hardware/hd44780.rs) - Hardware abstraction
    ↓
GPIO Pins (RS, E, D4-D7, RW, ON, LED)
    ↓
HD44780 Display Controller
    ↓
LCD Panel
```

## References

- **LCD Crate**: Provides high-level LCD interface
- **HD44780 Datasheet**: Complete technical specifications
- **STM32L1 HAL**: Hardware abstraction layer
- **Implementation**: `src/hardware/hd44780.rs` and `src/hardware/display.rs`
