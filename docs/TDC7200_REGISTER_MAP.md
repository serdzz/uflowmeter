# TDC7200 Register Map Documentation

## Overview

The TDC7200 is a High-Resolution Time-to-Digital Converter IC with advanced multi-channel measurement capabilities. It features 30 addressable registers organized in 5 banks for comprehensive time-of-flight and phase measurement.

## Register Summary Table

| Bank | Reg # | Name | Address | Access | Description |
|------|-------|------|---------|--------|-------------|
| 0 | 0 | CONFIG | 0x00 | R/W | General Configuration |
| 0 | 1 | INT_STATUS | 0x01 | RO | Interrupt Status |
| 0 | 2 | INT_MASK | 0x02 | R/W | Interrupt Mask |
| 0 | 3 | COMM_STATUS | 0x03 | RO | Communication Status |
| 0 | 4 | START_CONF | 0x04 | R/W | Start Signal Configuration |
| 0 | 5 | STOP_CONF | 0x05 | R/W | Stop Signal Configuration |
| 0 | 6 | START_CYC | 0x06 | R/W | Start Signal Cycle Configuration |
| 0 | 7 | STOP_CYC | 0x07 | R/W | Stop Signal Cycle Configuration |
| 1 | 8 | TIME1 | 0x10 | RO | Time Result 1 (MSB) |
| 1 | 9 | TIME2 | 0x11 | RO | Time Result 2 |
| 1 | 10 | TIME3 | 0x12 | RO | Time Result 3 (LSB) |
| 1 | 11 | CLOCK_COUNT | 0x13 | RO | Clock Counter |
| 2 | 12 | CALIBRATION | 0x20 | R/W | Calibration Configuration |
| 2 | 13 | CAL_COUNT | 0x21 | RO | Calibration Counter |
| 3 | 14 | PHASE1 | 0x30 | RO | Phase Result 1 (MSB) |
| 3 | 15 | PHASE2 | 0x31 | RO | Phase Result 2 (LSB) |
| 3 | 16 | PHASE3 | 0x32 | RO | Phase Result 3 |
| 4 | 17 | FIFO_STATUS | 0x40 | RO | FIFO Status |
| 4 | 18 | FIFO_DATA | 0x41 | RO | FIFO Data Output |

**Total Registers**: 19 (across 5 banks)  
**Address Range**: 0x00 - 0x41  
**Register Width**: 8-bit (1 byte), 24-bit and 32-bit results

---

## Detailed Register Descriptions

### Bank 0: Configuration Registers (0x00-0x07)

#### Register 0: CONFIG (0x00) - R/W
**General Configuration Register**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | enable | bool | 0 | Enable TDC7200 measurement |
| 1 | meas_mode | bool | 0 | Measurement mode (0=Single, 1=Multi) |
| 2 | start_edge | bool | 0 | Start signal edge (0=Rising, 1=Falling) |
| 3 | stop_edge | bool | 0 | Stop signal edge (0=Rising, 1=Falling) |
| 4 | start_meas | bool | 0 | Start measurement (pulse) |
| 5 | force_cal | bool | 0 | Force calibration (pulse) |
| 6 | parity_en | bool | 1 | Enable parity checking |
| 7 | sel_tac_bin | bool | 0 | TAC bin selection |

**Default Value**: 0x40 (Parity enabled)

---

#### Register 1: INT_STATUS (0x01) - RO
**Interrupt Status Register**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | meas_int | bool | 0 | Measurement interrupt flag |
| 1 | cal_int | bool | 0 | Calibration interrupt flag |
| 2 | fifo_int | bool | 0 | FIFO interrupt flag |
| 3 | clock_int | bool | 0 | Clock interrupt flag |
| 4 | err_int | bool | 0 | Error interrupt flag |
| 7:5 | (reserved) | - | 0 | Reserved |

**Default Value**: 0x00

---

#### Register 2: INT_MASK (0x02) - R/W
**Interrupt Mask Register**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | meas_int_mask | bool | 0 | Mask measurement interrupt |
| 1 | cal_int_mask | bool | 0 | Mask calibration interrupt |
| 2 | fifo_int_mask | bool | 0 | Mask FIFO interrupt |
| 3 | clock_int_mask | bool | 0 | Mask clock interrupt |
| 4 | err_int_mask | bool | 0 | Mask error interrupt |
| 7:5 | (reserved) | - | 0 | Reserved |

**Default Value**: 0x00

---

#### Register 3: COMM_STATUS (0x03) - RO
**Communication Status Register**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | spi_error | bool | 0 | SPI communication error |
| 1 | data_ready | bool | 0 | Data ready flag |
| 2 | parity_error | bool | 0 | Parity error detected |
| 3 | eeprom_error | bool | 0 | EEPROM error |
| 7:4 | (reserved) | - | 0 | Reserved |

**Default Value**: 0x00

---

#### Register 4: START_CONF (0x04) - R/W
**Start Signal Configuration**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 3:0 | start_signal | B4 | 0 | Start signal selection |
| 5:4 | start_mode | B2 | 0 | Start mode (0=Single, 1=Dual, 2=Multi) |
| 7:6 | (reserved) | - | 0 | Reserved |

---

#### Register 5: STOP_CONF (0x05) - R/W
**Stop Signal Configuration**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 3:0 | stop_signal | B4 | 0 | Stop signal selection |
| 5:4 | stop_mode | B2 | 0 | Stop mode |
| 7:6 | (reserved) | - | 0 | Reserved |

---

#### Register 6: START_CYC (0x06) - R/W
**Start Signal Cycle Configuration**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 4:0 | start_cycle | B5 | 1 | Start cycle count (1-32) |
| 7:5 | (reserved) | - | 0 | Reserved |

**Default Value**: 0x01

---

#### Register 7: STOP_CYC (0x07) - R/W
**Stop Signal Cycle Configuration**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 4:0 | stop_cycle | B5 | 1 | Stop cycle count (1-32) |
| 7:5 | (reserved) | - | 0 | Reserved |

**Default Value**: 0x01

---

### Bank 1: Measurement Results (0x10-0x13)

#### Register 8: TIME1 (0x10) - RO
**Time Result Register 1 (Most Significant Byte)**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | time_msb | u8 | Time measurement MSB |

**Part of 24-bit TIME result: `TIME = (TIME1 << 16) | (TIME2 << 8) | TIME3`**

---

#### Register 9: TIME2 (0x11) - RO
**Time Result Register 2 (Middle Byte)**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | time_mid | u8 | Time measurement middle byte |

---

#### Register 10: TIME3 (0x12) - RO
**Time Result Register 3 (Least Significant Byte)**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | time_lsb | u8 | Time measurement LSB |

**Time Range**: 0 - 16,777,215 TDC units (24-bit)

---

#### Register 11: CLOCK_COUNT (0x13) - RO
**Clock Counter Register**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | clock_count | u8 | System clock counter |

**Used for debugging and timing verification**

---

### Bank 2: Calibration (0x20-0x21)

#### Register 12: CALIBRATION (0x20) - R/W
**Calibration Configuration**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 0 | cal_enable | bool | 0 | Enable automatic calibration |
| 2:1 | cal_periods | B2 | 0 | Calibration periods (1, 2, 4, 8) |
| 3 | cal_auto_disable | bool | 0 | Auto-disable after calibration |
| 7:4 | (reserved) | - | 0 | Reserved |

---

#### Register 13: CAL_COUNT (0x21) - RO
**Calibration Counter**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | cal_count | u8 | Calibration counter value |

---

### Bank 3: Phase Results (0x30-0x32)

#### Register 14: PHASE1 (0x30) - RO
**Phase Result Register 1 (Most Significant)**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | phase_msb | u8 | Phase measurement MSB |

**Part of 24-bit PHASE result: `PHASE = (PHASE1 << 16) | (PHASE2 << 8) | PHASE3`**

---

#### Register 15: PHASE2 (0x31) - RO
**Phase Result Register 2**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | phase_mid | u8 | Phase measurement middle byte |

---

#### Register 16: PHASE3 (0x32) - RO
**Phase Result Register 3 (Least Significant)**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | phase_lsb | u8 | Phase measurement LSB |

**Phase Range**: 0 - 16,777,215 phase units (24-bit)

---

### Bank 4: FIFO Interface (0x40-0x41)

#### Register 17: FIFO_STATUS (0x40) - RO
**FIFO Status Register**

| Bits | Name | Type | Default | Description |
|------|------|------|---------|-------------|
| 3:0 | fifo_level | B4 | 0 | Number of entries in FIFO (0-15) |
| 4 | fifo_full | bool | 0 | FIFO is full |
| 5 | fifo_empty | bool | 1 | FIFO is empty |
| 7:6 | (reserved) | - | 0 | Reserved |

**FIFO Depth**: 16 entries

---

#### Register 18: FIFO_DATA (0x41) - RO
**FIFO Data Output**

| Bits | Name | Type | Description |
|------|------|------|-------------|
| 7:0 | fifo_data | u8 | Data from FIFO |

**Reading this register automatically advances the FIFO pointer**

---

## SPI Protocol

### Address Format
- **Read**: Address byte with MSB=1 (OR with 0x80)
- **Write**: Address byte with MSB=0

### Read/Write Operations
```
Read:  CS_LOW -> WRITE(addr | 0x80) -> READ(data) -> CS_HIGH
Write: CS_LOW -> WRITE(addr) -> WRITE(data) -> CS_HIGH
```

---

## Key Features vs TDC1000

| Feature | TDC7200 | TDC1000 |
|---------|---------|---------|
| Registers | 19 (5 banks) | 10 |
| Resolution | High (24-bit) | Standard (16-bit) |
| Modes | Multi-channel | Single channel |
| FIFO | 16-entry | N/A |
| Calibration | Automatic | Manual |
| Phase Measurement | Yes | No |
| Banks | 5 | N/A |
| Interrupts | Multiple | Basic |

---

## Initialization Sequence

1. **Reset Device** (if applicable)
2. **Configure Signals** (Registers 0x04-0x07)
   - Set START_CONF, STOP_CONF
   - Set START_CYC, STOP_CYC
3. **Enable Calibration** (Register 0x20)
   - Set calibration periods
4. **Configure Interrupts** (Registers 0x02)
   - Set interrupt masks as needed
5. **Enable Device** (Register 0x00)
   - Set enable bit
6. **Start Measurement** (Register 0x00)
   - Pulse start_meas bit

---

## Measurement Data Reading

### Single Measurement Mode
1. Wait for MEAS_INT interrupt
2. Read TIME1, TIME2, TIME3 (0x10-0x12)
3. Calculate: `TIME = (TIME1 << 16) | (TIME2 << 8) | TIME3`
4. Read CLOCK_COUNT if needed (0x13)

### Multi-Channel Mode with FIFO
1. Monitor FIFO_STATUS (0x40)
2. When data available:
   - Read FIFO_DATA (0x41) repeatedly
   - Parse multi-byte results from FIFO
3. Continue until FIFO_EMPTY is set

---

## Common Configuration Examples

### Basic Single Channel
```
CONFIG = 0x41        // Enable, parity on
START_CONF = 0x00    // Start on CH0
STOP_CONF = 0x01     // Stop on CH1
START_CYC = 0x01     // 1 cycle
STOP_CYC = 0x01      // 1 cycle
```

### Multi-Channel with FIFO
```
CONFIG = 0x43        // Enable, multi-mode, parity
START_CONF = 0x0F    // All channels
STOP_CONF = 0x0F     // All channels
CALIBRATION = 0x02   // Auto calibration, 2 periods
INT_MASK = 0x04      // Enable FIFO interrupt
```

### High-Precision Phase Mode
```
CONFIG = 0x41        // Enable measurement
CALIBRATION = 0x06   // Auto calibration, 4 periods
START_CONF = 0x00
STOP_CONF = 0x02
```

---

## Error Handling

### COMM_STATUS Error Flags
- **SPI_ERROR (bit 0)**: SPI communication error detected
- **PARITY_ERROR (bit 2)**: Parity check failed
- **EEPROM_ERROR (bit 3)**: EEPROM read/write error

### Recovery
1. Read COMM_STATUS to identify error
2. Retry operation or reset device if needed
3. Clear error flags via CONFIG write

---

## Timing Specifications

- **TAC (Time-to-Amplitude Converter)**: High resolution time capture
- **Measurement Cycle**: Typical 1-32 clock periods
- **Calibration Time**: Depends on calibration periods selected
- **FIFO Read Rate**: Must keep pace with measurement rate

---

## Related Documentation

- [TDC1000 Register Map](TDC1000_REGISTER_MAP.md) - Simpler single-channel variant
- [Hardware Integration Guide](HARDWARE_INTEGRATION.md)
- [Ultrasonic Flow Measurement](ULTRASONIC_FLOW.md)
- TDC7200 Datasheet: Texas Instruments

---

## Glossary

- **TAC**: Time-to-Amplitude Converter
- **FIFO**: First-In-First-Out buffer
- **MSB**: Most Significant Byte
- **LSB**: Least Significant Byte
- **TDC**: Time-to-Digital Converter
- **CAL**: Calibration

