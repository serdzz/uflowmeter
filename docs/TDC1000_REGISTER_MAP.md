# TDC1000 Register Map Documentation

## Overview

The TDC1000 is a Time-to-Digital Converter IC with 10 addressable registers. This document describes all registers, their addresses, access modes, and bit fields.

## Register Summary Table

| Reg # | Name | Address | Access | Description |
|-------|------|---------|--------|-------------|
| 0 | Config0 | 0x00 | R/W | Device Configuration 0 - TX frequency & pulse count |
| 1 | Config1 | 0x01 | R/W | Device Configuration 1 - Mode & channel selection |
| 2 | Config2 | 0x02 | R/W | Device Configuration 2 - Pulse mask & clock selection |
| 3 | Config3 | 0x03 | R/W | Device Configuration 3 - Transducer freq & gain |
| 4 | Config4 | 0x04 | R/W | Device Configuration 4 - TDC resolution & range |
| 5 | TOF1 | 0x05 | R/W | Time of Flight Register 1 (High byte) |
| 6 | TOF0 | 0x06 | R/W | Time of Flight Register 0 (Low byte) |
| 7 | ErrorFlags | 0x07 | R/W | Error Flags - Status & error indicators |
| 8 | Timeout | 0x08 | R/W | Timeout Register - Measurement timeout |
| 9 | ClockRate | 0x09 | R/W | Clock Rate Register - Clock configuration |

**Total Registers**: 10  
**Address Range**: 0x00 - 0x09  
**Register Width**: 8-bit (1 byte)

---

## Detailed Register Descriptions

### Register 0: Config0 (0x00) - R/W
**Device Configuration Register 0**

Controls TX frequency divider and number of TX pulses.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 2:0 | tx_freq_div | B3 | 0x5 | TX Frequency Divider (0-7: Div2-Div256) |
| 7:3 | num_tx | B5 | 0x4 | Number of TX Pulses (1-31) |

**Frequency Divider Options**:
- 0 = Div2 (divide by 2)
- 1 = Div4 (divide by 4)
- 2 = Div8 (divide by 8)
- 3 = Div16 (divide by 16)
- 4 = Div32 (divide by 32)
- 5 = Div64 (divide by 64)
- 6 = Div128 (divide by 128)
- 7 = Div256 (divide by 256)

**Default Value**: 0x45 (TX Div64, 5 pulses)

---

### Register 1: Config1 (0x01) - R/W
**Device Configuration Register 1**

Controls measurement mode, continuous operation, and channel selection.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | measurement_mode | bool | 0 | Measurement mode (0=SingleShot, 1=Continuous) |
| 1 | continuous_mode | bool | 0 | Enable continuous measurement mode |
| 2 | channel_select | bool | 0 | Channel selection (0=CH1, 1=CH2) |
| 7:3 | (reserved) | - | 0 | Reserved bits |

**Measurement Modes**:
- 0 = Single shot measurement
- 1 = Continuous measurement

**Default Value**: 0x00

---

### Register 2: Config2 (0x02) - R/W
**Device Configuration Register 2**

Controls TX pulse mask and clock selection.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 4:0 | tx_pulse_mask | B5 | 0 | Transmit pulse mask pattern |
| 6:5 | clock_select | B2 | 0 | Clock source selection |
| 7 | (reserved) | - | 0 | Reserved |

**Clock Selection Options**:
- 0 = Internal oscillator
- 1 = External clock input 1
- 2 = External clock input 2
- 3 = External clock input 3

**Default Value**: 0x00

---

### Register 3: Config3 (0x03) - R/W
**Device Configuration Register 3**

Controls transducer frequency and amplifier gain settings.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 2:0 | transducer_freq | B3 | 0 | Transducer frequency selection (0-7) |
| 5:3 | amplifier_gain | B3 | 0 | Amplifier gain setting (0-7) |
| 7:6 | (reserved) | - | 0 | Reserved |

**Transducer Frequency Options** (typical):
- 0 = 500 kHz
- 1 = 1 MHz
- 2 = 2 MHz
- 3 = 4 MHz
- 4-7 = Reserved

**Amplifier Gain Options**:
- 0 = 0 dB
- 1 = 6 dB
- 2 = 12 dB
- 3 = 18 dB
- 4 = 24 dB
- 5 = 30 dB
- 6 = 36 dB
- 7 = 42 dB

**Default Value**: 0x00

---

### Register 4: Config4 (0x04) - R/W
**Device Configuration Register 4**

Controls TDC resolution, measurement range, and auto-calibration.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 2:0 | tdc_resolution | B3 | 0 | TDC resolution (0-7) |
| 4:3 | measurement_range | B2 | 0 | Measurement range selection |
| 5 | auto_calibration | bool | 0 | Enable auto-calibration |
| 7:6 | (reserved) | - | 0 | Reserved |

**TDC Resolution Options**:
- 0 = 250 ps
- 1 = 500 ps
- 2 = 1 ns
- 3 = 2 ns
- 4 = 4 ns
- 5 = 8 ns
- 6 = 16 ns
- 7 = 32 ns

**Measurement Range Options**:
- 0 = 0-100 ns
- 1 = 0-200 ns
- 2 = 0-400 ns
- 3 = 0-800 ns

**Auto-Calibration**:
- 0 = Manual calibration
- 1 = Automatic calibration enabled

**Default Value**: 0x00

---

### Register 5: TOF1 (0x05) - R/W
**Time of Flight Register 1 (High Byte)**

High byte of the 16-bit Time of Flight measurement result.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 7:0 | tof_high | u8 | 0 | Time of flight high byte |

**Usage**: Combined with TOF0 to form 16-bit TOF value: `tof_value = (TOF1 << 8) | TOF0`

**Default Value**: 0x00

---

### Register 6: TOF0 (0x06) - R/W
**Time of Flight Register 0 (Low Byte)**

Low byte of the 16-bit Time of Flight measurement result.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 7:0 | tof_low | u8 | 0 | Time of flight low byte |

**Usage**: Combined with TOF1 to form 16-bit TOF value: `tof_value = (TOF1 << 8) | TOF0`

**Default Value**: 0x00

**Note**: Time of Flight calculation depends on clock frequency and TDC resolution.

---

### Register 7: ErrorFlags (0x07) - R/W
**Error Flags Register**

Status and error indicator flags from the device.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | tof_error | bool | 0 | Time of Flight error flag |
| 1 | cal_error | bool | 0 | Calibration error flag |
| 2 | range_overflow | bool | 0 | Range overflow flag |
| 3 | adc_overflow | bool | 0 | ADC overflow flag |
| 7:4 | (reserved) | - | 0 | Reserved |

**Error Flag Meanings**:
- **tof_error**: TOF measurement failed or invalid
- **cal_error**: Calibration operation failed
- **range_overflow**: Measurement exceeds configured range
- **adc_overflow**: ADC saturation detected

**Default Value**: 0x00

**Clearing Errors**: Write 0x03 to this register to clear error flags.

---

### Register 8: Timeout (0x08) - R/W
**Timeout Register**

Sets the measurement timeout in clock cycles.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 7:0 | timeout_value | u8 | 0xFF | Timeout value (0-255 cycles) |

**Timeout Calculation**: `timeout = timeout_value * clock_period`

**Default Value**: 0xFF (Maximum timeout)

**Notes**:
- 0 = Timeout disabled
- 1-255 = Timeout in clock cycles
- Timeout prevents infinite wait if no echo is received

---

### Register 9: ClockRate (0x09) - R/W
**Clock Rate Register**

Controls internal clock divider and enable settings.

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 3:0 | clock_divider | B4 | 0 | Clock divider ratio (0-15) |
| 4 | clock_enable | bool | 0 | Internal clock enable |
| 7:5 | (reserved) | - | 0 | Reserved |

**Clock Divider Options**:
- 0 = Div1 (no division)
- 1 = Div2
- 2 = Div4
- 3 = Div8
- 4 = Div16
- 5-15 = Higher divisions

**Clock Enable**:
- 0 = Clock disabled
- 1 = Internal clock enabled

**Default Value**: 0x00

**Output Clock Frequency**: `f_out = f_in / (2^divider)` when enabled

---

## Access Patterns

### SPI Protocol
- **Address Format**: Address byte | MSB = Read flag (0x40)
- **Read Operation**: Send `(addr | 0x40)`, then read data byte
- **Write Operation**: Send `(addr | 0x40)`, then send data byte

### Code Example (Rust)
```rust
// Reading register 0x07 (ErrorFlags)
let addr = 0x07 | 0x40;  // Set read flag
cs.set_low()?;
spi.write(&[addr])?;
let mut data = [0u8];
spi.transfer(&mut data)?;  // Read response
cs.set_high()?;
let error_flags = data[0];

// Writing to register 0x00 (Config0) 
let addr = 0x00 | 0x40;  // Write (0x40 flag for write)
let value = 0x45;         // Config0 value
cs.set_low()?;
spi.write(&[addr, value])?;
cs.set_high()?;
```

---

## Register Access Methods

### TDC1000 Driver Methods

```rust
// Read/Write raw bytes
pub fn read_register(&mut self, address: u8) -> Result<u8, Error>
pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), Error>

// Read/Write structured registers
pub fn get_config0(&mut self) -> Result<Config0, Error>
pub fn set_config0(&mut self, cfg: Config0) -> Result<(), Error>

// Utility functions
pub fn read_all_registers(&mut self, data: &mut [u8]) -> Result<(), Error>
pub fn set_config(&mut self, bytes: &[u8]) -> Result<(), Error>
pub fn set_channel(&mut self, ch: bool) -> Result<(), Error>
pub fn get_error_flags(&mut self) -> Result<u8, Error>
pub fn clear_error_flags(&mut self) -> Result<(), Error>
```

---

## Initialization Sequence

Typical device initialization:

1. **Reset Device**
   - Call `reset()` method
   - Wait for device to stabilize

2. **Configure Clock** (Register 0x09)
   - Set clock divider if needed
   - Enable internal clock

3. **Configure TX** (Register 0x00)
   - Set TX frequency divider
   - Set number of TX pulses

4. **Configure Measurement** (Registers 0x01-0x04)
   - Select measurement mode
   - Set channel and frequency
   - Set amplifier gain
   - Configure TDC resolution
   - Set measurement range

5. **Configure Timeout** (Register 0x08)
   - Set appropriate timeout value

6. **Enable Measurement**
   - Start measurement cycle

---

## Common Configuration Examples

### Ultra-Sonic Flow Measurement (Default)
```
Config0 = 0x45  // Div64, 5 pulses
Config1 = 0x00  // Single-shot, CH1
Config2 = 0x00  // Default pulse mask
Config3 = 0x00  // 500kHz, 0dB gain
Config4 = 0x00  // 250ps resolution, 0-100ns range
Timeout = 0xFF  // Max timeout
```

### High-Precision Configuration
```
Config0 = 0x02  // Div4, 2 pulses (faster TX)
Config1 = 0x01  // Continuous mode
Config3 = 0x36  // 4MHz, 36dB gain (higher signal)
Config4 = 0x05  // 8ns resolution, 0-800ns range
```

### Low-Power Configuration
```
Config0 = 0xC4  // Div256, 4 pulses
Config1 = 0x00  // Single-shot
Config4 = 0x07  // 32ns resolution (lower precision)
Timeout = 0x10  // Short timeout
```

---

## Bit Notation Guide

- **u8**: Unsigned 8-bit value (0-255)
- **bool**: Boolean flag (0/1)
- **B3**: 3-bit field (0-7)
- **B5**: 5-bit field (0-31)
- **B2**: 2-bit field (0-3)
- **B4**: 4-bit field (0-15)
- **RO**: Read-Only register
- **RW**: Read-Write register

---

## References

- TDC1000 Datasheet: Texas Instruments
- Register Access: SPI interface with 0x40 read flag
- Clock Source: Internal or external
- Resolution: 250ps to 32ns (configurable)
- Measurement Range: 100ns to 800ns (configurable)

---

## Related Documentation

- [TDC7200 Register Map](TDC7200_REGISTER_MAP.md) - Companion TDC IC
- [Hardware Integration Guide](HARDWARE_INTEGRATION.md)
- [Ultrasonic Flow Measurement](ULTRASONIC_FLOW.md)
