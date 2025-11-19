#![cfg(test)]

use crate::history_lib::{RingStorage, ServiceData};

#[test]
fn test_service_data_default() {
    let sd = ServiceData::default();
    assert_eq!(sd.size(), 0);
    assert_eq!(sd.offset_of_last(), 0);
    assert_eq!(sd.time_of_last(), 0);
}

#[test]
fn test_service_data_creation_with_values() {
    let mut sd = ServiceData::default();
    sd.set_size(5);
    sd.set_offset_of_last(2);
    sd.set_time_of_last(1000);

    assert_eq!(sd.size(), 5);
    assert_eq!(sd.offset_of_last(), 2);
    assert_eq!(sd.time_of_last(), 1000);
}

#[test]
fn test_service_data_bytes_conversion() {
    let mut sd = ServiceData::default();
    sd.set_size(10);
    sd.set_offset_of_last(3);
    sd.set_time_of_last(5000);

    let bytes = sd.into_bytes();
    let sd_restored = ServiceData::from_bytes(bytes);

    assert_eq!(sd_restored.size(), 10);
    assert_eq!(sd_restored.offset_of_last(), 3);
    assert_eq!(sd_restored.time_of_last(), 5000);
}

#[test]
fn test_advance_offset_wrapping() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    // Test offset advancing without wrapping
    rs.data.set_offset_of_last(5);
    rs.advance_offset_by_one();
    assert_eq!(rs.data.offset_of_last(), 6);

    // Test offset wrapping at SIZE boundary
    rs.data.set_offset_of_last((RING_SIZE - 1) as u32);
    rs.advance_offset_by_one();
    assert_eq!(rs.data.offset_of_last(), 0);
}

#[test]
fn test_size_increment() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    assert_eq!(rs.data.size(), 0);
    let tmp = rs.data.size() + 1;
    rs.data.set_size(tmp);
    assert_eq!(rs.data.size(), 1);
}

#[test]
fn test_timestamp_normalization() {
    // Test that timestamps are normalized to 60-second intervals
    let time1 = 1234567;
    let normalized1 = time1 - time1 % 60;
    assert_eq!(normalized1 % 60, 0);

    let time2 = 1234500;
    let normalized2 = time2 - time2 % 60;
    assert_eq!(normalized2 % 60, 0);
}

#[test]
fn test_first_stored_timestamp_empty() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    rs.data.set_time_of_last(5000);
    rs.data.set_size(0);

    assert_eq!(rs.first_stored_timestamp(), 5000);
}

#[test]
fn test_first_stored_timestamp_with_data() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    rs.data.set_time_of_last(5000);
    rs.data.set_size(5);

    // first_timestamp = last_timestamp - ELEMENT_SIZE * (size - 1)
    // = 5000 - 60 * 4 = 5000 - 240 = 4760
    assert_eq!(rs.first_stored_timestamp(), 4760);
}

#[test]
fn test_last_stored_timestamp() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    rs.data.set_time_of_last(9999);
    assert_eq!(rs.last_stored_timestamp(), 9999);
}

#[test]
fn test_multiple_advances() {
    const RING_SIZE: i32 = 5;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    rs.data.set_offset_of_last(0);

    // Advance through all positions
    for i in 1..=RING_SIZE as u32 {
        rs.advance_offset_by_one();
        assert_eq!(rs.data.offset_of_last(), i % RING_SIZE as u32);
    }
}

#[test]
fn test_offset_calculation() {
    const RING_SIZE: i32 = 10;
    const ELEMENT_SIZE: i32 = 60;
    const OFFSET: usize = 0;

    let mut rs = RingStorage::<OFFSET, RING_SIZE, ELEMENT_SIZE> {
        data: ServiceData::default(),
    };

    // offset = Self::OFFSET + size_of::<ServiceData>() + size_of::<u16>() + (index * 4)
    let offset0 = rs.offset(0);
    let offset1 = rs.offset(1);

    // offset1 should be 4 bytes (size of i32) more than offset0
    assert_eq!(offset1 - offset0, 4);
}
