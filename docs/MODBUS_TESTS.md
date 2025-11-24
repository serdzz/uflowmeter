# Modbus Tests Documentation

## Overview

The Modbus RTU implementation includes comprehensive unit tests in `src/modbus.rs`.

## Test Coverage

### CRC Calculation Tests
- `test_crc_calculation()` - Validates CRC-16 (Modbus) calculation against spec examples
- `test_crc_another_example()` - Additional CRC validation

### Frame Parsing Tests
- `test_parse_read_holding_registers()` - Function 0x03 parsing
- `test_parse_read_input_registers()` - Function 0x04 parsing  
- `test_parse_write_single_register()` - Function 0x06 parsing
- `test_parse_write_multiple_registers()` - Function 0x10 parsing
- `test_parse_broadcast_address()` - Broadcast (address 0) handling

### Error Handling Tests
- `test_parse_invalid_crc()` - CRC validation
- `test_parse_invalid_length()` - Frame length validation
- `test_parse_wrong_slave_address()` - Address filtering

### Response Building Tests
- `test_build_response()` - Normal response construction
- `test_build_exception_response()` - Exception response construction

### Configuration Tests
- `test_slave_address_get_set()` - Slave address configuration
- `test_function_code_from_u8()` - Function code conversion

## Running Tests

### Note on Test Execution

The project is configured for embedded target (`thumbv7m-none-eabi`) which does not support standard test framework. Tests are included in the source code but cannot be executed with `cargo test` on the embedded target.

### Test Validation Methods

#### Option 1: Code Review
Review test implementations in `src/modbus.rs` lines 280-438 to validate correctness.

#### Option 2: Integration Testing
Tests can be validated through integration with actual hardware or simulator.

#### Option 3: Documentation Tests
Tests serve as executable documentation showing correct usage:

```rust
use uflowmeter::modbus::ModbusRtu;

// Example: Parse a Modbus frame
let modbus = ModbusRtu::new(0x01);
let frame = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC5, 0xCD];
let request = modbus.parse_request(&frame).unwrap();

assert_eq!(request.slave_address, 0x01);
assert_eq!(request.start_address, 0x0000);
assert_eq!(request.quantity, 0x000A);
```

## Test Scenarios

### 1. Read Holding Registers (0x03)
```
Request:  [01 03 00 00 00 0A C5 CD]
- Slave: 0x01
- Function: Read Holding Registers
- Start: 0x0000
- Quantity: 10 registers
- CRC: 0xC5CD
```

### 2. Read Input Registers (0x04)
```
Request:  [01 04 00 00 00 04 F1 CC]
- Slave: 0x01
- Function: Read Input Registers  
- Start: 0x0000
- Quantity: 4 registers
- CRC: 0xF1CC
```

### 3. Write Single Register (0x06)
```
Request:  [01 06 00 05 12 34 D8 5E]
- Slave: 0x01
- Function: Write Single Register
- Address: 0x0005
- Value: 0x1234
- CRC: 0xD85E
```

### 4. Write Multiple Registers (0x10)
```
Request:  [01 10 00 01 00 02 04 00 0A 01 02 C6 F0]
- Slave: 0x01
- Function: Write Multiple Registers
- Start: 0x0001
- Quantity: 2
- Byte Count: 4
- Data: [00 0A 01 02]
- CRC: 0xC6F0
```

### 5. Exception Response
```
Request:  [01 03 FF FF 00 01 50 B9]  // Invalid address
Response: [01 83 02 XX XX]
- Slave: 0x01
- Function: 0x83 (0x03 | 0x80 - error bit)
- Exception: 0x02 (Illegal Data Address)
- CRC: varies
```

## CRC Calculation Examples

### Example 1
```
Data: [01 03 00 00 00 0A]
CRC:  0xC5CD
```

### Example 2  
```
Data: [11 03 00 6B 00 03]
CRC:  0x7687
```

## Test Implementation Details

### CRC Algorithm
The implementation uses standard Modbus CRC-16 with polynomial 0xA001:
- Initial value: 0xFFFF
- Process each byte with XOR
- Apply polynomial on LSB set
- Result in little-endian format

### Frame Structure Validation
All tests verify:
1. Correct frame length
2. Valid CRC
3. Proper slave address
4. Function code validity
5. Data integrity

## Manual Testing with Hardware

For hardware validation, use a Modbus master tool:
- **ModRSsim2** - Windows simulator
- **mbpoll** - Linux CLI tool
- **pymodbus** - Python library
- **QModMaster** - Cross-platform GUI

### Example with pymodbus:
```python
from pymodbus.client import ModbusSerialClient

client = ModbusSerialClient(
    port='/dev/ttyUSB0',
    baudrate=9600,
    parity='N',
    stopbits=1,
    bytesize=8
)

# Read 10 holding registers from address 0
result = client.read_holding_registers(0, 10, unit=1)
if not result.isError():
    print(f"Registers: {result.registers}")

# Write single register
result = client.write_register(5, 0x1234, unit=1)
if not result.isError():
    print("Write successful")
```

## Debugging

### Enable Debug Logging
When running on hardware with defmt:
```rust
defmt::info!("Received frame: {:x}", frame);
defmt::info!("Parsed request: {:?}", request);
```

### Common Issues
1. **CRC Mismatch**: Check byte order (little-endian)
2. **Invalid Length**: Verify frame completeness
3. **Wrong Address**: Confirm slave address configuration
4. **Function Not Supported**: Check implemented function codes

## Future Enhancements

Planned test additions:
- [ ] History data access tests
- [ ] Float encoding/decoding tests
- [ ] Multi-register read/write tests
- [ ] Timeout handling tests
- [ ] Buffer overflow protection tests

## References

- Modbus Application Protocol Specification V1.1b3
- Modbus over Serial Line Specification V1.02
- Test vectors from official Modbus spec
