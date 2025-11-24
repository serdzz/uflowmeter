# Modbus RTU Register Map

## Overview

The flowmeter implements Modbus RTU slave protocol over RS-485/UART serial communication.

**Default Settings:**
- Baudrate: 9600 bps (configurable)
- Data bits: 8
- Parity: None
- Stop bits: 1
- Slave Address: 1 (configurable via register 0x0027)

---

## Supported Function Codes

| Code | Function | Description |
|------|----------|-------------|
| 0x03 | Read Holding Registers | Read configuration and current flow data |
| 0x04 | Read Input Registers | Read current flow measurements (read-only) |
| 0x06 | Write Single Register | Write single configuration register |
| 0x10 | Write Multiple Registers | Write multiple configuration registers |

---

## Register Map

### Holding Registers (Function 0x03) - Configuration

#### Options Structure (Addresses 0x0000 - 0x001F) - 32 registers

| Address | Name | Type | R/W | Description |
|---------|------|------|-----|-------------|
| 0x0000 | CRC | u16 | R | Configuration CRC checksum |
| 0x0001-0x0002 | Serial Number | u32 | R/W | Device serial number |
| 0x0003 | Sensor Type | u8 | R/W | Sensor type identifier |
| 0x0004-0x0008 | TDC1000 Regs | 10 bytes | R/W | TDC1000 register values |
| 0x0009-0x000D | TDC7200 Regs | 10 bytes | R/W | TDC7200 register values |
| 0x000E-0x000F | Zero1 | i32 | R/W | Zero calibration value 1 |
| 0x0010-0x0011 | Zero2 | i32 | R/W | Zero calibration value 2 |
| 0x0012-0x0013 | V11 | f32 | R/W | Velocity calibration 1.1 |
| 0x0014-0x0015 | V12 | f32 | R/W | Velocity calibration 1.2 |
| 0x0016-0x0017 | V13 | f32 | R/W | Velocity calibration 1.3 |
| 0x0018-0x0019 | V21 | f32 | R/W | Velocity calibration 2.1 |
| 0x001A-0x001B | V22 | f32 | R/W | Velocity calibration 2.2 |
| 0x001C-0x001D | V23 | f32 | R/W | Velocity calibration 2.3 |
| 0x001E-0x001F | K11 | f32 | R/W | K-factor calibration 1.1 |
| 0x0020-0x0021 | K12 | f32 | R/W | K-factor calibration 1.2 |
| 0x0022-0x0023 | K13 | f32 | R/W | K-factor calibration 1.3 |
| 0x0024-0x0025 | K21 | f32 | R/W | K-factor calibration 2.1 |
| 0x0026-0x0027 | K22 | f32 | R/W | K-factor calibration 2.2 |
| 0x0028-0x0029 | K23 | f32 | R/W | K-factor calibration 2.3 |
| 0x002A-0x002B | Uptime | u32 | R | Device uptime (seconds) |
| 0x002C-0x002D | Total | u32 | R/W | Total accumulated flow |
| 0x002E-0x002F | Hour Total | u32 | R | Current hour accumulated flow |
| 0x0030-0x0031 | Day Total | u32 | R | Current day accumulated flow |
| 0x0032-0x0033 | Month Total | u32 | R | Current month accumulated flow |
| 0x0034-0x0035 | Rest | u32 | R/W | Reserved |
| 0x0036 | Enable Negative | u8 | R/W | Enable negative flow (0=No, 1=Yes) |
| 0x0037 | Slave Address | u8 | R/W | Modbus slave address (1-247) |
| 0x0038 | Comm Type | u8 | R/W | Communication type |
| 0x0039 | Modbus Mode | u8 | R/W | Modbus mode settings |

#### Current Flow Data (Addresses 0x0064 - 0x006B) - 8 registers

| Address | Name | Type | R/W | Description |
|---------|------|------|-----|-------------|
| 0x0064-0x0065 | Flow Rate | f32 | R | Instantaneous flow rate (L/min) |
| 0x0066-0x0067 | Hour Flow | f32 | R | Accumulated flow this hour (L) |
| 0x0068-0x0069 | Day Flow | f32 | R | Accumulated flow today (L) |
| 0x006A-0x006B | Month Flow | f32 | R | Accumulated flow this month (L) |

---

### Input Registers (Function 0x04) - Read-Only Measurements

#### Flow Measurements (Addresses 0x0000 - 0x0007) - 8 registers

| Address | Name | Type | Description |
|---------|------|------|-------------|
| 0x0000-0x0001 | Flow Rate | f32 | Instantaneous flow rate (L/min) |
| 0x0002-0x0003 | Hour Flow | f32 | Accumulated flow this hour (L) |
| 0x0004-0x0005 | Day Flow | f32 | Accumulated flow today (L) |
| 0x0006-0x0007 | Month Flow | f32 | Accumulated flow this month (L) |

---

### History Data (Reserved for future implementation)

| Base Address | History Type | Element Size | Max Elements |
|--------------|--------------|--------------|--------------|
| 0x1000 | Hour History | 1 hour | 2160 (90 days) |
| 0x2000 | Day History | 1 day | 1116 (3 years) |
| 0x3000 | Month History | 1 month | 120 (10 years) |

---

## Usage Examples

### Example 1: Read Current Flow Rate

**Request:**
```
Slave: 0x01
Function: 0x04 (Read Input Registers)
Start Address: 0x0000
Quantity: 0x0002 (2 registers = 1 float)
CRC: [calculated]
```

**Response:**
```
Slave: 0x01
Function: 0x04
Byte Count: 0x04
Data: [4 bytes of IEEE 754 float, big-endian]
CRC: [calculated]
```

**Python example:**
```python
import struct
from pymodbus.client import ModbusSerialClient

client = ModbusSerialClient(port='/dev/ttyUSB0', baudrate=9600)
result = client.read_input_registers(0, 2, unit=1)

if result.isError():
    print("Error reading")
else:
    # Convert 2 registers to float
    bytes_data = struct.pack('>HH', result.registers[0], result.registers[1])
    flow_rate = struct.unpack('>f', bytes_data)[0]
    print(f"Flow rate: {flow_rate} L/min")
```

---

### Example 2: Read All Current Flow Data

**Request:**
```
Function: 0x03 (Read Holding Registers)
Start Address: 0x0064
Quantity: 0x0008 (8 registers = 4 floats)
```

**Python example:**
```python
result = client.read_holding_registers(0x64, 8, unit=1)
if not result.isError():
    # Extract 4 floats
    flow_rate = struct.unpack('>f', struct.pack('>HH', *result.registers[0:2]))[0]
    hour_flow = struct.unpack('>f', struct.pack('>HH', *result.registers[2:4]))[0]
    day_flow = struct.unpack('>f', struct.pack('>HH', *result.registers[4:6]))[0]
    month_flow = struct.unpack('>f', struct.pack('>HH', *result.registers[6:8]))[0]
    
    print(f"Flow rate: {flow_rate} L/min")
    print(f"Hour total: {hour_flow} L")
    print(f"Day total: {day_flow} L")
    print(f"Month total: {month_flow} L")
```

---

### Example 3: Write Slave Address

**Request:**
```
Function: 0x06 (Write Single Register)
Address: 0x0037
Value: 0x0005 (new slave address = 5)
```

**Python example:**
```python
result = client.write_register(0x37, 5, unit=1)
if not result.isError():
    print("Slave address changed to 5")
```

---

### Example 4: Read Serial Number

**Request:**
```
Function: 0x03 (Read Holding Registers)
Start Address: 0x0001
Quantity: 0x0002 (2 registers = u32)
```

**Python example:**
```python
result = client.read_holding_registers(1, 2, unit=1)
if not result.isError():
    serial_number = (result.registers[0] << 16) | result.registers[1]
    print(f"Serial number: {serial_number}")
```

---

### Example 5: Write Multiple Calibration Values

**Request:**
```
Function: 0x10 (Write Multiple Registers)
Start Address: 0x0012 (V11)
Quantity: 0x0006 (6 registers = 3 floats: V11, V12, V13)
Byte Count: 0x0C (12 bytes)
Data: [12 bytes of 3 IEEE 754 floats]
```

**Python example:**
```python
import struct

v11 = 1480.5
v12 = 1481.2
v13 = 1479.8

# Pack floats to registers
values = []
for val in [v11, v12, v13]:
    bytes_data = struct.pack('>f', val)
    values.extend(struct.unpack('>HH', bytes_data))

result = client.write_registers(0x12, values, unit=1)
if not result.isError():
    print("Calibration values written")
```

---

## Error Codes

| Exception Code | Name | Description |
|----------------|------|-------------|
| 0x01 | Illegal Function | Function code not supported |
| 0x02 | Illegal Data Address | Register address out of range |
| 0x03 | Illegal Data Value | Invalid value or quantity |
| 0x04 | Server Device Failure | Device error (e.g., storage failure) |

---

## Notes

1. **Float Format:** All float values use IEEE 754 single-precision (32-bit) format, big-endian byte order.

2. **Register Addressing:** Modbus uses 0-based addressing. Register 0 = address 0x0000.

3. **Data Persistence:** Changes to holding registers (0x0000-0x003F) are saved to EEPROM immediately.

4. **Slave Address Change:** After changing the slave address (register 0x0037), the device will respond to the new address on the next request.

5. **History Access:** History data access is planned for future firmware versions.

6. **CRC:** All Modbus RTU frames use CRC-16 (Modbus polynomial 0xA001) for error detection.
