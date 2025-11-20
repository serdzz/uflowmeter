# HD44780 LCD Display Usage Examples

## Overview

The uflowmeter project uses an HD44780 LCD display for user interface. The LCD is initialized and controlled through the `Lcd` struct combined with `LcdHardware` for the HD44780 interface.

## Architecture

### Hardware Layer
- **LcdHardware** (`src/hardware/hd44780.rs`): Implements the physical interface
  - Controls RS, E, D4-D7 pins
  - Implements `lcd::Hardware` trait
  - Handles timing (delay_us)

### Middleware
- **Lcd** (from `lcd` crate): High-level LCD control
  - Manages cursor position
  - Displays text and characters
  - Power control

### GPIO Pins
```
Pin Configuration:
├── RS (Register Select)
├── E  (Enable)
├── RW (Read/Write)
├── D4-D7 (Data lines)
├── LCD_ON (Backlight control)
└── LCD_LED (LED indicator)
```

## Basic Usage

### 1. Initialization (from main.rs lines 185-188)

```rust
let hd44780 = LcdHardware::new(
    lcd_rs, lcd_e, lcd_d4, lcd_d5, lcd_d6, lcd_d7, lcd_rw
);
let mut lcd = Lcd::new(hd44780, lcd_on, lcd_led);
lcd.init();  // Initialize display
```

### 2. Display Operations

#### Write Text
```rust
lcd.write_str("Hello World!").ok();
```

#### Position Cursor
- Line 1: `lcd.set_cursor_pos(0x00).ok();`
- Line 2: `lcd.set_cursor_pos(0x40).ok();`

#### Control Display
```rust
lcd.power_on().ok();    // Turn on backlight
lcd.power_off().ok();   // Turn off backlight
lcd.clear_display().ok();  // Clear screen
```

## Common Tasks

### Display Multi-line Text
```rust
// Line 1
lcd.set_cursor_pos(0x00).ok();
lcd.write_str("Temperature:").ok();

// Line 2
lcd.set_cursor_pos(0x40).ok();
lcd.write_str("25.5C  Humidity").ok();
```

### Cursor Control
```rust
// Display with blinking cursor
lcd.set_display_mode(Mode::DisplayOnCursorBlinking).ok();

// Display without cursor
lcd.set_display_mode(Mode::DisplayOn).ok();
```

### Dynamic Content Update
```rust
// Clear and update
lcd.clear_display().ok();
lcd.set_cursor_pos(0x00).ok();
lcd.write_str("New Value: 42").ok();
```

## Hardware Timing

The LCD requires specific timing for operations:
- Initialization delay: ~15-40 ms
- Command execution: ~37 µs minimum
- Character write: ~37 µs

These are handled by `LcdHardware::delay_us()` which uses:
```rust
for _ in 1..=delay_usec {
    cortex_m::asm::delay(32);  // Cortex-M instruction cycle
}
```

## Pin Connection Reference

| HD44780 Pin | Signal | Purpose |
|-------------|--------|---------|
| 1 | VSS | Ground |
| 2 | VDD | +5V or +3.3V |
| 3 | V0 | Contrast adjustment |
| 4 | RS | Register Select (GPIO) |
| 5 | RW | Read/Write (typically GND) |
| 6 | E | Enable (GPIO) |
| 7-10 | - | Data lines DB0-DB3 (not used in 4-bit mode) |
| 11-14 | D4-D7 | Data lines (GPIO) |
| 15 | LED+ | Backlight + (through resistor) |
| 16 | LED- | Backlight - (Ground) |

## Data Format (4-bit Mode)

The HD44780 operates in 4-bit mode, sending high nibble then low nibble:

1. Set data lines D4-D7 for high nibble
2. Pulse Enable (E) line
3. Set data lines D4-D7 for low nibble
4. Pulse Enable (E) line

This is handled transparently by the `lcd` crate.

## Addressing Map (16x2 Display)

```
Row 0: 0x00 to 0x0F
Row 1: 0x40 to 0x4F
```

For 20x4 displays:
```
Row 0: 0x00 to 0x13
Row 1: 0x40 to 0x53
Row 2: 0x14 to 0x27
Row 3: 0x54 to 0x67
```

## Performance Considerations

- **Update Speed**: Each character write takes ~37 µs
- **Refresh Rate**: Practical limit ~30 Hz for full display updates
- **Power Consumption**: ~2-5 mA typical (backlight adds 100+ mA)

## Troubleshooting

### Display not showing text
- Check contrast adjustment (V0 potentiometer)
- Verify backlight is enabled
- Check pin connections
- Ensure initialization delay is sufficient

### Garbled display
- Verify 4-bit mode initialization
- Check timing delays
- Review pin order

### Flickering
- Add capacitors on power supply
- Increase pull-up resistor values if needed
- Check for noise on control lines

## Further Reading

- Hitachi HD44780 Datasheet
- `lcd` crate documentation
- STM32L1 HAL documentation
