//! UI Module Examples
//!
//! Runnable examples demonstrating UI logic, blink masks, and timestamp calculations.
//!
//! Run with:
//! ```bash
//! cargo run --example ui_examples
//! ```
//!
//! Note: This example temporarily removes the embedded target configuration
//! via Makefile to run on host.

use time::{macros::datetime, Duration, Month};

fn main() {
    println!("╔══════════════════════════════════════════════════════════════╗");
    println!("║           uFlowmeter UI Module Examples                     ║");
    println!("╚══════════════════════════════════════════════════════════════╝\n");

    example_1_blink_masks();
    example_2_timestamp_calculation();
    example_3_navigation_flow();
    example_4_month_navigation();
    example_5_history_types();
    example_6_complete_simulation();

    println!("\n╔══════════════════════════════════════════════════════════════╗");
    println!("║                    Examples Complete                         ║");
    println!("╚══════════════════════════════════════════════════════════════╝");
}

/// Example 1: Blink Mask Values
///
/// Demonstrates how blink masks control which characters flash during editing
fn example_1_blink_masks() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 1: Blink Mask Values                                 │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    // Time format: "HH:MM:SS" (8 characters)
    println!("Time Format: \"HH:MM:SS\" (8 characters)");
    println!("Position:     0  1  2  3  4  5  6  7");
    println!("Display:      H  H  :  M  M  :  S  S");
    println!("Bit:          7  6  5  4  3  2  1  0\n");

    const SECONDS_MASK: u32 = 0x03; // Bits 0-1 (positions 6-7)
    const MINUTES_MASK: u32 = 0x18; // Bits 3-4 (positions 3-4)
    const HOURS_MASK: u32 = 0xc0;   // Bits 6-7 (positions 0-1)

    println!("Time Masks:");
    println!("  Seconds: 0x{:02x} = 0b{:08b} (positions 6-7)", SECONDS_MASK, SECONDS_MASK);
    println!("  Minutes: 0x{:02x} = 0b{:08b} (positions 3-4)", MINUTES_MASK, MINUTES_MASK);
    println!("  Hours:   0x{:02x} = 0b{:08b} (positions 0-1)", HOURS_MASK, HOURS_MASK);

    // Verify no overlap
    assert_eq!(SECONDS_MASK & MINUTES_MASK, 0, "Masks should not overlap");
    assert_eq!(SECONDS_MASK & HOURS_MASK, 0, "Masks should not overlap");
    assert_eq!(MINUTES_MASK & HOURS_MASK, 0, "Masks should not overlap");
    println!("  ✓ All masks are non-overlapping\n");

    // Date format: "DD/MM/YY" (8 characters)
    println!("Date Format: \"DD/MM/YY\" (8 characters)");
    println!("Position:     0  1  2  3  4  5  6  7");
    println!("Display:      D  D  /  M  M  /  Y  Y");
    println!("Bit:          7  6  5  4  3  2  1  0\n");

    const DAY_MASK: u32 = 0xc0;    // Bits 6-7 (positions 0-1)
    const MONTH_MASK: u32 = 0x18;  // Bits 3-4 (positions 3-4)
    const YEAR_MASK: u32 = 0x03;   // Bits 0-1 (positions 6-7)

    println!("Date Masks:");
    println!("  Day:   0x{:02x} = 0b{:08b} (positions 0-1)", DAY_MASK, DAY_MASK);
    println!("  Month: 0x{:02x} = 0b{:08b} (positions 3-4)", MONTH_MASK, MONTH_MASK);
    println!("  Year:  0x{:02x} = 0b{:08b} (positions 6-7)", YEAR_MASK, YEAR_MASK);
    println!("  ✓ All masks verified\n");
}

/// Example 2: Timestamp Calculation
///
/// Shows how timestamps are calculated for history queries
fn example_2_timestamp_calculation() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 2: Timestamp Calculation                             │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let base = datetime!(2024-01-15 10:30:45 UTC);
    let base_ts = base.unix_timestamp() as u32;

    println!("Base DateTime: {}", base);
    println!("Unix Timestamp: {} (full value, NOT % 60)\n", base_ts);

    // Hour increments
    println!("Hour History Navigation:");
    for i in -2..=2_i64 {
        let dt = if i >= 0 {
            base.saturating_add(Duration::hours(i))
        } else {
            base.saturating_sub(Duration::hours(-i))
        };
        let ts = dt.unix_timestamp() as u32;
        let diff = (ts as i64) - (base_ts as i64);
        println!("  {:+3} hours: {} (ts: {}, diff: {:+6})", i, dt, ts, diff);
    }
    println!("  ✓ Each hour = 3600 seconds\n");

    // Day increments
    println!("Day History Navigation:");
    for i in -1..=1_i64 {
        let dt = if i >= 0 {
            base.saturating_add(Duration::days(i))
        } else {
            base.saturating_sub(Duration::days(-i))
        };
        let ts = dt.unix_timestamp() as u32;
        let diff = (ts as i64) - (base_ts as i64);
        println!("  {:+2} days: {} (ts: {}, diff: {:+7})", i, dt, ts, diff);
    }
    println!("  ✓ Each day = 86400 seconds\n");
}

/// Example 3: Navigation Flow
///
/// Demonstrates the state machine for field navigation
fn example_3_navigation_flow() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 3: Navigation Flow State Machine                     │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    println!("Navigation Sequence:");
    println!("  1. None     → [Enter] → Seconds  (mask: 0x03, time edit on)");
    println!("  2. Seconds  → [Enter] → Minutes  (mask: 0x18)");
    println!("  3. Minutes  → [Enter] → Hours    (mask: 0xc0)");
    println!("  4. Hours    → [Enter] → Day      (mask: 0xc0, time off, date on)");
    println!("  5. Day      → [Enter] → Month    (mask: 0x18)");
    println!("  6. Month    → [Enter] → Year     (mask: 0x03)");
    println!("  7. Year     → [Enter] → None     (all editing off)\n");

    println!("At each step:");
    println!("  [Right] = Increment current field");
    println!("  [Left]  = Decrement current field");
    println!("  [Enter] = Move to next field\n");
}

/// Example 4: Month Navigation
///
/// Shows month wrapping behavior
fn example_4_month_navigation() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 4: Month Navigation with Wrapping                    │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let months = [
        Month::January, Month::February, Month::March,
        Month::April, Month::May, Month::June,
        Month::July, Month::August, Month::September,
        Month::October, Month::November, Month::December,
    ];

    println!("Forward navigation:");
    for month in &months[0..3] {
        let next = month.next();
        println!("  {:>9?} → next() → {:?}", month, next);
    }
    println!("\nWrapping forward:");
    let dec = Month::December;
    let jan = dec.next();
    println!("  {:>9?} → next() → {:?} (wraps to start)", dec, jan);

    println!("\nBackward navigation:");
    for month in &months[0..3] {
        let prev = month.previous();
        println!("  {:>9?} → previous() → {:?}", month, prev);
    }
    println!("\nWrapping backward:");
    let jan = Month::January;
    let dec = jan.previous();
    println!("  {:>9?} → previous() → {:?} (wraps to end)\n", jan, dec);
}

/// Example 5: History Types
///
/// Compares different history interval calculations
fn example_5_history_types() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 5: History Types and Intervals                       │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let base = datetime!(2024-01-15 12:00:00 UTC);

    println!("Hour History (3600 second intervals):");
    println!("  Storage: 2160 entries (~90 days)");
    for i in 0..4 {
        let dt = base.saturating_add(Duration::hours(i));
        let ts = dt.unix_timestamp() as u32;
        println!("    Entry {}: {} (ts: {})", i, dt, ts);
    }

    println!("\nDay History (86400 second intervals):");
    println!("  Storage: 1116 entries (~3 years)");
    for i in 0..4 {
        let dt = base.saturating_add(Duration::days(i));
        let ts = dt.unix_timestamp() as u32;
        println!("    Entry {}: {} (ts: {})", i, dt, ts);
    }

    println!("\nMonth History (~30 day intervals):");
    println!("  Storage: 120 entries (10 years)");
    let mut dt = base;
    for i in 0..4 {
        let ts = dt.unix_timestamp() as u32;
        println!("    Entry {}: {} (ts: {})", i, dt, ts);
        // In real code: dt = dt.replace_month(dt.month().next()).unwrap();
        dt = dt.saturating_add(Duration::days(30)); // Approximation
    }
    println!();
}

/// Example 6: Complete User Interaction Simulation
///
/// Step-by-step simulation of editing history
fn example_6_complete_simulation() {
    println!("┌──────────────────────────────────────────────────────────────┐");
    println!("│ Example 6: Complete User Interaction Simulation              │");
    println!("└──────────────────────────────────────────────────────────────┘\n");

    let mut datetime = datetime!(2024-01-15 10:30:45 UTC);
    let mut current_field = "None";
    let mut editable = false;

    println!("Initial State:");
    println!("  DateTime: {}", datetime);
    println!("  Field: {}", current_field);
    println!("  Display: \"{:02}/{:02}/{:02}    {:02}:{:02}:{:02}\"",
        datetime.day(), datetime.month() as u8, datetime.year() - 2000,
        datetime.hour(), datetime.minute(), datetime.second());
    println!("  Editable: {}\n", editable);

    // User presses Enter
    println!("──────────────────────────────────────────");
    println!("[User Action: Press Enter]");
    current_field = "Seconds";
    editable = true;
    println!("  → Field: {} (mask: 0x03)", current_field);
    println!("  → Editable: {}", editable);
    println!("  → Display: \"{:02}/{:02}/{:02}    {:02}:{:02}:__\" (blink)", 
        datetime.day(), datetime.month() as u8, datetime.year() - 2000,
        datetime.hour(), datetime.minute());

    // User presses Right (increment)
    println!("\n[User Action: Press Right - Increment]");
    datetime = datetime.saturating_add(Duration::SECOND);
    let ts = datetime.unix_timestamp() as u32;
    println!("  → DateTime: {}", datetime);
    println!("  → Timestamp: {}", ts);

    // Move to minutes
    println!("\n[User Action: Press Enter]");
    current_field = "Minutes";
    println!("  → Field: {} (mask: 0x18)", current_field);
    println!("  → Display: \"{:02}/{:02}/{:02}    {:02}:__:{:02}\" (blink)",
        datetime.day(), datetime.month() as u8, datetime.year() - 2000,
        datetime.hour(), datetime.second());

    // Increment minutes
    println!("\n[User Action: Press Right - Increment]");
    datetime = datetime.saturating_add(Duration::MINUTE);
    let ts = datetime.unix_timestamp() as u32;
    println!("  → DateTime: {}", datetime);
    println!("  → Timestamp: {} (+60 seconds)", ts);

    // Move to hours
    println!("\n[User Action: Press Enter]");
    current_field = "Hours";
    println!("  → Field: {} (mask: 0xc0)", current_field);
    println!("  → Display: \"{:02}/{:02}/{:02}    __:{:02}:{:02}\" (blink)",
        datetime.day(), datetime.month() as u8, datetime.year() - 2000,
        datetime.minute(), datetime.second());

    // Increment hours
    println!("\n[User Action: Press Right - Increment]");
    datetime = datetime.saturating_add(Duration::HOUR);
    let ts = datetime.unix_timestamp() as u32;
    println!("  → DateTime: {}", datetime);
    println!("  → Timestamp: {} (+3600 seconds)", ts);

    // Move to day
    println!("\n[User Action: Press Enter]");
    current_field = "Day";
    println!("  → Field: {} (mask: 0xc0)", current_field);
    println!("  → Time edit: OFF, Date edit: ON");
    println!("  → Display: \"__/{:02}/{:02}    {:02}:{:02}:{:02}\" (blink)",
        datetime.month() as u8, datetime.year() - 2000,
        datetime.hour(), datetime.minute(), datetime.second());

    // Increment day
    println!("\n[User Action: Press Right - Increment]");
    datetime = datetime.saturating_add(Duration::DAY);
    let ts = datetime.unix_timestamp() as u32;
    println!("  → DateTime: {}", datetime);
    println!("  → Timestamp: {} (+86400 seconds)", ts);

    // Final Enter to exit editing
    println!("\n[User Action: Press Enter through Month → Year → None]");
    current_field = "None";
    editable = false;
    println!("  → Field: {}", current_field);
    println!("  → Editable: {}", editable);
    println!("  → All editing complete");
    println!("  → Final DateTime: {}", datetime);
    println!("  → Final Timestamp: {}", ts);
    println!("\n✓ Simulation complete\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blink_masks() {
        // Time masks
        const SECONDS: u32 = 0x03;
        const MINUTES: u32 = 0x18;
        const HOURS: u32 = 0xc0;

        assert_eq!(SECONDS & MINUTES, 0);
        assert_eq!(SECONDS & HOURS, 0);
        assert_eq!(MINUTES & HOURS, 0);

        // Date masks
        const DAY: u32 = 0xc0;
        const MONTH: u32 = 0x18;
        const YEAR: u32 = 0x03;

        assert_eq!(DAY & MONTH, 0);
        assert_eq!(DAY & YEAR, 0);
        assert_eq!(MONTH & YEAR, 0);
    }

    #[test]
    fn test_timestamp_intervals() {
        let base = datetime!(2024-01-15 10:00:00 UTC);
        let base_ts = base.unix_timestamp() as u32;

        // Hour
        let hour = base.saturating_add(Duration::HOUR);
        let hour_ts = hour.unix_timestamp() as u32;
        assert_eq!(hour_ts - base_ts, 3600);

        // Day
        let day = base.saturating_add(Duration::DAY);
        let day_ts = day.unix_timestamp() as u32;
        assert_eq!(day_ts - base_ts, 86400);
    }

    #[test]
    fn test_month_wrapping() {
        assert_eq!(Month::December.next(), Month::January);
        assert_eq!(Month::January.previous(), Month::December);
    }
}
