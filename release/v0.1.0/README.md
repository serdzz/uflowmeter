# uFlowmeter v0.1.0 - Production Release

## Release Information
- **Version**: 0.1.0
- **Target**: STM32L151 (Cortex-M3)
- **Build Date**: 2025-11-21
- **Build Profile**: Release (optimized for size with LTO)

## Memory Usage
```
   text    data     bss     dec     hex filename
  61012    4356    2496   67864   10918 uflowmeter.elf
```

## Release Files
- `uflowmeter.elf` - ELF binary with debug symbols
- `uflowmeter.bin` - Raw binary for flashing
- `uflowmeter.hex` - Intel HEX format
- `memory_usage.txt` - Detailed memory usage report
- `changelog.txt` - Recent changes

## Build Configuration
- **Optimization**: `opt-level = 'z'` (optimize for size)
- **LTO**: Fat (full link-time optimization)
- **Codegen Units**: 1 (better optimization)
- **Debug Info**: Included (level 2)

## Flashing Instructions

### Using STM32CubeProgrammer (default)
```bash
STM32_Programmer_CLI -c port=SWD -w uflowmeter.bin 0x08000000 -v -rst
```

### Using probe-rs
```bash
probe-rs run --chip STM32L151CBxxA uflowmeter.elf
```

### Using OpenOCD
```bash
openocd -f interface/stlink.cfg -f target/stm32l1.cfg \
  -c "program uflowmeter.elf verify reset exit"
```

## Features
- Ultrasonic flow measurement using TDC7200
- HD44780 LCD display support
- EEPROM data storage (25LC1024)
- RTIC-based real-time operating system
- Low power mode support
- Hardware abstraction layer for STM32L1xx

## System Requirements
- STM32L151CBxxA microcontroller
- 128KB Flash, 32KB RAM
- SWD debugger for programming

## Verification
All release artifacts passed:
- ✓ Cargo build (release profile)
- ✓ Clippy lints (zero warnings)
- ✓ Memory constraints verified
- ✓ Git tagged: v0.1.0

## Notes
- This is a production-ready release optimized for embedded deployment
- Debug symbols included for post-mortem analysis
- All dependencies verified and tested
