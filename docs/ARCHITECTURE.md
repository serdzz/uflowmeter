# uFlowmeter Architecture

## System Overview

The uFlowmeter is an embedded ultrasonic flow measurement system built on STM32F103 microcontroller using Rust. It measures fluid flow using transit-time-of-flight (TOF) method with TDC1000/TDC7200 chipset.

### Hardware Components
- **MCU**: STM32F103 (Cortex-M3)
- **AFE**: TDC1000 analog front-end for ultrasonic transducer excitation
- **TDC**: TDC7200 time-to-digital converter for precise TOF measurement
- **Display**: LCD 1602 character display
- **Storage**: AT24Cxx EEPROM for calibration data
- **Transducers**: Ultrasonic transducers for bidirectional flow measurement

### System Diagram
```
┌─────────────────────────────────────────────────────┐
│              STM32F103 Application                  │
│  ┌──────────────────────────────────────────────┐  │
│  │           Main Control Loop                  │  │
│  │  - Bidirectional TOF measurement             │  │
│  │  - Flow velocity calculation                 │  │
│  │  - Calibration management                    │  │
│  │  - LCD display & logging                     │  │
│  └──────────────────────────────────────────────┘  │
│         ↓           ↓           ↓          ↓        │
│   ┌─────────┐ ┌─────────┐ ┌────────┐ ┌────────┐   │
│   │ TDC1000 │ │ TDC7200 │ │  LCD   │ │ EEPROM │   │
│   │ Driver  │ │ Driver  │ │ Driver │ │Storage │   │
│   └─────────┘ └─────────┘ └────────┘ └────────┘   │
│       ↓           ↓            ↓          ↓         │
└───────────────────────────────────────────────────┬─┘
        │           │            │          │         
        │           │            │          │         
    ┌───────┐   ┌───────┐   ┌──────┐  ┌────────┐   
    │TDC1000│   │TDC7200│   │ LCD  │  │EEPROM  │   
    │  AFE  │   │  TDC  │   │1602  │  │AT24Cxx │   
    └───────┘   └───────┘   └──────┘  └────────┘   
        ↓           ↑                                
    Ultrasonic  ────┘                                
    Transducers    TOF measurement                   
```

---

## Build Configuration

This project is an embedded STM32 application that needs to work both as:
1. **Binary (no_std embedded)**: Runs on embedded hardware with `thumbv7m-none-eabi` target
2. **Library (std)**: Provides testable modules for host platform

## Module Structure

### `src/main.rs`
- Binary entry point
- Uses embedded features (RTIC, HAL, etc.)
- Compiled only for `thumbv7m-none-eabi` target

### `src/lib.rs`
- Library root
- Re-exports testable modules
- Conditional compilation: `#![cfg_attr(not(test), no_std)]`
- Tests use host target, library code uses `no_std` when embedded

### `src/history.rs`
- Original embedded module with full HAL dependencies
- Not directly testable due to HAL requirements

### `src/history_lib.rs`
- Standalone testable version of history module
- No HAL dependencies
- Contains core logic for `RingStorage` and `ServiceData`
- Used by both library and embedded builds

### `src/history_lib_tests.rs`
- Unit tests for `history_lib` module
- Compiled only for host target (`#[cfg(test)]`)
- 11 comprehensive test cases

## Build Configuration

### `.cargo/config.toml`
```toml
[build]
target = "thumbv7m-none-eabi"  # Default for binary builds

[target.thumbv7m-none-eabi]
rustflags = ["-C", "link-arg=-Tlink.x", "-C", "link-arg=-Tdefmt.x"]
```

This default target is used for binary builds but **not** for library tests.

## Building and Testing

### Build Release Binary
```bash
cargo build --release
# Uses thumbv7m-none-eabi target, creates optimized embedded binary
```

### Run Tests
```bash
# Using Makefile (recommended)
make test

# Or manually temporarily disable embedded target
sed -i.bak '/^target = /d' .cargo/config.toml && \
cargo test --lib --release && \
mv .cargo/config.toml.bak .cargo/config.toml
```

The Makefile handles target switching automatically. When running tests, Cargo uses the host platform target instead of the embedded target, allowing tests to link against std library.

## Why This Approach?

1. **Embedded-first**: Binary uses no_std with minimal overhead
2. **Testable**: Core logic extracted to a no-std-compatible module that can be tested on host
3. **No duplication**: Tests reuse the same `history_lib` code that embedded build uses
4. **Flexible**: Easy to add more testable modules following same pattern

## Adding New Tests

1. Add test functions to `src/history_lib_tests.rs`
2. Keep `src/history_lib.rs` public methods that tests need
3. Run tests with the command above

## Limitations

- Tests cannot directly test embedded-specific code (RTIC, HAL)
- Only core logic can be tested this way
- For full integration tests, physical hardware or simulator needed

---

## Hardware Driver Architecture

### TDC1000 Driver (`src/hardware/tdc1000.rs`)

The TDC1000 is an analog front-end (AFE) that drives ultrasonic transducers and manages channel switching.

**Key Features:**
- SPI interface for configuration (8 registers: 0x00-0x07)
- GPIO control for enable and trigger signals
- Channel switching (CH1/CH2) for bidirectional measurement
- Error flag monitoring
- Hardware reset capability

**Public API:**
```rust
pub struct Tdc1000<SPI, ENABLE, TRIGGER> { ... }

impl<SPI, ENABLE, TRIGGER, E> Tdc1000<SPI, ENABLE, TRIGGER>
where
    SPI: spi::Write<u8, Error = E> + spi::Transfer<u8, Error = E>,
    ENABLE: OutputPin,
    TRIGGER: OutputPin,
{
    pub fn new(spi: SPI, enable: ENABLE, trigger: TRIGGER) -> Result<Self, E>
    pub fn reset(&mut self) -> Result<(), PinError>
    pub fn set_channel(&mut self, ch2: bool) -> Result<(), E>
    pub fn get_error_flags(&mut self) -> Result<u8, E>
    pub fn set_config0(&mut self, value: u8) -> Result<(), E>
    // ... other register setters
}
```

**Example Usage:**
```rust
let mut tdc1000 = Tdc1000::new(spi2_bus.acquire(), pb10, pb11)?;
tdc1000.reset()?;
tdc1000.set_channel(false)?;  // Select CH1 (downstream)
let errors = tdc1000.get_error_flags()?;
```

### TDC7200 Driver (`src/hardware/tdc7200.rs`)

The TDC7200 is a time-to-digital converter that measures precise time intervals for TOF calculation.

**Key Features:**
- SPI interface for configuration and data readout
- Multiple measurement ranges (250ns to 8ms)
- Interrupt-based measurement completion
- Supports multi-cycle averaging

**Public API:**
```rust
pub struct Tdc7200<SPI, ENABLE, CS, INTB> { ... }

impl<SPI, ENABLE, CS, INTB> Tdc7200<SPI, ENABLE, CS, INTB> {
    // Simple constructor without trait bounds for use in SharedBus context
    pub fn new_simple(spi: SPI, enable: ENABLE, cs: CS, intb: INTB) -> Self
}
```

**Note on Trait Bounds:**
The original `new()` constructor has complex trait bounds that conflict with SharedBus. The `new_simple()` constructor bypasses these for use in examples. See "SharedBus Patterns" section for details.

---

## Flow Measurement Algorithm

### Transit-Time Principle

Ultrasonic flow measurement uses the difference in sound propagation time along and against the flow:

```
Flow →
  ↓
[T1]────────L────────→[T2]  Downstream: t_down = L / (c + v)
  ↑                    │
  └────────L───────────┘     Upstream:   t_up = L / (c - v)
         ← Flow
```

**Key Formulas:**
- `Δt = t_up - t_down` (time difference)
- `v = (L/2) × (Δt / (t_up × t_down))` (flow velocity)
- `Q = v × A` (volume flow rate)

Where:
- `L` = transducer separation distance (mm)
- `c` = speed of sound in fluid (m/s)
- `v` = flow velocity (m/s)
- `A` = pipe cross-sectional area (mm²)

### Measurement Workflow

1. **Downstream Measurement (CH1)**
   ```rust
   tdc1000.set_channel(false)?;  // Select CH1
   // Trigger TDC7200 measurement
   let t_down = tdc7200.read_time()?;  // Read TOF
   ```

2. **Upstream Measurement (CH2)**
   ```rust
   tdc1000.set_channel(true)?;   // Select CH2
   // Trigger TDC7200 measurement
   let t_up = tdc7200.read_time()?;    // Read TOF
   ```

3. **Calculate Flow**
   ```rust
   let delta_t = t_up - t_down;
   let delta_t_corrected = delta_t - calibration.zero_offset;
   let velocity = (k * delta_t_corrected) / (t_down * t_up);
   let flow_rate = velocity * pipe_area;
   ```

### Calibration

Calibration data is stored in EEPROM using the `Options` struct:

```rust
pub struct Options {
    zero1: i16,      // Zero offset for CH1 (nanoseconds)
    zero2: i16,      // Zero offset for CH2 (nanoseconds)
    v11: i16,        // Velocity calibration factor CH1
    v21: i16,        // Velocity calibration factor CH2
    uptime: u32,     // System uptime counter
    // ... other fields
}
```

**Calibration Application:**
- `zero1/zero2`: Subtract from measured Δt to compensate for transducer asymmetry
- `v11/v21`: Scaling factors for velocity calculation
- Values loaded from EEPROM at startup, applied to each measurement

---

## SharedBus Pattern and Limitations

### Problem

Multiple SPI devices (TDC1000, TDC7200) need to share a single SPI peripheral. The `shared-bus` crate provides `BusManager` for this, but introduces trait bound challenges.

### SharedBus Usage

```rust
use shared_bus::BusManagerSimple;

type Spi2BusManager = BusManagerSimple<Spi<SPI2, ...>>;

// Create bus manager
let spi2_manager: Spi2BusManager = BusManagerSimple::new(spi2);

// Acquire proxies for each device
let tdc1000 = Tdc1000::new(spi2_manager.acquire(), enable_pin, trigger_pin)?;
let tdc7200 = Tdc7200::new_simple(spi2_manager.acquire(), enable_pin, cs_pin, intb_pin);
```

### Trait Bound Limitations

**Core Issue:**
TDC driver methods have trait bounds like:
```rust
where
    SPI: spi::Write<u8, Error = E>,
    ENABLE: OutputPin<Error = PinError>,
    E: From<PinError>  // ← This bound is NOT satisfied by SharedBus
```

SharedBus's `BusProxy` wraps the SPI error type, but doesn't implement `From<PinError>`, so the error conversion trait bound fails.

**Workarounds:**

1. **Call methods in `main()` where concrete types are known:**
   ```rust
   // This WORKS in main.rs:
   let errors = tdc1000.get_error_flags()?;
   tdc1000.set_channel(false)?;
   ```

2. **Use `new_simple()` for TDC7200:**
   ```rust
   // Instead of new() with trait bounds:
   let tdc7200 = Tdc7200::new_simple(spi, enable, cs, intb);
   ```

3. **Avoid generic functions that call TDC methods:**
   ```rust
   // This FAILS:
   fn measure<S, E, T>(tdc: &mut Tdc1000<S, E, T>) {
       tdc.set_channel(false)?;  // ← Trait bound not satisfied
   }
   ```

**Why It Works in main.rs:**
- Concrete types known at compile time
- Compiler can monomorphize the code
- No generic type parameters
- See `src/main.rs:242` for working example with `set_config0()`

**Documentation:**
See `examples/ultrasonic_flow_example.rs` header comments for detailed explanation and examples.

---

## Examples

### `examples/ultrasonic_flow_example.rs`

Comprehensive example demonstrating:
1. Hardware initialization (SPI, GPIO, LCD, EEPROM)
2. SharedBus setup for TDC1000/TDC7200
3. Real bidirectional TOF measurement:
   - Channel switching (CH1 → CH2)
   - Time-of-flight capture
   - Delta-t calculation
4. Flow calculation with calibration:
   - Zero offset correction
   - Velocity computation
   - Volume flow rate (mL/s)
   - Flow direction detection
5. LCD display and defmt logging

**Hardware Setup:**
- TDC1000: SPI2, PB10 (enable), PB11 (trigger)
- TDC7200: SPI2, PB12 (enable), PB13 (CS), PB14 (INTB)
- LCD 1602: I2C1
- EEPROM: I2C1

**Running:**
```bash
cargo build --release --example ultrasonic_flow_example
cargo flash --release --example ultrasonic_flow_example
```

### Example Output
```
INFO  Calibration loaded
INFO  === Bidirectional TOF Measurement ===
INFO  Measuring downstream (CH1)...
INFO  Downstream TOF: 45123 ns
INFO  Measuring upstream (CH2)...
INFO  Upstream TOF: 47234 ns
INFO  Time difference: 2111 ns
INFO  === Flow Calculation with Calibration ===
INFO  Zero offset: 50, Corrected delta_t: 2061
INFO  Flow velocity: 1234 mm/s (v11=1000)
INFO  Volume flow rate: 218 mL/s (area=177mm²)
INFO  Flow direction: Forward (positive delta_t)
INFO  Updated uptime: 12345 seconds
```

---

## Future Enhancements

1. **Real TDC7200 Integration**: Add actual TOF measurement methods (currently placeholders)
2. **Interrupt-driven Measurement**: Use TDC7200 INTB signal for non-blocking operation
3. **Multi-cycle Averaging**: Implement averaging across multiple measurements for noise reduction
4. **Temperature Compensation**: Read temperature sensor and adjust speed of sound
5. **Flow Totalizer**: Accumulate total volume over time, persist to EEPROM
6. **Error Detection**: Monitor TDC error flags and handle timeout/overflow conditions
7. **Modbus Interface**: Add Modbus RTU/TCP support for remote monitoring
8. **Web UI**: Expose flow data via HTTP server (if MCU supports networking)

---

## References

- TDC1000 Datasheet: [TI Product Page](https://www.ti.com/product/TDC1000)
- TDC7200 Datasheet: [TI Product Page](https://www.ti.com/product/TDC7200)
- Transit-Time Flow Measurement: [Wikipedia](https://en.wikipedia.org/wiki/Ultrasonic_flow_meter)
- STM32F103 Reference Manual: [STMicroelectronics](https://www.st.com/en/microcontrollers-microprocessors/stm32f103.html)
