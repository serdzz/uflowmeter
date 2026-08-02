//! Tests for the moving-average window.

use crate::average_lib::*;

#[test]
fn empty_buffer_has_no_average() {
    let b = AverageBuffer::new();
    assert!(b.is_empty());
    assert_eq!(b.len(), 0);
    assert_eq!(b.average(), None);
    assert_eq!(b.total(), 0.0);
}

#[test]
fn averages_only_the_populated_slots() {
    let mut b = AverageBuffer::new();
    b.push(3.0);
    b.push(5.0);
    // Divides by 2, not by the full window — otherwise a fresh meter
    // would report a fifth of the real flow until the window filled.
    assert_eq!(b.len(), 2);
    assert_eq!(b.average(), Some(4.0));
}

#[test]
fn window_saturates_and_evicts_the_oldest() {
    let mut b = AverageBuffer::new();
    for i in 0..WINDOW {
        b.push(i as f32);
    }
    assert_eq!(b.len(), WINDOW);
    // 0..9 sums to 45.
    assert_eq!(b.average(), Some(4.5));

    // One more push drops the 0 and appends 10.
    b.push(10.0);
    assert_eq!(b.len(), WINDOW);
    assert_eq!(b.average(), Some(5.5));
}

#[test]
fn eviction_wraps_more_than_once() {
    let mut b = AverageBuffer::new();
    for _ in 0..WINDOW * 3 {
        b.push(1.0);
    }
    b.push(11.0);
    // Nine 1.0s plus one 11.0.
    assert_eq!(b.average(), Some(2.0));
}

#[test]
fn push_pair_stores_one_sample_not_two() {
    let mut b = AverageBuffer::new();
    b.push_pair(2.0, 4.0);
    assert_eq!(b.len(), 1, "a sensor pair is one reading");
    assert_eq!(b.average(), Some(3.0));
}

#[test]
fn handles_negative_flow() {
    // Reverse flow is legitimate — the calibration table has a Vneg
    // bound, so the window must not clamp it.
    let mut b = AverageBuffer::new();
    b.push(-2.0);
    b.push(1.0);
    assert_eq!(b.average(), Some(-0.5));
}
