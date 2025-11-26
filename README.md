# uFlowmeter

Embedded ultrasonic flow measurement system based on STM32L151 microcontroller.

## Description

uFlowmeter is a transit-time-of-flight liquid flow measurement system implemented in Rust for the STM32 platform. The system uses TDC1000/TDC7200 chipsets for precise ultrasonic signal transit time measurement and flow velocity calculation.

### Key Features

- **Precise Measurement**: Using TDC7200 for high-precision time measurement
- **Bidirectional Measurement**: Support for measurement in both flow directions
- **Data Storage**: Measurement history (hourly, daily, monthly) in non-volatile memory
- **Interface**: LCD 1602 display with keyboard for control
- **Modbus RTU**: Protocol for remote monitoring and configuration
- **Power Management**: Low power consumption with sleep mode support
- **Testability**: Modular architecture with unit tests for core logic

## Hardware Platform

- **MCU**: STM32L151C6 (Cortex-M3, 256KB Flash, 32KB RAM)
- **AFE**: TDC1000 — analog front-end for ultrasonic transducer control
- **TDC**: TDC7200 — precision time-to-digital converter
- **Display**: LCD 1602 character display
- **Memory**: Microchip 25LC1024 (128KB EEPROM) for history and configuration storage
- **Interfaces**: 
  - SPI for communication with TDC chipsets and EEPROM
  - UART for Modbus RTU
  - GPIO for keyboard and power management

## Project Structure

```
uflowmeter/
├── src/
│   ├── main.rs              # Main application (RTIC)
│   ├── lib.rs               # Library interface for testing
│   ├── apps.rs              # Application logic (measurement, settings)
│   ├── ui.rs                # UI framework
│   ├── gui/                 # GUI widgets (Label, Edit, etc.)
│   ├── hardware/            # Hardware drivers
│   │   ├── tdc1000.rs       # TDC1000 driver
│   │   ├── tdc7200.rs       # TDC7200 driver
│   │   ├── hd44780.rs       # LCD driver
│   │   └── pins.rs          # Pin configuration
│   ├── history.rs           # History system (embedded)
│   ├── history_lib.rs       # History system (testable)
│   ├── modbus.rs            # Modbus RTU implementation
│   ├── modbus_handler.rs    # Modbus request handler
│   └── measurement/         # Flow measurement algorithms
├── examples/                # Usage examples
├── docs/                    # Documentation
│   ├── ARCHITECTURE.md      # System architecture
│   ├── MODBUS_MAP.md        # Modbus register map
│   ├── TESTING.md           # Testing guide
│   └── ...
├── tests/                   # Integration tests
├── Cargo.toml               # Dependency configuration
├── memory.x                 # Linker memory map
├── .embed.toml              # cargo-embed configuration
└── Makefile                 # Build and test commands
```

## Building and Flashing

### Requirements

- Rust toolchain (rustup recommended)
- `thumbv7m-none-eabi` target
- cargo-embed or probe-rs for flashing

```bash
# Install target
rustup target add thumbv7m-none-eabi

# Install cargo-embed (optional)
cargo install cargo-embed
```

### Building

```bash
# Release build
make build
# or
cargo build --release

# Debug build
cargo build
```

### Flashing

```bash
# Using cargo-embed
cargo embed --release

# Or using probe-rs directly
probe-rs run --chip STM32L151C6 target/thumbv7m-none-eabi/release/uflowmeter
```

## Testing

The project supports testing on host platform thanks to its modular architecture.

```bash
# Run all tests
make test

# Run Modbus tests
make test-modbus

# Run tests with verbose output
make test-modbus-verbose

# Run clippy
make clippy
```

### UI Examples

```bash
# Run UI examples on host platform
make ui-examples
```

For more details on testing, see [docs/TESTING.md](docs/TESTING.md).

## Architecture

The project uses RTIC (Real-Time Interrupt-driven Concurrency) for real-time task management:

- **Measurement Task**: Periodic flow measurement
- **UI Task**: Keyboard handling and display updates
- **Modbus Task**: UART request processing
- **History**: Automatic data storage to EEPROM

For detailed architecture documentation, see:
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/UI_ARCHITECTURE.md](docs/UI_ARCHITECTURE.md)
- [docs/HISTORY_SYSTEM.md](docs/HISTORY_SYSTEM.md)

## Modbus Interface

The device supports Modbus RTU for remote monitoring:

- **Baud Rate**: 9600 baud, 8N1
- **Device Address**: Configurable (default 1)
- **Functions**: 0x03 (Read Holding Registers), 0x06 (Write Single Register), 0x10 (Write Multiple Registers)

See register map in [docs/MODBUS_MAP.md](docs/MODBUS_MAP.md).

## Examples

Available examples in the `examples/` directory:

- `display_example.rs` — LCD display usage example
- `ui_examples.rs` — UI widgets demonstration (for host platform)
- `ui_examples_embedded.rs` — UI widgets for embedded system
- `options_example.rs` — Working with system settings
- `power_management_example.rs` — Power management
- `ultrasonic_flow_example.rs` — Flow measurement example

See also [examples/README.md](examples/README.md).

## Debugging

The project uses defmt for logging via RTT (Real-Time Transfer):

```bash
# Run with RTT logging
cargo embed --release
```

Logs will be displayed in the terminal with timestamps.

## Dependencies

Main dependencies:

- `stm32l1xx-hal` — HAL for STM32L1xx
- `cortex-m-rtic` — RTIC framework
- `microchip-eeprom-25lcxx` — EEPROM driver
- `embedded-hal` — Embedded HAL abstractions
- `time` — Date/time handling
- `defmt` — Efficient logging for embedded systems

See full list in [Cargo.toml](Cargo.toml).

## Documentation

Complete documentation is available in the `docs/` directory:

- [ARCHITECTURE.md](docs/ARCHITECTURE.md) — System architecture
- [HARDWARE_INTEGRATION.md](docs/HARDWARE_INTEGRATION.md) — Hardware integration
- [TESTING.md](docs/TESTING.md) — Testing guide
- [MODBUS_MAP.md](docs/MODBUS_MAP.md) — Modbus register map
- [TDC1000_REGISTER_MAP.md](docs/TDC1000_REGISTER_MAP.md) — TDC1000 registers
- [TDC7200_REGISTER_MAP.md](docs/TDC7200_REGISTER_MAP.md) — TDC7200 registers

## License

MIT OR Apache-2.0

## Author

Sergej Lepin <sergej.lepin@gmail.com>
