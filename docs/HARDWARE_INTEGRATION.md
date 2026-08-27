# Hardware Integration Guide

## Overview

Schematics are in this directory: `FM-2-SCH.pdf` (overview),
`FM-02-SCH-CPU.pdf` (MCU sheet, U3) and `FM-02-SCH-TDC.pdf` (analog
front end). The pin table below was read off the CPU sheet and checked
against `src/main.rs`.

This guide covers the hardware integration for the UFlowMeter ultrasonic flow measurement system. It includes MCU setup, peripheral configuration, and integration with TDC1000/TDC7200 time-to-digital converters.

## System Architecture

```
┌─────────────────────────────────────────────┐
│         STM32L151 MCU (Cortex-M3)          │
│                                             │
│  ┌─────────┐  ┌────────┐  ┌────────┐      │
│  │  UART   │  │  SPI   │  │  GPIO  │      │
│  └────┬────┘  └────┬───┘  └────┬───┘      │
│       │            │            │          │
└───────┼────────────┼────────────┼──────────┘
        │            │            │
    ┌───▼──┐    ┌───▼──┐    ┌───▼──┐
    │Debug │    │TDC   │    │GPIO  │
    │Port  │    │Ctrl  │    │Ctrl  │
    └──────┘    └──────┘    └──────┘
        │            │            │
    USB-UART     SPI Devices   Power/Reset
                 (TDC1000/7200)
```

## Pin Configuration

Taken from `docs/FM-02-SCH-CPU.pdf` (sheet U3, STM32L15X-64-2) and
checked line by line against `src/main.rs`. Net names are the
schematic's; every assignment below matches what the firmware
configures.

### Port A

| Pin | Net | Firmware |
|---|---|---|
| PA0 | `WKUP` | — |
| PA1 | `IR_SD` | — |
| PA2 | `IR_TXD` | — |
| PA3 | `IR_RXD` | — |
| PA4 | `D4` | LCD data |
| PA5 | `D5` | LCD data |
| PA6 | `D6` | LCD data |
| PA7 | `D7` | LCD data |
| PA8 | `CLK_MCO_8MHZ` | MCO, the TDC reference clock |
| PA9 | `U1TX` | USART1 |
| PA10 | `U1RX` | USART1, also the EXTI wake source |
| PA11 | `OSC_EN` | driven low |
| PA13 | `SWDIO` | debug, not configured by firmware |
| PA14 | `SWCLK` | debug, not configured by firmware |

### Port B

| Pin | Net | Firmware |
|---|---|---|
| PB0 | `TDC7200_INT` | measurement-complete input |
| PB1 | `TDC7200_EN` | |
| PB2 | `BOOT1` | — |
| PB3 | `SW_EN` | `SensorMux` bit 0 |
| PB4 | `SW_A0` | `SensorMux` bit 1 |
| PB5 | `SW_A1` | `SensorMux` bit 2 |
| PB6 | `BUT_SET` | keypad |
| PB7 | `BUT_ENTER` | keypad |
| PB8 | `BUT_DOWN` | keypad |
| PB9 | `BUT_UP` | keypad |
| PB10 | `TDC1000_EN` | |
| PB11 | `SPI_TDC1000_CS` | |
| PB12 | `SPI_TDC7200_CS` | |
| PB13 | `SPI_CLK` | SPI2 |
| PB14 | `SPI_MISO` | SPI2 |
| PB15 | `SPI_MOSI` | SPI2 |

The mux encoding follows from the net names: bit 0 enables, bits 1-2
address. `Channel::Off = 0`, `One = 1`, `Two = 3` — enable alone, then
enable plus address bit 0.

### Port C

| Pin | Net | Firmware |
|---|---|---|
| PC0 | `LCD_ON` | panel supply gate |
| PC1 | `LCD_RS` | |
| PC2 | `LCD_RW` | |
| PC3 | `LCD_E` | |
| PC4 | `PHOTO_R` | — |
| PC5 | `LCD_LED_ON` | backlight, active low |
| PC6 | `TDC1000_RES` | **active high, idles LOW** |
| PC7 | `EXT_IN` | — |
| PC8 | `EXT_OUT` | — |
| PC9 | `RS_POWER_EN` | RS-485 transceiver supply |
| PC10 | `MEM_EN` | EEPROM chip select |
| PC11 | `MEM_HOLD` | held high |
| PC12 | `MEM_WP` | held high |
| PC13 | `TAMPER-RTC` | — |
| PC14/PC15 | `OSC32_IN/OUT` | 32.768 kHz for the RTC |

### Wired but unused

The board carries more than the firmware drives. None of these is a
defect; they are simply not reached yet:

- **`PA1`/`PA2`/`PA3` — an infrared port** on USART2. Confirmed unused.
- `PA0` `WKUP`.
- `PC4` `PHOTO_R`.
- `PC7`/`PC8` `EXT_IN`/`EXT_OUT`.

### Crystals

`ZQ2` is marked **8 MHz** on the schematic, which is what makes the
MCO output work: the L1's MCO prescaler divides by 1/2/4/8/16 only, so
the 8 MHz the TDC pair expects can come only from an 8 MHz crystal at
÷1. The legacy firmware declared 24 MHz and was wrong.

`ZQ1`/`ZQ4` and `PC14`/`PC15` carry the 32.768 kHz RTC oscillator.


## SPI Configuration

### SPI2 Settings
```
Mode:           Master
Baud Rate:      1 MHz (up to 10 MHz supported)
CPOL:           0 (Clock idle low)
CPHA:           0 (Sample on rising edge)
Data Width:     8-bit
Bit Order:      MSB first
```

### Command byte

Bit 6 is write, bit 7 is auto-increment, and the low six bits are the
register address. A plain read sends the **bare address** — there is no
read bit.

| Operation | Command byte | Used by |
|---|---|---|
| Write one register | `0x40 \| addr` | `write_register` |
| Read one register | `addr` | `read_register` |
| Read a block | `0x80 \| addr` | `read_bulk`, `read_results` |
| Write a block | `0xC0` | `load_config` |

`0xC0` is write-plus-auto-increment, not a read. An earlier version of
this document had it as the read command, which is the same inversion
the legacy driver in `src/hardware/tdc7200.rs` carries — following
either costs a debugging session, because a chip addressed this way
answers plausibly and wrongly rather than failing.

Multi-byte reads need the auto-increment bit or they return the same
register repeatedly instead of walking the block.

## Peripheral Configuration

### Clock Configuration
```
System Clock:    32 MHz (from PLL)
AHB Clock:       32 MHz
APB1 Clock:      16 MHz (SPI2)
APB2 Clock:      16 MHz (UART)
LSE:             32.768 kHz (RTC)
```

### Power Management
```
Mode:            Normal/Run
Voltage Scaling: Scale 1 (highest performance)
LVD:             Enabled at 2.7V
Supply:          USB or battery (3.3V)
```

## TDC1000 Integration

### Hardware Connections
```
TDC1000 Pin      MCU Pin      Net              Signal
──────────────────────────────────────────────────────────────
SCLK             PB13         SPI_CLK          SPI2 clock
MOSI             PB15         SPI_MOSI         data out
MISO             PB14         SPI_MISO         data in
CS               PB11         SPI_TDC1000_CS   chip select
RESET            PC6          TDC1000_RES      active HIGH, idles LOW
EN               PB10         TDC1000_EN       enable
VDD              3.3V                          power
VSS              GND                           ground
```

Note the chip select: **PB11**, not PB12. PB12 belongs to the TDC7200.
The two are adjacent and swapping them gives a bus where each chip
answers to the other's transactions.

### Transducer Connections
```
TX (Transmitter):  Ultrasonic transducer output
RX (Receiver):     Ultrasonic transducer input
BIAS:              Reference voltage (1.65V typical)
```

### Reset — active HIGH, idles LOW

```
1. Drive TDC1000_RES (PC6) HIGH   — asserts reset
2. Drive it LOW                    — releases it; LOW is the running level
```

**This is the opposite of what it looks like.** The net is `TDC1000_RES`
with no bar over it, and it is asserted high. An earlier version of this
document had the sequence inverted — low to reset, high to run — which
is what the firmware originally did, and the chip sat in reset for its
entire life.

The symptom is worth memorising because it points nowhere near the
reset line: **every TDC1000 register read returns `0x00`, while the
TDC7200 on the same SPI bus answers normally.** The TDC7200 has no reset
line, so the asymmetry between the two chips is the diagnostic. The
EEPROM, also on SPI2, reads fine throughout — which rules out the bus
and makes it tempting to suspect the SPI framing instead.

The firmware does this once at start-up rather than per measurement, as
the C++ does:

```rust
pub fn reset(&mut self) {
    self.res.set_high();
    self.res.set_low();   // running level is LOW
}
```

### Bring-up ordering

Reset, then enable, once at start-up. Per channel: select it, clear the
latched flags on both chips, settle ~2 ms, then trigger.

The INT wait is **level-triggered**, not edge-triggered. The C++ polls
`while (INT::IsSet())`, so a line already low when the wait begins
counts as done; waiting for a falling edge instead sits there until the
timeout expires and reports a failure that never happened.


## TDC7200 Integration

### Hardware Connections
```
TDC7200 Pin      MCU Pin      Net              Signal
──────────────────────────────────────────────────────────────
SCLK             PB13         SPI_CLK          SPI2 clock
MOSI             PB15         SPI_MOSI         data out
MISO             PB14         SPI_MISO         data in
CS               PB12         SPI_TDC7200_CS   chip select
INT              PB0          TDC7200_INT      measurement complete
EN               PB1          TDC7200_EN       enable
CLOCK            PA8          CLK_MCO_8MHZ     8 MHz reference from MCO
VDD              3.3V                          power
VSS              GND                           ground
```

**The TDC7200 has no reset line.** An earlier version of this document
listed one as "shared or separate" with the TDC1000's. There is no such
net on the schematic, and that absence is what makes the TDC1000 reset
bug diagnosable: with RESET stuck asserted, the TDC1000 returns `0x00`
while this chip keeps answering.

`INT` is not optional — the measurement path waits on it, level-triggered.

### Multi-Chip SPI (Shared Bus)
```
Shared on SPI2:
- SCLK (PB13), MOSI (PB15), MISO (PB14)

One chip select each:
- TDC1000  → PB11   (SPI_TDC1000_CS)
- TDC7200  → PB12   (SPI_TDC7200_CS)
- EEPROM   → PC10   (MEM_EN)
```

Three devices, not two: the 25LC1024 sits on the same bus. In the
firmware they share it through
`embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice` with a
`NoopRawMutex` — the executor is single-threaded, so nothing contends.

## Clock Distribution

### Reference Clock (optional)
```
External Oscillator: 1-10 MHz (optional)
Internal Oscillator: Supported (check datasheets)
Timing Accuracy:     ±1% typical
```

### RTC Integration
```
LSE (Low Speed External): 32.768 kHz
Provides:
- Time-of-day clock
- Wake-up timer
- Timestamp for flow measurements
```

## Power Supply Design

### Voltage Regulation
```
Input:           USB 5V or Battery
Regulator:       LDO (3.3V, 500mA minimum)
Decoupling:      100nF close to TDC
Bulk:            10µF for MCU
Output:          3.3V (±5%)
```

### Current Budget
```
STM32L151:       ~5-10 mA (run mode)
TDC1000/7200:    ~10-50 mA (measurement)
SPI Flash:       ~5 mA (write)
Total:           ~30-70 mA typical
```

### Battery Configuration (optional)
```
Type:            Li-Po 3.7V
Protection:      3.3V regulator + LVD
Charger:         USB charging circuit (optional)
```

## Signal Conditioning

### Ultrasonic Transducers
```
Type:            40 kHz piezoelectric transducers
Interface:
- TX Driver:     Capacitive drive circuit
- RX Path:       Low-noise amplifier + biasing
- Impedance:     ~70-80Ω typical
```

### Receive Path Filter
```
Type:            Bandpass IIR filter (40 kHz ±2 kHz)
Provides:        Noise rejection, harmonic suppression
```

### ADC Path (if used)
```
Sampling Rate:   200 kSps (≥5× signal frequency)
Resolution:      12-bit
Input:           0-3.3V (TDC output or analog frontend)
```

## GPIO Configuration

### Status Indicators
```
LED1 (PB4):      Power indicator (green)
LED2 (PB5):      Measurement active (red)
Button (PA11):   Reset/Mode select
```

### Debugging
```
Debug TX (PA9):  Serial output at 115200 bps
Debug RX (PA10): Serial input for commands
```

## Interrupt Configuration

### SPI Interrupts (optional DMA)
```
Interrupt Priority: High (5)
DMA Channel:       DMA1_Ch3 (SPI2 RX), DMA1_Ch4 (SPI2 TX)
Mode:              Standard (no FIFO)
```

### Timer Interrupts
```
TIM2: 1 kHz tick for timeouts and scheduling
TIM3: Measurement timing (if needed)
TIM4: Debug/monitor functions
```

## EEPROM/Flash Configuration

### External SPI Flash (optional)
```
Type:            25LC1024 (128 KB)
Interface:       SPI
Purpose:         Data logging, configuration storage
```

### Internal Flash
```
Size:            256 KB
Usage:           Firmware storage
Boot:            From internal flash
```

## Debugging & Testing

### Serial Debug Interface
```
Speed:           115200 bps
Data:            8 bits, 1 stop bit, no parity
Flow Control:    None
Commands:
  'R'            → Read all registers
  'T'            → Start measurement
  'C'            → Configuration
  'H'            → Help
```

### JTAG/SWD Interface (in-circuit debugging)
```
SWDIO (PA13):    Serial Wire Data
SWCLK (PA14):    Serial Wire Clock
GND:             Reference
```

### Test Points
```
TP1: 3.3V supply
TP2: GND reference
TP3: SPI SCLK
TP4: SPI MOSI
TP5: SPI MISO
TP6: TDC CS
```

## Measurement Signal Flow

### Time-of-Flight Measurement
```
Transducer TX
    ↓
Ultrasonic Pulse (40 kHz)
    ↓
Propagation through medium
    ↓
Transducer RX
    ↓
RX Amplifier
    ↓
40 kHz Bandpass Filter
    ↓
TDC1000/TDC7200 Input
    ↓
Time Measurement (ns resolution)
    ↓
MCU Processing
    ↓
Flow Calculation
    ↓
UART Output
```

## Calibration Procedures

### Temperature Calibration
```
Procedure:
1. Measure at known temperature (25°C reference)
2. Record system delay (offset)
3. Store in EEPROM
4. Apply at runtime: compensated_delay = offset + temp_correction
```

### Velocity Correction
```
Factors:
- Temperature: ±0.2% per °C (ultrasonic speed variation)
- Pressure: ±0.05% per 1% pressure change
- Medium composition: +2-5% for different gases/liquids
```

## Environmental Considerations

### Operating Conditions
```
Temperature:     -10°C to +50°C
Humidity:        10% to 90% RH (non-condensing)
Pressure:        0.95 to 1.05 atm
Supply Voltage:  3.0V to 3.6V
```

### EMI/EMC
```
Shielding:       SPI signals on twisted pair
Filtering:       Ferrite beads on supply
Layout:          Ground plane, short traces
Impedance:       50Ω for SPI at high speeds
```

## Troubleshooting

### SPI Communication Issues
```
Problem:         No response from TDC
Check:
1. CS timing (100ns setup, 100ns hold)
2. Clock polarity (should be 0)
3. Clock phase (should be 0)
4. Voltage levels (3.3V)
5. Reset sequence completed

Solution:        Reduce SPI speed, add pull-up on MISO
```

### Measurement Errors
```
Problem:         Unstable or out-of-range readings
Check:
1. TDC reset completed
2. Transducers properly connected
3. Receive signal amplitude
4. Temperature compensation applied
5. Calibration data valid

Solution:        Re-calibrate, check transducers
```

### Power Issues
```
Problem:         MCU resets unexpectedly
Check:
1. Supply voltage stability
2. Current draw (measure with ammeter)
3. Decoupling capacitors present
4. LVD threshold configured correctly
5. No short circuits

Solution:        Use power bank or better supply
```

## Design Checklist

- [ ] STM32L151 clocking configured
- [ ] SPI2 configured for TDC communication
- [ ] GPIO pins configured (reset, enable, CS)
- [ ] UART configured for debug output
- [ ] Power supply 3.3V regulated
- [ ] Decoupling capacitors placed
- [ ] TDC reset sequence tested
- [ ] SPI communication verified
- [ ] Transducers properly connected
- [ ] Receive path filtered and amplified
- [ ] Temperature sensor integrated
- [ ] EEPROM for calibration data
- [ ] Debug interface functional
- [ ] Interrupts configured if using DMA
- [ ] EMI/EMC filtering applied

## Bill of Materials (BoM)

### MCU & Core Components
- STM32L151CBU6 (MCU)
- TDC1000 or TDC7200 (Time-to-Digital Converter)
- 25LC1024 (EEPROM, optional)

### Power
- LDO 3.3V 500mA regulator
- 100nF decoupling capacitors (×3)
- 10µF bulk capacitor
- Ferrite beads for filtering

### Passive Components
- 47kΩ pull-up resistors (SPI lines)
- 0.1µF for clock filtering
- 1µF for power filtering

### Transducers
- 40 kHz ultrasonic transmitter
- 40 kHz ultrasonic receiver
- RX amplifier circuit

### Connectors & Debug
- USB micro connector (power/debug)
- SWD programming connector
- Flow sensor output connector

## Related Documentation

- [TDC1000 Register Map](TDC1000_REGISTER_MAP.md)
- [TDC7200 Register Map](TDC7200_REGISTER_MAP.md)
- [Ultrasonic Flow Measurement](ULTRASONIC_FLOW.md)
- STM32L151 Reference Manual: ARM Cortex-M3
- TDC1000/TDC7200 Datasheets: Texas Instruments

