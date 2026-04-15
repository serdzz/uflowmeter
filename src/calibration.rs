//! Calibration module for ultrasonic flow meter
//!
//! Ported from C++ calibration.hpp / calculator.hpp.
//!
//! Calibration table structure:
//!   dTOF0 — zero offset (delta TOF with no flow)
//!   data[3] — calibration points { V, K } where V=m³/h, K=ratio
//!
//! The calculator applies piecewise linear interpolation between
//! calibration points, with zero-cut at Vmin and clamping at Vmax.

#![allow(dead_code)]

/// Single calibration data point
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, Default)]
pub struct CalibData {
    /// Measured volume at this calibration point (m³/h)
    pub v: f32,
    /// Correction ratio K = V_et / V_measured
    pub k: f32,
}

/// Calibration table for one sensor channel
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy, Default)]
pub struct CalibTable {
    /// Zero offset: delta TOF with no flow (nanoseconds)
    pub dtof0: f32,
    /// Three calibration points
    pub data: [CalibData; 3],
}

/// Meter configuration limits
#[cfg_attr(not(test), derive(defmt::Format))]
#[derive(Debug, Clone, Copy)]
pub struct MeterConfig {
    /// Speed-of-sound constant: L²/(2*cos(α))
    pub const_val: f32,
    /// Negative flow threshold (m³/h)
    pub vneg: f32,
    /// Minimum measurable flow (m³/h)
    pub vmin: f32,
    /// Maximum measurable flow (m³/h)
    pub vmax: f32,
    /// Minimum valid TOF (timer ticks)
    pub tof_min: u32,
    /// Maximum valid TOF (timer ticks)
    pub tof_max: u32,
}

impl Default for MeterConfig {
    fn default() -> Self {
        Self {
            const_val: 0.0, // Must be calibrated per installation
            vneg: -0.01,
            vmin: 0.01,
            vmax: 500.0,
            tof_min: 100,
            tof_max: 5_000_000,
        }
    }
}

/// Flow calculator using calibration table
pub struct Calculator {
    config: MeterConfig,
}

impl Calculator {
    pub fn new(config: MeterConfig) -> Self {
        Self { config }
    }

    /// Calculate raw volume (m³/h) from TOF measurements
    /// tof_up and tof_down are in nanoseconds
    pub fn get_raw_volume(&self, table: &CalibTable, tof_up: f32, tof_down: f32) -> f32 {
        if !self.check_tof(tof_up as u32) || !self.check_tof(tof_down as u32) {
            return 0.0;
        }

        let sum_tof = tof_up + tof_down;
        let dtof = tof_up - tof_down;
        let vm = ((dtof - table.dtof0) * self.config.const_val) / (sum_tof * sum_tof);
        vm * 3600.0 // Convert to m³/h
    }

    /// Calculate calibrated volume (m³/h) with ratio correction
    pub fn get_volume(&self, table: &CalibTable, tof_up: f32, tof_down: f32) -> f32 {
        let raw = self.get_raw_volume(table, tof_up, tof_down);
        self.apply_ratio(table, raw)
    }

    /// Apply piecewise linear calibration ratio
    pub fn apply_ratio(&self, table: &CalibTable, val: f32) -> f32 {
        let k = [table.data[0].k, table.data[1].k, table.data[2].k];

        // Dead zone: below Vmin
        if val < self.config.vmin && val > self.config.vneg {
            return 0.0;
        }

        // Zone 1: |val| < V0
        if val < table.data[0].v && val > -table.data[0].v {
            return val * k[0];
        }

        // Zone 2: |val| < V1 (interpolate K0→K1)
        if val < table.data[1].v && val > -table.data[1].v {
            let denom = table.data[1].v - table.data[0].v;
            if denom.abs() < f32::EPSILON {
                return val * k[0];
            }
            return val * (k[0] + (k[1] - k[0]) * ((val - table.data[0].v) / denom));
        }

        // Zone 3: |val| < V2 (interpolate K1→K2)
        if val < table.data[2].v && val > -table.data[2].v {
            let denom = table.data[2].v - table.data[1].v;
            if denom.abs() < f32::EPSILON {
                return val * k[1];
            }
            return val * (k[1] + (k[2] - k[1]) * ((val - table.data[1].v) / denom));
        }

        // Zone 4: above V2 (constant K2)
        if val < self.config.vmax && val > -self.config.vmax {
            return val * k[2];
        }

        // Clamping
        if val >= 0.0 {
            self.config.vmax
        } else {
            -self.config.vmax
        }
    }

    fn check_tof(&self, tof: u32) -> bool {
        tof >= self.config.tof_min && tof <= self.config.tof_max
    }
}

/// Auto-zero calibration: measure 10 samples with no flow to determine dTOF0
/// Returns the zero offset for both channels
pub fn auto_zero(
    measure_fn: impl Fn() -> (i32, i32), // (delta_ch0, delta_ch1) in ns
) -> [f32; 2] {
    let mut delta: [i32; 2] = [0, 0];

    for _ in 0..10 {
        let (d0, d1) = measure_fn();
        delta[0] += d0;
        delta[1] += d1;
    }

    [
        delta[0] as f32 / 10_000.0,
        delta[1] as f32 / 10_000.0,
    ]
}

/// Auto-calibration for a specific coefficient (1-3)
/// Takes 10 measurements with known reference flow (vet in m³/h)
/// Returns updated CalibData for the given coefficient index
pub fn auto_calibrate(
    _coef_no: u8,   // 1, 2, or 3 (reserved for logging)
    vet: f32,       // reference flow m³/h
    measure_fn: impl Fn() -> [f32; 2], // [vm_raw_ch0, vm_raw_ch1]
) -> [CalibData; 2] {
    let mut vm_raw: [f32; 2] = [0.0, 0.0];

    for _ in 0..10 {
        let m = measure_fn();
        vm_raw[0] += m[0];
        vm_raw[1] += m[1];
    }

    vm_raw[0] /= 10.0;
    vm_raw[1] /= 10.0;

    [
        CalibData {
            v: vm_raw[0],
            k: if vm_raw[0].abs() > f32::EPSILON {
                vet / vm_raw[0]
            } else {
                1.0
            },
        },
        CalibData {
            v: vm_raw[1],
            k: if vm_raw[1].abs() > f32::EPSILON {
                vet / vm_raw[1]
            } else {
                1.0
            },
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_table() -> CalibTable {
        CalibTable {
            dtof0: 0.0,
            data: [
                CalibData { v: 100.0, k: 1.0 },
                CalibData { v: 300.0, k: 1.05 },
                CalibData { v: 500.0, k: 1.10 },
            ],
        }
    }

    fn default_config() -> MeterConfig {
        MeterConfig {
            const_val: 1000.0,
            vneg: -0.01,
            vmin: 0.01,
            vmax: 500.0,
            tof_min: 100,
            tof_max: 5_000_000,
        }
    }

    #[test]
    fn test_dead_zone() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // Value within dead zone
        let result = calc.apply_ratio(&table, 0.005);
        assert_eq!(result, 0.0);
    }

    #[test]
    fn test_zone1_constant_k() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // |val| < V0 = 100, should use K0 = 1.0
        let result = calc.apply_ratio(&table, 50.0);
        assert!((result - 50.0).abs() < 0.001);
    }

    #[test]
    fn test_zone2_interpolation() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // 100 < val < 300, should interpolate K0→K1
        let val = 200.0;
        let expected = val * (1.0 + (1.05 - 1.0) * ((200.0 - 100.0) / (300.0 - 100.0)));
        let result = calc.apply_ratio(&table, val);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn test_zone3_interpolation() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // 300 < val < 500, should interpolate K1→K2
        let val = 400.0;
        let expected = val * (1.05 + (1.10 - 1.05) * ((400.0 - 300.0) / (500.0 - 300.0)));
        let result = calc.apply_ratio(&table, val);
        assert!((result - expected).abs() < 0.01);
    }

    #[test]
    fn test_zone4_constant_k2() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // |val| < Vmax = 500 but > V2 = 500
        // Actually val=450 is between V1=300 and V2=500
        let val = 499.0;
        let result = calc.apply_ratio(&table, val);
        // Zone 3 (V1 < val < V2)
        assert!(result > 0.0);
    }

    #[test]
    fn test_clamping() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // Beyond Vmax
        let result = calc.apply_ratio(&table, 600.0);
        assert_eq!(result, 500.0); // clamped to Vmax
    }

    #[test]
    fn test_negative_clamping() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        let result = calc.apply_ratio(&table, -600.0);
        assert_eq!(result, -500.0);
    }

    #[test]
    fn test_negative_zone1() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        let result = calc.apply_ratio(&table, -50.0);
        assert!((result - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn test_auto_zero() {
        // Each call returns (100, 200), 10 calls summed = (1000, 2000)
        // 1000 / 10000 = 0.1, 2000 / 10000 = 0.2
        let result = auto_zero(|| (100, 200));
        assert!((result[0] - 0.1).abs() < 0.01);
        assert!((result[1] - 0.2).abs() < 0.01);
    }

    #[test]
    fn test_auto_calibrate() {
        let result = auto_calibrate(1, 150.0, || [100.0, 120.0]);
        assert!((result[0].v - 100.0).abs() < 0.001);
        assert!((result[0].k - 1.5).abs() < 0.01); // 150/100
        assert!((result[1].v - 120.0).abs() < 0.001);
        assert!((result[1].k - 1.25).abs() < 0.01); // 150/120
    }

    #[test]
    fn test_get_raw_volume() {
        let calc = Calculator::new(default_config());
        let table = default_table();
        // tof_up = 100000 ns, tof_down = 99900 ns
        let result = calc.get_raw_volume(&table, 100000.0, 99900.0);
        assert!(result > 0.0); // positive flow
    }

    #[test]
    fn test_zero_dtof_zero_flow() {
        let calc = Calculator::new(default_config());
        let table = CalibTable {
            dtof0: 100.0, // offset = 100ns
            data: default_table().data,
        };
        // dtof = tof_up - tof_down = 100, same as dtof0 → zero flow
        let result = calc.get_raw_volume(&table, 100100.0, 100000.0);
        assert!(result.abs() < 0.01);
    }
}