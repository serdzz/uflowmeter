# uFlowmeter Architecture

## System Overview

The uFlowmeter is an embedded ultrasonic flow measurement system built on STM32L151 microcontroller using Rust. It measures fluid flow using transit-time-of-flight (TOF) method with TDC1000/TDC7200 chipset.

### Hardware Components
- **MCU**: STM32L151 (Cortex-M3)
- **AFE**: TDC1000 analog front-end for ultrasonic transducer excitation
- **TDC**: TDC7200 time-to-digital converter for precise TOF measurement
- **Display**: LCD 1602 character display
- **Storage**: AT24Cxx EEPROM for calibration data
- **Transducers**: Ultrasonic transducers for bidirectional flow measurement

### System Diagram
```
┌─────────────────────────────────────────────────────┐
│              STM32L151 Application                  │
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

## Dual-target design

This project is an embedded STM32 application that needs to work both as:
1. **Binary (no_std embedded)**: Runs on embedded hardware with `thumbv7m-none-eabi` target
2. **Library (std)**: Provides testable modules for host platform

## Module Structure

### `src/main.rs`
- Binary entry point
- Uses embedded features (embassy executor, HAL, etc.)
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

- Tests cannot directly test embedded-specific code (embassy tasks, HAL)
- Only core logic can be tested this way
- For full integration tests, physical hardware or simulator needed

---

## Hardware Driver Architecture

Both TDC drivers are generic over `embedded_hal::spi::SpiDevice`, and in
`main.rs` that is `embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice`
over SPI2 — the bus is shared with the EEPROM, and each device owns its
own chip-select `Output`. `main.rs` defines `Tdc1000Dev` and `Tdc7200Dev`
as aliases for the fully-specified types.

### TDC1000 Driver (`src/drivers/tdc1000.rs`)

Analog front end: drives the transducers and switches channels.

```rust
pub struct Tdc1000<'d, D> { ... }

impl<'d, D: SpiDevice> Tdc1000<'d, D> {
    pub fn new(spi: D, en: Output<'d>, res: Output<'d>) -> Self
    pub fn reset(&mut self)
    pub fn power_on(&mut self)
    pub fn power_off(&mut self)
    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error>
    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error>
    pub fn load_config(&mut self, regs: &[u8; CONFIG_REG_COUNT]) -> Result<(), D::Error>
    pub fn set_channel(&mut self, ch2: bool) -> Result<(), D::Error>
    pub fn clear_error_flags(&mut self) -> Result<(), D::Error>
    pub fn error_flags(&mut self) -> Result<u8, D::Error>
}
```

`CONFIG_REG_COUNT` is 10 — registers `0x00..0x09`, loaded as one block
from `Options::tdc1000_regs`.

**`reset()` leaves the line LOW, and that is not a detail.** RESET is
active high; low is the running level. Holding it high keeps the chip in
reset for its whole life, and the only symptom is that every register
read returns `0x00` while the TDC7200 — which has no reset line —
answers normally. That asymmetry is the diagnostic.

**Channel select is bit 2 (`0x04`) of CONFIG_2**, not bit 0. With the
wrong bit both "channels" measure the same acoustic path and the flow
comes out at zero without any error being reported.

### TDC7200 Driver (`src/drivers/tdc7200.rs`)

Time-to-digital converter: measures the interval the flow calculation
needs.

```rust
pub struct Tdc7200<'d, D> { ... }

impl<'d, D: SpiDevice> Tdc7200<'d, D> {
    pub fn new(...) -> Self
    pub fn power_on(&mut self)
    pub fn power_off(&mut self)
    pub fn read_register(&mut self, address: u8) -> Result<u8, D::Error>
    pub fn write_register(&mut self, address: u8, value: u8) -> Result<(), D::Error>
    pub fn read_bulk(...) -> Result<(), D::Error>
    pub fn load_config(&mut self, regs: &[u8; CONFIG_REG_COUNT]) -> Result<(), D::Error>
    pub fn clear_int_flags(&mut self) -> Result<(), D::Error>
    pub fn start_measurement(&mut self) -> Result<(), D::Error>
    pub fn read_results(&mut self) -> Result<[u8; RESULT_BLOCK_LEN], D::Error>
    pub fn stop_numbers(&self) -> usize
}
```

`read_results` returns the whole 39-byte block from `0x10`; decoding it
into picoseconds is `tdc_lib::decode_tof`, which is pure and tested on
the host.

### Command byte encoding (both chips)

Bit 6 is write, bit 7 is auto-increment, and a read sends the bare
address. Multi-byte reads need auto-increment or they return the same
register repeatedly.

The legacy driver in `src/hardware/tdc7200.rs` has read and write
inverted relative to both the datasheet and the C++ firmware. It is
archived, not a reference.


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

## Sharing SPI2

Three devices sit on SPI2: TDC1000, TDC7200 and the 25LC1024 EEPROM.
They share it through
`embassy_embedded_hal::shared_bus::blocking::spi::SpiDevice`, with a
`NoopRawMutex` — the executor is single-threaded, so no real locking is
needed — and one chip-select `Output` per device.

The RTIC firmware used `shared_bus_rtic::SharedBus`, whose `BusProxy`
did not satisfy the `E: From<PinError>` bound the old TDC drivers
carried. That produced a rule about only calling concrete TDC methods
from monomorphised code in `main.rs`, and a `new_simple` constructor to
sidestep the bound. None of it applies now: the crate is gone, the
drivers are generic over `SpiDevice` and carry no pin-error bound, and
`new_simple` does not exist.


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
- STM32L151 Reference Manual: [STMicroelectronics](https://www.st.com/en/microcontrollers-microprocessors/stm32f103.html)
