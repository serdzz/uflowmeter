//! Modbus Slave Handler
//!
//! This module handles Modbus requests and maps them to application data structures.

#![allow(dead_code)]

use crate::history::RingStorage;
use crate::modbus::{
    ExceptionCode, FunctionCode, ModbusError, ModbusRequest, ModbusResponse, ModbusRtu,
};
use crate::options::Options;
use embedded_storage::Storage;
use heapless::Vec;

/// Register address ranges
pub mod registers {
    /// Options registers (0-31): 32 registers = 64 bytes
    pub const OPTIONS_START: u16 = 0x0000;
    pub const OPTIONS_END: u16 = 0x001F;

    /// Current flow data (100-103): 4 registers = 8 bytes  
    pub const FLOW_RATE: u16 = 0x0064; // f32
    pub const HOUR_FLOW: u16 = 0x0066; // f32
    pub const DAY_FLOW: u16 = 0x0068; // f32
    pub const MONTH_FLOW: u16 = 0x006A; // f32

    /// History base addresses
    pub const HOUR_HISTORY_BASE: u16 = 0x1000;
    pub const DAY_HISTORY_BASE: u16 = 0x2000;
    pub const MONTH_HISTORY_BASE: u16 = 0x3000;
}

/// Modbus slave handler
pub struct ModbusHandler {
    modbus: ModbusRtu,
}

impl ModbusHandler {
    /// Create new Modbus handler
    pub fn new(slave_address: u8) -> Self {
        Self {
            modbus: ModbusRtu::new(slave_address),
        }
    }

    /// Process Modbus request and generate response
    #[allow(clippy::too_many_arguments)]
    pub fn handle_request<S, E>(
        &self,
        frame: &[u8],
        options: &mut Options,
        storage: &mut S,
        flow_rate: f32,
        hour_flow: f32,
        day_flow: f32,
        month_flow: f32,
        _hour_history: &mut dyn HistoryAccess<S, E>,
        _day_history: &mut dyn HistoryAccess<S, E>,
        _month_history: &mut dyn HistoryAccess<S, E>,
    ) -> Result<Vec<u8, 256>, ModbusError>
    where
        S: Storage,
        crate::options::Error<E>: From<S::Error>,
    {
        // Parse request
        let request = match self.modbus.parse_request(frame) {
            Ok(req) => req,
            Err(ModbusError::InvalidSlaveAddress) => {
                // Not for us, ignore
                return Err(ModbusError::InvalidSlaveAddress);
            }
            Err(e) => return Err(e),
        };

        // Handle request
        match request.function_code {
            FunctionCode::ReadHoldingRegisters => self.handle_read_holding_registers(
                &request, options, flow_rate, hour_flow, day_flow, month_flow,
            ),
            FunctionCode::ReadInputRegisters => self
                .handle_read_input_registers(&request, flow_rate, hour_flow, day_flow, month_flow),
            FunctionCode::WriteSingleRegister => {
                self.handle_write_single_register(&request, options, storage)
            }
            FunctionCode::WriteMultipleRegisters => {
                self.handle_write_multiple_registers(&request, options, storage)
            }
            _ => {
                // Unsupported function
                self.modbus.build_exception(
                    request.slave_address,
                    request.function_code as u8,
                    ExceptionCode::IllegalFunction,
                )
            }
        }
    }

    /// Handle Read Holding Registers (0x03)
    fn handle_read_holding_registers(
        &self,
        request: &ModbusRequest,
        options: &Options,
        flow_rate: f32,
        hour_flow: f32,
        day_flow: f32,
        month_flow: f32,
    ) -> Result<Vec<u8, 256>, ModbusError> {
        let start = request.start_address;
        let quantity = request.quantity;

        // Check quantity
        if quantity == 0 || quantity > 125 {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataValue,
            );
        }

        let mut data = Vec::new();
        let byte_count = (quantity * 2) as u8;
        data.push(byte_count)
            .map_err(|_| ModbusError::BufferTooSmall)?;

        // Read from Options (registers 0-31)
        if start <= registers::OPTIONS_END {
            let options_bytes = options.into_bytes();
            let start_byte = (start - registers::OPTIONS_START) as usize * 2;
            let end_byte = start_byte + (quantity as usize * 2);

            if end_byte > options_bytes.len() {
                return self.modbus.build_exception(
                    request.slave_address,
                    request.function_code as u8,
                    ExceptionCode::IllegalDataAddress,
                );
            }

            for &byte in options_bytes
                .iter()
                .skip(start_byte)
                .take(end_byte - start_byte)
            {
                data.push(byte).map_err(|_| ModbusError::BufferTooSmall)?;
            }
        }
        // Read current flow data (registers 100-103)
        else if (registers::FLOW_RATE..registers::FLOW_RATE + 8).contains(&start) {
            let mut values = Vec::<f32, 4>::new();
            values.push(flow_rate).ok();
            values.push(hour_flow).ok();
            values.push(day_flow).ok();
            values.push(month_flow).ok();

            let reg_index = (start - registers::FLOW_RATE) / 2;
            for i in 0..quantity {
                let idx = (reg_index + i) as usize;
                if idx < values.len() {
                    let bytes = values[idx].to_be_bytes();
                    let offset = ((start + i - registers::FLOW_RATE) % 2) as usize * 2;
                    data.push(bytes[offset])
                        .map_err(|_| ModbusError::BufferTooSmall)?;
                    data.push(bytes[offset + 1])
                        .map_err(|_| ModbusError::BufferTooSmall)?;
                }
            }
        } else {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataAddress,
            );
        }

        let response = ModbusResponse {
            slave_address: request.slave_address,
            function_code: request.function_code as u8,
            data,
        };

        self.modbus.build_response(&response)
    }

    /// Handle Read Input Registers (0x04)
    fn handle_read_input_registers(
        &self,
        request: &ModbusRequest,
        flow_rate: f32,
        hour_flow: f32,
        day_flow: f32,
        month_flow: f32,
    ) -> Result<Vec<u8, 256>, ModbusError> {
        // Input registers = read-only current values
        let start = request.start_address;
        let quantity = request.quantity;

        if quantity == 0 || quantity > 125 {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataValue,
            );
        }

        let mut data = Vec::new();
        let byte_count = (quantity * 2) as u8;
        data.push(byte_count)
            .map_err(|_| ModbusError::BufferTooSmall)?;

        // Registers 0-7: flow data (4 floats = 8 registers)
        if (0..8).contains(&start) {
            let mut values = Vec::<f32, 4>::new();
            values.push(flow_rate).ok();
            values.push(hour_flow).ok();
            values.push(day_flow).ok();
            values.push(month_flow).ok();

            for i in 0..quantity {
                let reg = start + i;
                let float_idx = (reg / 2) as usize;
                let byte_idx = (reg % 2) as usize;

                if float_idx < values.len() {
                    let bytes = values[float_idx].to_be_bytes();
                    data.push(bytes[byte_idx * 2])
                        .map_err(|_| ModbusError::BufferTooSmall)?;
                    data.push(bytes[byte_idx * 2 + 1])
                        .map_err(|_| ModbusError::BufferTooSmall)?;
                }
            }
        } else {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataAddress,
            );
        }

        let response = ModbusResponse {
            slave_address: request.slave_address,
            function_code: request.function_code as u8,
            data,
        };

        self.modbus.build_response(&response)
    }

    /// Handle Write Single Register (0x06)
    fn handle_write_single_register<S, E>(
        &self,
        request: &ModbusRequest,
        options: &mut Options,
        storage: &mut S,
    ) -> Result<Vec<u8, 256>, ModbusError>
    where
        S: Storage,
        crate::options::Error<E>: From<S::Error>,
    {
        let address = request.start_address;

        // Only allow writes to Options registers
        if address > registers::OPTIONS_END {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataAddress,
            );
        }

        if request.write_data.len() != 2 {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataValue,
            );
        }

        // Modify options
        let mut options_bytes = options.into_bytes().to_vec();
        let byte_offset = ((address - registers::OPTIONS_START) * 2) as usize;
        options_bytes[byte_offset] = request.write_data[0];
        options_bytes[byte_offset + 1] = request.write_data[1];

        // Update options
        *options = Options::from_bytes(options_bytes.as_slice().try_into().unwrap());

        // Save to storage
        if options.save(storage).is_err() {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::ServerDeviceFailure,
            );
        }

        // Echo back the request as response
        let mut data = Vec::new();
        data.extend_from_slice(&address.to_be_bytes()).ok();
        data.extend_from_slice(&request.write_data).ok();

        let response = ModbusResponse {
            slave_address: request.slave_address,
            function_code: request.function_code as u8,
            data,
        };

        self.modbus.build_response(&response)
    }

    /// Handle Write Multiple Registers (0x10)
    fn handle_write_multiple_registers<S, E>(
        &self,
        request: &ModbusRequest,
        options: &mut Options,
        storage: &mut S,
    ) -> Result<Vec<u8, 256>, ModbusError>
    where
        S: Storage,
        crate::options::Error<E>: From<S::Error>,
    {
        let start = request.start_address;
        let quantity = request.quantity;

        // Only allow writes to Options registers
        if start + quantity - 1 > registers::OPTIONS_END {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataAddress,
            );
        }

        if request.write_data.len() != (quantity * 2) as usize {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::IllegalDataValue,
            );
        }

        // Modify options
        let mut options_bytes = options.into_bytes().to_vec();
        let start_byte = ((start - registers::OPTIONS_START) * 2) as usize;
        for (offset, &byte) in request.write_data.iter().enumerate() {
            options_bytes[start_byte + offset] = byte;
        }

        // Update options
        *options = Options::from_bytes(options_bytes.as_slice().try_into().unwrap());

        // Save to storage
        if options.save(storage).is_err() {
            return self.modbus.build_exception(
                request.slave_address,
                request.function_code as u8,
                ExceptionCode::ServerDeviceFailure,
            );
        }

        // Build response: slave + func + start_addr + quantity
        let mut data = Vec::new();
        data.extend_from_slice(&start.to_be_bytes()).ok();
        data.extend_from_slice(&quantity.to_be_bytes()).ok();

        let response = ModbusResponse {
            slave_address: request.slave_address,
            function_code: request.function_code as u8,
            data,
        };

        self.modbus.build_response(&response)
    }

    /// Get Modbus RTU instance
    pub fn modbus(&self) -> &ModbusRtu {
        &self.modbus
    }

    /// Get mutable Modbus RTU instance
    pub fn modbus_mut(&mut self) -> &mut ModbusRtu {
        &mut self.modbus
    }
}

/// Trait for accessing history data (to avoid generic parameters in handler)
pub trait HistoryAccess<S, E> {
    fn find(&mut self, storage: &mut S, time: u32) -> Result<Option<i32>, crate::history::Error>;
    fn first_timestamp(&mut self) -> u32;
    fn last_timestamp(&mut self) -> u32;
}

impl<S: Storage, E, const OFFSET: usize, const SIZE: i32, const ELEMENT_SIZE: i32>
    HistoryAccess<S, E> for RingStorage<OFFSET, SIZE, ELEMENT_SIZE>
{
    fn find(&mut self, storage: &mut S, time: u32) -> Result<Option<i32>, crate::history::Error> {
        RingStorage::find(self, storage, time)
    }

    fn first_timestamp(&mut self) -> u32 {
        RingStorage::first_stored_timestamp(self)
    }

    fn last_timestamp(&mut self) -> u32 {
        RingStorage::last_stored_timestamp(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::options::Options;

    // Mock storage for testing
    struct MockStorage {
        data: [u8; 4096],
    }

    impl MockStorage {
        fn new() -> Self {
            Self { data: [0xFF; 4096] }
        }
    }

    impl embedded_storage::ReadStorage for MockStorage {
        type Error = crate::options::Error<()>;

        fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
            let start = offset as usize;
            let end = start + bytes.len();
            if end <= self.data.len() {
                bytes.copy_from_slice(&self.data[start..end]);
                Ok(())
            } else {
                Err(crate::options::Error::Storage)
            }
        }

        fn capacity(&self) -> usize {
            self.data.len()
        }
    }

    impl embedded_storage::Storage for MockStorage {
        fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
            let start = offset as usize;
            let end = start + bytes.len();
            if end <= self.data.len() {
                self.data[start..end].copy_from_slice(bytes);
                Ok(())
            } else {
                Err(crate::options::Error::Storage)
            }
        }
    }

    // Mock history for testing
    struct MockHistory;

    impl<S, E> HistoryAccess<S, E> for MockHistory {
        fn find(
            &mut self,
            _storage: &mut S,
            _time: u32,
        ) -> Result<Option<i32>, crate::history::Error> {
            Ok(None)
        }

        fn first_timestamp(&mut self) -> u32 {
            0
        }

        fn last_timestamp(&mut self) -> u32 {
            0
        }
    }

    #[test]
    fn test_read_holding_registers_options() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Set specific serial number for testing
        options.set_serial_number(0x12345678);

        // Read registers 1-2 (serial number: u32 = 2 registers)
        let frame = [0x01, 0x03, 0x00, 0x01, 0x00, 0x02, 0x95, 0xCB];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                1.5,
                10.0,
                100.0,
                1000.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        // Response: slave(1) + func(1) + byte_count(1) + data(4) + crc(2) = 9 bytes
        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x03); // Function code
        assert_eq!(response[2], 0x04); // Byte count (2 registers * 2 bytes)
        // Serial number bytes (bitfield stores as little-endian)
        assert_eq!(response[3], 0x78); // Low byte of serial
        assert_eq!(response[4], 0x56);
        assert_eq!(response[5], 0x34);
        assert_eq!(response[6], 0x12); // High byte of serial
        assert_eq!(response.len(), 9); // Total length with CRC
    }

    #[test]
    fn test_read_holding_registers_flow_data() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Read flow_rate (registers 100-101: 0x0064-0x0065)
        let frame = [0x01, 0x03, 0x00, 0x64, 0x00, 0x02, 0x85, 0xD4];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                1.5,
                10.0,
                100.0,
                1000.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x03); // Function code
        assert_eq!(response[2], 0x04); // Byte count
        // Verify float value 1.5 in IEEE 754 format (big-endian)
        let float_bytes = 1.5f32.to_be_bytes();
        assert_eq!(response[3], float_bytes[0]);
        assert_eq!(response[4], float_bytes[1]);
        assert_eq!(response[5], float_bytes[2]);
        assert_eq!(response[6], float_bytes[3]);
    }

    #[test]
    fn test_read_input_registers() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Read first 4 registers (flow_rate and hour_flow)
        let frame = [0x01, 0x04, 0x00, 0x00, 0x00, 0x04, 0xF1, 0xC9];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                2.5,
                15.0,
                150.0,
                1500.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x04); // Function code
        assert_eq!(response[2], 0x08); // Byte count (4 registers * 2 bytes)

        // Verify flow_rate (2.5)
        let flow_rate_bytes = 2.5f32.to_be_bytes();
        assert_eq!(response[3], flow_rate_bytes[0]);
        assert_eq!(response[4], flow_rate_bytes[1]);
        assert_eq!(response[5], flow_rate_bytes[2]);
        assert_eq!(response[6], flow_rate_bytes[3]);

        // Verify hour_flow (15.0)
        let hour_flow_bytes = 15.0f32.to_be_bytes();
        assert_eq!(response[7], hour_flow_bytes[0]);
        assert_eq!(response[8], hour_flow_bytes[1]);
        assert_eq!(response[9], hour_flow_bytes[2]);
        assert_eq!(response[10], hour_flow_bytes[3]);
    }

    #[test]
    fn test_write_single_register() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Write register 0 (CRC field)
        let frame = [0x01, 0x06, 0x00, 0x00, 0xAB, 0xCD, 0x37, 0x6F];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                0.0,
                0.0,
                0.0,
                0.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        // Response should echo the request
        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x06); // Function code
        assert_eq!(response[2], 0x00); // Address high
        assert_eq!(response[3], 0x00); // Address low
        assert_eq!(response[4], 0xAB); // Value high
        assert_eq!(response[5], 0xCD); // Value low

        // Note: CRC is recalculated by options.save(), so we don't check the exact value
    }

    #[test]
    fn test_write_multiple_registers() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Write 2 registers starting at register 0 (CRC and part of serial)
        let frame = [
            0x01, 0x10, 0x00, 0x00, 0x00, 0x02, 0x04, 0x12, 0x34, 0x56, 0x78, 0x88, 0x9B,
        ];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                0.0,
                0.0,
                0.0,
                0.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        // Response: slave + func + start_addr + quantity + crc
        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x10); // Function code
        assert_eq!(response[2], 0x00); // Start address high
        assert_eq!(response[3], 0x00); // Start address low
        assert_eq!(response[4], 0x00); // Quantity high
        assert_eq!(response[5], 0x02); // Quantity low

        // Note: CRC is recalculated by options.save(), so we don't check the exact value
    }

    #[test]
    fn test_invalid_slave_address() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Request for slave 0x02 (not us)
        let frame = [0x02, 0x03, 0x00, 0x00, 0x00, 0x0A, 0xC4, 0x1E];

        let result = handler.handle_request(
            &frame,
            &mut options,
            &mut storage,
            0.0,
            0.0,
            0.0,
            0.0,
            &mut hour_history,
            &mut day_history,
            &mut month_history,
        );

        assert!(matches!(result, Err(ModbusError::InvalidSlaveAddress)));
    }

    #[test]
    fn test_illegal_data_address() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Try to read from invalid address 0x1000
        let frame = [0x01, 0x03, 0x10, 0x00, 0x00, 0x01, 0x80, 0xCA];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                0.0,
                0.0,
                0.0,
                0.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        // Should be an exception response
        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x83); // Function code with error bit (0x03 | 0x80)
        assert_eq!(response[2], 0x02); // Exception code: IllegalDataAddress
    }

    #[test]
    fn test_illegal_quantity() {
        let handler = ModbusHandler::new(0x01);
        let mut options = Options::default();
        let mut storage = MockStorage::new();
        let mut hour_history = MockHistory;
        let mut day_history = MockHistory;
        let mut month_history = MockHistory;

        // Try to read 0 registers (invalid)
        let frame = [0x01, 0x03, 0x00, 0x00, 0x00, 0x00, 0x45, 0xCA];

        let response = handler
            .handle_request(
                &frame,
                &mut options,
                &mut storage,
                0.0,
                0.0,
                0.0,
                0.0,
                &mut hour_history,
                &mut day_history,
                &mut month_history,
            )
            .unwrap();

        // Should be an exception response
        assert_eq!(response[0], 0x01); // Slave address
        assert_eq!(response[1], 0x83); // Function code with error bit
        assert_eq!(response[2], 0x03); // Exception code: IllegalDataValue
    }
}
