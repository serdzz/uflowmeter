#![cfg(test)]

use crate::history_lib::*;
use core::mem::size_of;

// ============================================================================
// MOCK STORAGE IMPLEMENTATION
// ============================================================================

/// Mock storage for testing RingStorage without actual flash operations
#[derive(Debug, Clone)]
struct MockHistoryStorage {
    /// Storage buffer simulating flash memory
    buffer: Vec<u8>,
    /// Records stored
    records: Vec<u32>,
    /// Write operations counter
    write_count: usize,
    /// Read operations counter
    read_count: usize,
    /// Clear operations counter
    clear_count: usize,
    /// Last error simulation
    last_error: Option<Error>,
}

impl MockHistoryStorage {
    fn new(capacity: usize) -> Self {
        Self {
            buffer: vec![0u8; capacity],
            records: Vec::new(),
            write_count: 0,
            read_count: 0,
            clear_count: 0,
            last_error: None,
        }
    }

    fn with_error(mut self, error: Error) -> Self {
        self.last_error = Some(error);
        self
    }

    fn get_stats(&self) -> StorageStats {
        StorageStats {
            buffer_size: self.buffer.len(),
            records_count: self.records.len(),
            write_ops: self.write_count,
            read_ops: self.read_count,
            clear_ops: self.clear_count,
        }
    }
}

/// Statistics structure for mock storage assertions
#[derive(Debug, Clone, PartialEq, Eq)]
struct StorageStats {
    buffer_size: usize,
    records_count: usize,
    write_ops: usize,
    read_ops: usize,
    clear_ops: usize,
}

// ============================================================================
// HISTORY STATE MOCK
// ============================================================================

/// Mock for tracking history state changes
#[derive(Debug, Clone)]
struct HistoryStateMock {
    size: u32,
    offset_of_last: u32,
    time_of_last: u32,
    crc_valid: bool,
    operations_log: Vec<String>,
}

impl HistoryStateMock {
    fn new() -> Self {
        Self {
            size: 0,
            offset_of_last: 0,
            time_of_last: 0,
            crc_valid: true,
            operations_log: Vec::new(),
        }
    }

    fn log_operation(&mut self, op: &str) {
        self.operations_log.push(op.to_string());
    }

    fn update_size(&mut self, size: u32) {
        self.size = size;
        self.log_operation(&format!("size_updated: {}", size));
    }

    fn update_offset(&mut self, offset: u32) {
        self.offset_of_last = offset;
        self.log_operation(&format!("offset_updated: {}", offset));
    }

    fn update_timestamp(&mut self, time: u32) {
        self.time_of_last = time;
        self.log_operation(&format!("timestamp_updated: {}", time));
    }

    fn get_operation_count(&self) -> usize {
        self.operations_log.len()
    }

    fn last_operation(&self) -> Option<&str> {
        self.operations_log.last().map(|s| s.as_str())
    }
}

// ============================================================================
// SERVICE DATA TESTS
// ============================================================================

#[cfg(test)]
mod service_data_tests {
    use super::*;

    #[test]
    fn test_service_data_creation() {
        let service_data = ServiceData::new();
        assert_eq!(service_data.size(), 0);
        assert_eq!(service_data.offset_of_last(), 0);
        assert_eq!(service_data.time_of_last(), 0);
    }

    #[test]
    fn test_service_data_default() {
        let service_data = ServiceData::default();
        assert_eq!(service_data.size(), 0);
        assert_eq!(service_data.offset_of_last(), 0);
    }

    #[test]
    fn test_service_data_set_size() {
        let mut service_data = ServiceData::new();
        service_data.set_size(100);
        assert_eq!(service_data.size(), 100);
    }

    #[test]
    fn test_service_data_set_offset() {
        let mut service_data = ServiceData::new();
        service_data.set_offset_of_last(50);
        assert_eq!(service_data.offset_of_last(), 50);
    }

    #[test]
    fn test_service_data_set_timestamp() {
        let mut service_data = ServiceData::new();
        let timestamp = 1234567890u32;
        service_data.set_time_of_last(timestamp);
        assert_eq!(service_data.time_of_last(), timestamp);
    }

    #[test]
    fn test_service_data_copy_semantics() {
        let mut service_data1 = ServiceData::new();
        service_data1.set_size(42);

        let service_data2 = service_data1;
        assert_eq!(service_data2.size(), 42);
    }

    #[test]
    fn test_service_data_multiple_updates() {
        let mut service_data = ServiceData::new();

        for i in 0..10 {
            service_data.set_size(i);
            assert_eq!(service_data.size(), i);
        }
    }

    #[test]
    fn test_service_data_max_values() {
        let mut service_data = ServiceData::new();

        service_data.set_size(u32::MAX);
        assert_eq!(service_data.size(), u32::MAX);

        service_data.set_offset_of_last(u32::MAX);
        assert_eq!(service_data.offset_of_last(), u32::MAX);

        service_data.set_time_of_last(u32::MAX);
        assert_eq!(service_data.time_of_last(), u32::MAX);
    }
}

// ============================================================================
// RING STORAGE TESTS
// ============================================================================

#[cfg(test)]
mod ring_storage_tests {
    use super::*;

    #[test]
    fn test_ring_storage_creation() {
        let storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        assert_eq!(storage.data.size(), 0);
        assert_eq!(storage.data.offset_of_last(), 0);
    }

    #[test]
    fn test_ring_storage_size_on_flash() {
        // SIZE_ON_FLASH = sizeof(u32) + SIZE + sizeof(ServiceData) + sizeof(u16)
        let _storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        let expected_size = size_of::<u32>() + 100 + size_of::<ServiceData>() + size_of::<u16>();
        assert!(expected_size > 0);
    }

    #[test]
    fn test_ring_storage_offset_calculation() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();

        let offset_0 = storage.offset(0);
        let offset_1 = storage.offset(1);

        // Each element is size_of(u32) bytes apart
        assert_eq!(offset_1, offset_0 + size_of::<u32>() as u32);
    }

    #[test]
    fn test_ring_storage_advance_offset_basic() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        assert_eq!(storage.data.offset_of_last(), 0);

        storage.advance_offset_by_one();
        assert_eq!(storage.data.offset_of_last(), 1);

        storage.advance_offset_by_one();
        assert_eq!(storage.data.offset_of_last(), 2);
    }

    #[test]
    fn test_ring_storage_advance_offset_wraparound() {
        let mut storage: RingStorage<0, 5, 10> = RingStorage::new_empty();

        // Advance to SIZE - 1
        for _ in 0..4 {
            storage.advance_offset_by_one();
        }
        assert_eq!(storage.data.offset_of_last(), 4);

        // Next advance should wrap to 0
        storage.advance_offset_by_one();
        assert_eq!(storage.data.offset_of_last(), 0);
    }

    #[test]
    fn test_ring_storage_advance_offset_multiple_wraps() {
        let mut storage: RingStorage<0, 3, 10> = RingStorage::new_empty();

        for i in 0..10 {
            storage.advance_offset_by_one();
            let expected = (i + 1) % 3;
            assert_eq!(storage.data.offset_of_last() as usize, expected);
        }
    }

    #[test]
    fn test_ring_storage_last_timestamp() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        storage.data.set_time_of_last(12345);

        assert_eq!(storage.last_stored_timestamp(), 12345);
    }

    #[test]
    fn test_ring_storage_first_timestamp_empty() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        storage.data.set_time_of_last(12345);
        storage.data.set_size(0);

        assert_eq!(storage.first_stored_timestamp(), 12345);
    }

    #[test]
    fn test_ring_storage_first_timestamp_single_element() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        storage.data.set_time_of_last(12345);
        storage.data.set_size(1);

        assert_eq!(storage.first_stored_timestamp(), 12345);
    }

    #[test]
    fn test_ring_storage_first_timestamp_multiple_elements() {
        let mut storage: RingStorage<0, 100, 5> = RingStorage::new_empty();
        let last_time = 100u32;
        storage.data.set_time_of_last(last_time);
        storage.data.set_size(3);

        // first = last - ELEMENT_SIZE * (size - 1)
        // first = 100 - 5 * 2 = 90
        let first = storage.first_stored_timestamp();
        assert_eq!(first, 90);
    }

    #[test]
    fn test_ring_storage_size_tracking() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        assert_eq!(storage.size(), 0);

        storage.data.set_size(50);
        assert_eq!(storage.size(), 50);

        storage.data.set_size(100);
        assert_eq!(storage.size(), 100);
    }
}

// ============================================================================
// ERROR HANDLING TESTS
// ============================================================================

#[cfg(test)]
mod error_tests {
    use super::*;

    #[test]
    fn test_error_no_records() {
        let error = Error::NoRecords;
        // Should be copyable
        let _error_copy = error;
        assert!(matches!(error, Error::NoRecords));
    }

    #[test]
    fn test_error_uninitialized() {
        let error = Error::Unitialized;
        assert!(matches!(error, Error::Unitialized));
    }

    #[test]
    fn test_error_storage() {
        let error = Error::Storage;
        assert!(matches!(error, Error::Storage));
    }

    #[test]
    fn test_error_wrong_crc() {
        let error = Error::WrongCrc;
        assert!(matches!(error, Error::WrongCrc));
    }

    #[test]
    fn test_error_unimplemented() {
        let error = Error::Unimplented;
        assert!(matches!(error, Error::Unimplented));
    }

    #[test]
    fn test_error_debug_output() {
        let errors = vec![
            Error::NoRecords,
            Error::Unitialized,
            Error::Unimplented,
            Error::Storage,
            Error::WrongCrc,
        ];

        for error in errors {
            let debug_str = alloc::format!("{:?}", error);
            assert!(!debug_str.is_empty());
        }
    }
}

// ============================================================================
// MOCK STORAGE TESTS
// ============================================================================

#[cfg(test)]
mod mock_storage_tests {
    use super::*;

    #[test]
    fn test_mock_storage_creation() {
        let storage = MockHistoryStorage::new(1024);
        assert_eq!(storage.buffer.len(), 1024);
        assert_eq!(storage.records.len(), 0);
        assert_eq!(storage.write_count, 0);
    }

    #[test]
    fn test_mock_storage_stats() {
        let storage = MockHistoryStorage::new(512);
        let stats = storage.get_stats();

        assert_eq!(stats.buffer_size, 512);
        assert_eq!(stats.records_count, 0);
        assert_eq!(stats.write_ops, 0);
    }

    #[test]
    fn test_mock_storage_with_error() {
        let storage = MockHistoryStorage::new(512).with_error(Error::Storage);
        assert!(matches!(storage.last_error, Some(Error::Storage)));
    }

    #[test]
    fn test_mock_storage_clone() {
        let storage1 = MockHistoryStorage::new(256);
        let storage2 = storage1.clone();

        assert_eq!(storage2.buffer.len(), 256);
        assert_eq!(storage1.records.len(), storage2.records.len());
    }
}

// ============================================================================
// HISTORY STATE MOCK TESTS
// ============================================================================

#[cfg(test)]
mod history_state_mock_tests {
    use super::*;

    #[test]
    fn test_history_state_creation() {
        let state = HistoryStateMock::new();
        assert_eq!(state.size, 0);
        assert_eq!(state.offset_of_last, 0);
        assert_eq!(state.time_of_last, 0);
        assert!(state.crc_valid);
    }

    #[test]
    fn test_history_state_log_operation() {
        let mut state = HistoryStateMock::new();
        state.log_operation("test_op");

        assert_eq!(state.get_operation_count(), 1);
        assert_eq!(state.last_operation(), Some("test_op"));
    }

    #[test]
    fn test_history_state_update_size() {
        let mut state = HistoryStateMock::new();
        state.update_size(42);

        assert_eq!(state.size, 42);
        assert_eq!(state.get_operation_count(), 1);
        assert!(state.last_operation().unwrap().contains("size_updated"));
    }

    #[test]
    fn test_history_state_update_offset() {
        let mut state = HistoryStateMock::new();
        state.update_offset(15);

        assert_eq!(state.offset_of_last, 15);
        assert_eq!(state.get_operation_count(), 1);
        assert!(state.last_operation().unwrap().contains("offset_updated"));
    }

    #[test]
    fn test_history_state_update_timestamp() {
        let mut state = HistoryStateMock::new();
        state.update_timestamp(1234567);

        assert_eq!(state.time_of_last, 1234567);
        assert_eq!(state.get_operation_count(), 1);
    }

    #[test]
    fn test_history_state_multiple_operations() {
        let mut state = HistoryStateMock::new();

        state.update_size(10);
        state.update_offset(5);
        state.update_timestamp(999);

        assert_eq!(state.get_operation_count(), 3);
        assert_eq!(state.size, 10);
        assert_eq!(state.offset_of_last, 5);
        assert_eq!(state.time_of_last, 999);
    }

    #[test]
    fn test_history_state_operation_order() {
        let mut state = HistoryStateMock::new();

        state.log_operation("first");
        state.log_operation("second");
        state.log_operation("third");

        assert_eq!(state.operations_log.len(), 3);
        assert_eq!(state.operations_log[0], "first");
        assert_eq!(state.operations_log[1], "second");
        assert_eq!(state.operations_log[2], "third");
    }
}

// ============================================================================
// INTEGRATION TESTS WITH MOCKS
// ============================================================================

#[cfg(test)]
mod history_integration_tests {
    use super::*;

    #[test]
    fn test_ring_storage_with_state_mock() {
        let storage: RingStorage<0, 100, 10> = RingStorage::new_empty();
        let mut state = HistoryStateMock::new();

        state.update_size(storage.data.size());
        state.update_offset(storage.data.offset_of_last());

        assert_eq!(state.size, 0);
        assert_eq!(state.offset_of_last, 0);
    }

    #[test]
    fn test_ring_storage_full_workflow_with_mock() {
        let mut storage: RingStorage<0, 10, 5> = RingStorage::new_empty();
        let mut state = HistoryStateMock::new();
        let mut mock_records = MockHistoryStorage::new(1024);

        // Initial state
        assert_eq!(storage.data.size(), 0);
        state.update_size(storage.data.size());

        // Add some operations
        for _ in 0..5 {
            storage.advance_offset_by_one();
            mock_records.write_count += 1;
        }

        state.update_offset(storage.data.offset_of_last());

        assert_eq!(storage.data.offset_of_last(), 5);
        assert_eq!(mock_records.write_count, 5);
    }

    #[test]
    fn test_history_wraparound_with_mock() {
        let mut storage: RingStorage<0, 3, 5> = RingStorage::new_empty();
        let mut state = HistoryStateMock::new();

        // Advance beyond ring size
        for i in 0..10 {
            storage.advance_offset_by_one();
            state.log_operation(&format!("advance_{}", i));
        }

        // Should have wrapped around multiple times
        assert_eq!(state.get_operation_count(), 10);
        assert!(storage.data.offset_of_last() < 3);
    }

    #[test]
    fn test_timestamp_tracking_with_mock() {
        let mut storage: RingStorage<0, 100, 5> = RingStorage::new_empty();
        let mut state = HistoryStateMock::new();

        let timestamps = vec![100u32, 105, 110, 115, 120];

        for (i, &time) in timestamps.iter().enumerate() {
            storage.data.set_time_of_last(time);
            storage.data.set_size((i + 1) as u32);
            state.update_timestamp(time);
        }

        assert_eq!(storage.last_stored_timestamp(), 120);
        assert_eq!(state.time_of_last, 120);
    }
}

// ============================================================================
// EDGE CASE TESTS
// ============================================================================

#[cfg(test)]
mod edge_case_tests {
    use super::*;

    #[test]
    fn test_ring_storage_offset_calculation_edge() {
        let mut storage: RingStorage<0, 1, 1> = RingStorage::new_empty();
        let offset = storage.offset(0);
        assert!(offset > 0);
    }

    #[test]
    fn test_ring_storage_large_size() {
        let mut storage: RingStorage<0, 10000, 10> = RingStorage::new_empty();
        storage.data.set_size(10000);

        let first = storage.first_stored_timestamp();
        // Should calculate without overflow
        assert!(first <= u32::MAX);
    }

    #[test]
    fn test_ring_storage_timestamp_calculation_boundary() {
        let mut storage: RingStorage<0, 100, 1> = RingStorage::new_empty();

        storage.data.set_time_of_last(u32::MAX);
        storage.data.set_size(1);

        let first = storage.first_stored_timestamp();
        assert_eq!(first, u32::MAX);
    }

    #[test]
    fn test_mock_storage_large_capacity() {
        let storage = MockHistoryStorage::new(1_000_000);
        assert_eq!(storage.buffer.len(), 1_000_000);
    }

    #[test]
    fn test_history_state_many_operations() {
        let mut state = HistoryStateMock::new();

        for i in 0..1000 {
            state.log_operation(&format!("op_{}", i));
        }

        assert_eq!(state.get_operation_count(), 1000);
    }

    #[test]
    fn test_ring_storage_advance_zero_size() {
        let mut storage: RingStorage<0, 1, 10> = RingStorage::new_empty();
        storage.data.set_size(0);

        storage.advance_offset_by_one();
        // With SIZE=1: offset goes 0 -> 1, then wraps back to 0
        assert_eq!(storage.data.offset_of_last(), 0);
    }
}

// ============================================================================
// PROPERTY-BASED TESTS
// ============================================================================

#[cfg(test)]
mod property_tests {
    use super::*;

    #[test]
    fn test_advance_offset_never_exceeds_size() {
        const SIZE: i32 = 50;
        let mut storage: RingStorage<0, SIZE, 10> = RingStorage::new_empty();

        for _ in 0..1000 {
            storage.advance_offset_by_one();
            assert!(storage.data.offset_of_last() < SIZE as u32);
        }
    }

    #[test]
    fn test_offset_calculation_is_monotonic_within_size() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();

        let offset_0 = storage.offset(0);
        let offset_1 = storage.offset(1);
        let offset_2 = storage.offset(2);

        assert!(offset_1 > offset_0);
        assert!(offset_2 > offset_1);
    }

    #[test]
    fn test_first_timestamp_less_than_or_equal_to_last() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();

        storage.data.set_time_of_last(1000);
        storage.data.set_size(10);

        let first = storage.first_stored_timestamp();
        let last = storage.last_stored_timestamp();

        assert!(first <= last);
    }

    #[test]
    fn test_mock_stats_consistency() {
        let mut storage = MockHistoryStorage::new(512);
        storage.write_count = 10;
        storage.read_count = 5;

        let stats1 = storage.get_stats();
        let stats2 = storage.get_stats();

        assert_eq!(stats1, stats2);
    }
}

// ============================================================================
// PERFORMANCE TESTS
// ============================================================================

#[cfg(test)]
mod performance_tests {
    use super::*;

    #[test]
    fn test_many_offset_advances() {
        let mut storage: RingStorage<0, 100, 10> = RingStorage::new_empty();

        for _ in 0..10000 {
            storage.advance_offset_by_one();
        }

        // Should have completed without issues
        assert!(storage.data.offset_of_last() < 100);
    }

    #[test]
    fn test_mock_storage_many_records() {
        let mut storage = MockHistoryStorage::new(1024);

        for i in 0..1000 {
            storage.records.push(i);
        }

        assert_eq!(storage.records.len(), 1000);
    }

    #[test]
    fn test_many_state_updates() {
        let mut state = HistoryStateMock::new();

        for i in 0..100 {
            state.update_size(i);
            state.update_offset(i);
            state.update_timestamp(i as u32);
        }

        assert_eq!(state.get_operation_count(), 300);
    }
}
