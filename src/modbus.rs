//! Modbus RTU Slave Implementation
//!
//! This module implements a Modbus RTU slave protocol over serial communication.
//! Supports reading configuration (Options) and history data (Hour/Day/Month).
//!
//! ## Register Map
//!
//! ### Configuration Registers (Function 0x03 - Read Holding Registers)
//! - Address 0-31: Options structure (64 bytes = 32 registers)
//!   - 0: CRC (u16)
//!   - 1-2: Serial Number (u32)
//!   - 3: Sensor Type (u8)
//!   - 4-8: TDC1000 registers (10 bytes)
//!   - 9-13: TDC7200 registers (10 bytes)
//!   - 14-31: Calibration data, totals, etc.
//!
//! ### Input Registers (Function 0x04 - Read Input Registers)
//! - Address 0-3: Current flow data
//!   - 0-1: Flow rate (f32)
//!   - 2-3: Hour flow (f32)
//!
//! ### History Data (Function 0x17 - Read/Write Multiple Registers)
//! - Hour History: Start address 0x1000
//! - Day History: Start address 0x2000
//! - Month History: Start address 0x3000

#![allow(dead_code)]

use heapless::Vec;

/// Modbus function codes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FunctionCode {
    ReadCoils = 0x01,
    ReadDiscreteInputs = 0x02,
    ReadHoldingRegisters = 0x03,
    ReadInputRegisters = 0x04,
    WriteSingleCoil = 0x05,
    WriteSingleRegister = 0x06,
    WriteMultipleCoils = 0x0F,
    WriteMultipleRegisters = 0x10,
    ReadWriteMultipleRegisters = 0x17,
}

impl FunctionCode {
    fn from_u8(value: u8) -> Option<Self> {
        match value {
            0x01 => Some(Self::ReadCoils),
            0x02 => Some(Self::ReadDiscreteInputs),
            0x03 => Some(Self::ReadHoldingRegisters),
            0x04 => Some(Self::ReadInputRegisters),
            0x05 => Some(Self::WriteSingleCoil),
            0x06 => Some(Self::WriteSingleRegister),
            0x0F => Some(Self::WriteMultipleCoils),
            0x10 => Some(Self::WriteMultipleRegisters),
            0x17 => Some(Self::ReadWriteMultipleRegisters),
            _ => None,
        }
    }
}

/// Modbus exception codes
#[derive(Debug, Clone, Copy)]
pub enum ExceptionCode {
    IllegalFunction = 0x01,
    IllegalDataAddress = 0x02,
    IllegalDataValue = 0x03,
    ServerDeviceFailure = 0x04,
}

/// Modbus error
#[derive(Debug)]
pub enum ModbusError {
    InvalidCrc,
    InvalidLength,
    InvalidSlaveAddress,
    BufferTooSmall,
    Exception(ExceptionCode),
}

/// Modbus request
#[derive(Debug)]
pub struct ModbusRequest {
    pub slave_address: u8,
    pub function_code: FunctionCode,
    pub start_address: u16,
    pub quantity: u16,
    pub write_data: Vec<u8, 256>,
}

/// Modbus response
#[derive(Debug)]
pub struct ModbusResponse {
    pub slave_address: u8,
    pub function_code: u8,
    pub data: Vec<u8, 256>,
}

/// Modbus RTU frame parser and builder
pub struct ModbusRtu {
    slave_address: u8,
}

impl ModbusRtu {
    /// Create new Modbus RTU handler with specified slave address
    pub fn new(slave_address: u8) -> Self {
        Self { slave_address }
    }

    /// Calculate CRC16 (Modbus)
    fn calculate_crc(data: &[u8]) -> u16 {
        let mut crc: u16 = 0xFFFF;
        for byte in data {
            crc ^= *byte as u16;
            for _ in 0..8 {
                if (crc & 0x0001) != 0 {
                    crc = (crc >> 1) ^ 0xA001;
                } else {
                    crc >>= 1;
                }
            }
        }
        crc
    }

    /// Parse incoming Modbus RTU frame
    pub fn parse_request(&self, frame: &[u8]) -> Result<ModbusRequest, ModbusError> {
        // Minimum frame: slave(1) + function(1) + data(4) + crc(2) = 8 bytes
        if frame.len() < 8 {
            return Err(ModbusError::InvalidLength);
        }

        // Check slave address
        let slave_address = frame[0];
        if slave_address != self.slave_address && slave_address != 0 {
            return Err(ModbusError::InvalidSlaveAddress);
        }

        // Verify CRC
        let received_crc = u16::from_le_bytes([frame[frame.len() - 2], frame[frame.len() - 1]]);
        let calculated_crc = Self::calculate_crc(&frame[..frame.len() - 2]);
        if received_crc != calculated_crc {
            return Err(ModbusError::InvalidCrc);
        }

        // Parse function code
        let function_code = FunctionCode::from_u8(frame[1])
            .ok_or(ModbusError::Exception(ExceptionCode::IllegalFunction))?;

        // Parse data based on function code
        match function_code {
            FunctionCode::ReadHoldingRegisters | FunctionCode::ReadInputRegisters => {
                let start_address = u16::from_be_bytes([frame[2], frame[3]]);
                let quantity = u16::from_be_bytes([frame[4], frame[5]]);

                Ok(ModbusRequest {
                    slave_address,
                    function_code,
                    start_address,
                    quantity,
                    write_data: Vec::new(),
                })
            }
            FunctionCode::WriteSingleRegister => {
                let start_address = u16::from_be_bytes([frame[2], frame[3]]);
                let mut write_data = Vec::new();
                write_data.push(frame[4]).ok();
                write_data.push(frame[5]).ok();

                Ok(ModbusRequest {
                    slave_address,
                    function_code,
                    start_address,
                    quantity: 1,
                    write_data,
                })
            }
            FunctionCode::WriteMultipleRegisters => {
                let start_address = u16::from_be_bytes([frame[2], frame[3]]);
                let quantity = u16::from_be_bytes([frame[4], frame[5]]);
                let byte_count = frame[6] as usize;

                if frame.len() < 7 + byte_count + 2 {
                    return Err(ModbusError::InvalidLength);
                }

                let mut write_data = Vec::new();
                for i in 0..byte_count {
                    write_data
                        .push(frame[7 + i])
                        .map_err(|_| ModbusError::BufferTooSmall)?;
                }

                Ok(ModbusRequest {
                    slave_address,
                    function_code,
                    start_address,
                    quantity,
                    write_data,
                })
            }
            _ => Err(ModbusError::Exception(ExceptionCode::IllegalFunction)),
        }
    }

    /// Build response frame
    pub fn build_response(&self, response: &ModbusResponse) -> Result<Vec<u8, 256>, ModbusError> {
        let mut frame = Vec::new();

        // Slave address
        frame
            .push(response.slave_address)
            .map_err(|_| ModbusError::BufferTooSmall)?;

        // Function code
        frame
            .push(response.function_code)
            .map_err(|_| ModbusError::BufferTooSmall)?;

        // Data
        for byte in &response.data {
            frame.push(*byte).map_err(|_| ModbusError::BufferTooSmall)?;
        }

        // Calculate and append CRC
        let crc = Self::calculate_crc(&frame);
        let crc_bytes = crc.to_le_bytes();
        frame
            .push(crc_bytes[0])
            .map_err(|_| ModbusError::BufferTooSmall)?;
        frame
            .push(crc_bytes[1])
            .map_err(|_| ModbusError::BufferTooSmall)?;

        Ok(frame)
    }

    /// Build exception response
    pub fn build_exception(
        &self,
        slave_address: u8,
        function_code: u8,
        exception: ExceptionCode,
    ) -> Result<Vec<u8, 256>, ModbusError> {
        let mut frame = Vec::new();

        frame
            .push(slave_address)
            .map_err(|_| ModbusError::BufferTooSmall)?;
        frame
            .push(function_code | 0x80)
            .map_err(|_| ModbusError::BufferTooSmall)?;
        frame
            .push(exception as u8)
            .map_err(|_| ModbusError::BufferTooSmall)?;

        let crc = Self::calculate_crc(&frame);
        let crc_bytes = crc.to_le_bytes();
        frame
            .push(crc_bytes[0])
            .map_err(|_| ModbusError::BufferTooSmall)?;
        frame
            .push(crc_bytes[1])
            .map_err(|_| ModbusError::BufferTooSmall)?;

        Ok(frame)
    }

    /// Get slave address
    pub fn slave_address(&self) -> u8 {
        self.slave_address
    }

    /// Set slave address
    pub fn set_slave_address(&mut self, address: u8) {
        self.slave_address = address;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crc_calculation() {
        // Example from Modbus spec
        let data = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A];
        let crc = ModbusRtu::calculate_crc(&data);
        assert_eq!(crc, 0xCDC5); // Expected CRC for this frame
    }

    #[test]
    fn test_parse_read_holding_registers() {
        let modbus = ModbusRtu::new(0x01);
        // Read 10 registers starting at address 0
        let frame = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC5, 0xCD];

        let request = modbus.parse_request(&frame).unwrap();
        assert_eq!(request.slave_address, 0x01);
        assert_eq!(request.function_code, FunctionCode::ReadHoldingRegisters);
        assert_eq!(request.start_address, 0x0000);
        assert_eq!(request.quantity, 0x000A);
    }

    #[test]
    fn test_build_response() {
        let modbus = ModbusRtu::new(0x01);
        let mut data = Vec::new();
        data.push(0x04).unwrap(); // Byte count
        data.push(0x12).unwrap();
        data.push(0x34).unwrap();
        data.push(0x56).unwrap();
        data.push(0x78).unwrap();

        let response = ModbusResponse {
            slave_address: 0x01,
            function_code: 0x03,
            data,
        };

        let frame = modbus.build_response(&response).unwrap();

        // Verify frame structure
        assert_eq!(frame[0], 0x01); // Slave address
        assert_eq!(frame[1], 0x03); // Function code
        assert_eq!(frame[2], 0x04); // Byte count
        assert_eq!(frame.len(), 9); // Total length including CRC (5 data + 2 header + 2 CRC)
    }

    #[test]
    fn test_parse_write_single_register() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x01, 0x06, 0x00, 0x05, 0x12, 0x34, 0x94, 0xBC];

        let request = modbus.parse_request(&frame).unwrap();
        assert_eq!(request.slave_address, 0x01);
        assert_eq!(request.function_code, FunctionCode::WriteSingleRegister);
        assert_eq!(request.start_address, 0x0005);
        assert_eq!(request.quantity, 1);
        assert_eq!(request.write_data.len(), 2);
    }

    #[test]
    fn test_parse_write_multiple_registers() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [
            0x01, 0x10, 0x00, 0x01, 0x00, 0x02, 0x04, 0x00, 0x0A, 0x01, 0x02, 0x92, 0x30,
        ];

        let request = modbus.parse_request(&frame).unwrap();
        assert_eq!(request.slave_address, 0x01);
        assert_eq!(request.function_code, FunctionCode::WriteMultipleRegisters);
        assert_eq!(request.start_address, 0x0001);
        assert_eq!(request.quantity, 0x0002);
        assert_eq!(request.write_data.len(), 4);
    }

    #[test]
    fn test_parse_read_input_registers() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x01, 0x04, 0x00, 0x00, 0x00, 0x04, 0xF1, 0xC9];

        let request = modbus.parse_request(&frame).unwrap();
        assert_eq!(request.slave_address, 0x01);
        assert_eq!(request.function_code, FunctionCode::ReadInputRegisters);
        assert_eq!(request.start_address, 0x0000);
        assert_eq!(request.quantity, 0x0004);
    }

    #[test]
    fn test_parse_invalid_crc() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x01, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xFF, 0xFF];

        let result = modbus.parse_request(&frame);
        assert!(matches!(result, Err(ModbusError::InvalidCrc)));
    }

    #[test]
    fn test_parse_invalid_length() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x01, 0x03];

        let result = modbus.parse_request(&frame);
        assert!(matches!(result, Err(ModbusError::InvalidLength)));
    }

    #[test]
    fn test_parse_wrong_slave_address() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x02, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC4, 0x1E];

        let result = modbus.parse_request(&frame);
        assert!(matches!(result, Err(ModbusError::InvalidSlaveAddress)));
    }

    #[test]
    fn test_parse_broadcast_address() {
        let modbus = ModbusRtu::new(0x01);
        let frame = [0x00, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC4, 0x1C];

        let request = modbus.parse_request(&frame).unwrap();
        assert_eq!(request.slave_address, 0x00);
    }

    #[test]
    fn test_build_exception_response() {
        let modbus = ModbusRtu::new(0x01);
        let frame = modbus
            .build_exception(0x01, 0x03, ExceptionCode::IllegalDataAddress)
            .unwrap();

        assert_eq!(frame[0], 0x01); // Slave address
        assert_eq!(frame[1], 0x83); // Function code with error bit (0x03 | 0x80)
        assert_eq!(frame[2], 0x02); // Exception code
        assert_eq!(frame.len(), 5); // slave + func + exception + CRC
    }

    #[test]
    fn test_crc_another_example() {
        let data = [0x11, 0x03, 0x00, 0x6B, 0x00, 0x03];
        let crc = ModbusRtu::calculate_crc(&data);
        assert_eq!(crc, 0x8776);
    }

    #[test]
    fn test_slave_address_get_set() {
        let mut modbus = ModbusRtu::new(0x01);
        assert_eq!(modbus.slave_address(), 0x01);

        modbus.set_slave_address(0x05);
        assert_eq!(modbus.slave_address(), 0x05);
    }

    #[test]
    fn test_function_code_from_u8() {
        assert_eq!(
            FunctionCode::from_u8(0x03),
            Some(FunctionCode::ReadHoldingRegisters)
        );
        assert_eq!(
            FunctionCode::from_u8(0x04),
            Some(FunctionCode::ReadInputRegisters)
        );
        assert_eq!(
            FunctionCode::from_u8(0x06),
            Some(FunctionCode::WriteSingleRegister)
        );
        assert_eq!(
            FunctionCode::from_u8(0x10),
            Some(FunctionCode::WriteMultipleRegisters)
        );
        assert_eq!(FunctionCode::from_u8(0xFF), None);
    }
}
