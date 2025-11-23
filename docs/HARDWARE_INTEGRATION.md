# Hardware Integration Guide

## Overview

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

### STM32L151 Pinout

#### UART Pins (Debug/Serial)
```
PA9  (TX) → Debug TX
PA10 (RX) → Debug RX
Baud Rate: 115200 bps
```

#### SPI Interface (TDC Communication)
```
PB13 (SCLK)  → SPI2 Clock
PB14 (MISO)  → SPI2 Data In
PB15 (MOSI)  → SPI2 Data Out
PB12 (CS)    → Chip Select (GPIO Output)
```

#### Control Pins
```
PA4  → TDC1000 Reset
PA5  → TDC1000 Enable
PB4  → Status LED
PB5  → Debug LED
```

#### Power Pins
```
VDD  → +3.3V
VSS  → GND
```

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

### SPI Frame Format
```
WRITE: ┌──┬─────┬──────┐
       │CS│Addr │Data  │
       └──┴─────┴──────┘
         0x40   0xFF

READ:  ┌──┬─────┬──────┐
       │CS│Addr │Data  │
       └──┴─────┴──────┘
         0xC0   Read
```

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
TDC1000 Pin      MCU Pin      Signal
─────────────────────────────────────
SCLK             PB13         SPI Clock
MOSI             PB15         Data Out
MISO             PB14         Data In
CS_N             PB12         Chip Select
RST_N            PA4          Reset
EN               PA5          Enable
VDD              3.3V         Power
VSS              GND          Ground
```

### Transducer Connections
```
TX (Transmitter):  Ultrasonic transducer output
RX (Receiver):     Ultrasonic transducer input
BIAS:              Reference voltage (1.65V typical)
```

### Reset Procedure
```
1. Pull RST_N low for 100µs
2. Wait 10ms for stabilization
3. RST_N high (pulled by internal pullup)
4. Device ready for communication
```

### Initialization Code
```rust
// Configure SPI
let spi = hal::spi::Spi::spi2(
    pac.SPI2,
    (clk_pin, miso_pin, mosi_pin),
    hal::spi::Mode {
        polarity: hal::spi::Polarity::IdleLow,
        phase: hal::spi::Phase::CaptureOnFirstChange,
    },
    1.MHz(),
    clk,
);

// Configure control pins
let mut reset = reset_pin.into_push_pull_output();
let mut enable = enable_pin.into_push_pull_output();
let cs = cs_pin.into_push_pull_output();

// Create TDC1000 instance
let mut tdc = TDC1000::new(spi, cs, reset, enable);

// Reset device
tdc.reset()?;

// Configure measurements
let config = Config0::default();
tdc.set_config0(config)?;
```

## TDC7200 Integration

### Hardware Connections
```
TDC7200 Pin      MCU Pin      Signal
────────────────────────────────────
SCLK             PB13         SPI Clock
MOSI             PB15         Data Out
MISO             PB14         Data In
CS_N             PB12         Chip Select (GPIO with multiplexing)
INT_N            PA6          Interrupt (optional)
RST_N            PA4          Reset (shared or separate)
VDD              3.3V         Power
VSS              GND          Ground
```

### Multi-Chip SPI (Shared Bus)
```
Shared:
- SCLK (PB13)
- MOSI (PB15)
- MISO (PB14)

Separate:
- TDC1000 CS → PB12
- TDC7200 CS → PA7 (additional GPIO)
- Reset → PA4 (shared)
```

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

