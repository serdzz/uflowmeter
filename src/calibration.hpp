/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Ultrasonic flow-rate calibration.
 *
 * Direct port of rework/embassy:src/calibration.rs (which itself was a
 * port of the legacy C++ calibration.hpp / calculator.hpp). No
 * runtime dependencies — pure floating-point math.
 *
 * Concept:
 *   - Each measurement cycle gives us a pair of times-of-flight
 *     (upstream + downstream) in nanoseconds.
 *   - dTOF = tof_up - tof_down, sum = tof_up + tof_down.
 *   - Raw volume = ((dTOF - dTOF0) * const_val) / sum² × 3600  [m³/h]
 *     where const_val = L² / (2·cos α) (pipe geometry).
 *   - Calibrated volume applies a piecewise-linear correction K
 *     across three reference volumes V0 < V1 < V2.
 *
 * The piecewise-linear table mirrors the production C++ field
 * units' calibration format — do not change without re-calibrating
 * existing units (the table is persisted in EEPROM Options).
 */

#pragma once

#include <cstdint>

namespace uflow::calibration {

struct CalibData {
	float v;  /* m³/h at this calibration point */
	float k;  /* correction ratio = V_reference / V_measured */
};

struct CalibTable {
	float dtof0;            /* zero offset on dTOF (ns), per channel */
	CalibData data[3];      /* three calibration points V0 < V1 < V2 */
};

struct MeterConfig {
	float const_val;        /* speed-of-sound geometry constant L²/(2·cos α) */
	float vneg;             /* negative-flow threshold (m³/h) */
	float vmin;             /* dead-zone lower bound (m³/h) */
	float vmax;             /* clamp ceiling (m³/h) */
	std::uint32_t tof_min;  /* lower TOF validity bound (timer ticks) */
	std::uint32_t tof_max;  /* upper TOF validity bound */

	/* Defaults match Rust MeterConfig::default. Uncalibrated unit
	 * has const_val=0 → calculator returns 0, matching the production
	 * "no flow until calibrated" behavior. */
	static constexpr MeterConfig defaults()
	{
		return MeterConfig{
			.const_val = 0.0f,
			.vneg      = -0.01f,
			.vmin      = 0.01f,
			.vmax      = 500.0f,
			.tof_min   = 100,
			.tof_max   = 5'000'000,
		};
	}
};

class Calculator {
public:
	explicit Calculator(MeterConfig cfg) : cfg_{cfg} {}

	/* Raw flow rate from a single TOF pair (m³/h). Returns 0.0 if
	 * either TOF falls outside [tof_min, tof_max]. */
	float get_raw_volume(const CalibTable& tbl, float tof_up, float tof_down) const;

	/* Calibrated flow rate — applies the piecewise-linear K
	 * correction on top of the raw volume. */
	float get_volume(const CalibTable& tbl, float tof_up, float tof_down) const;

	/* Apply the K table to a pre-computed raw volume. Exposed
	 * separately so callers can run it on cached values without
	 * re-doing the TOF math. */
	float apply_ratio(const CalibTable& tbl, float val) const;

	const MeterConfig& config() const { return cfg_; }

private:
	bool check_tof(std::uint32_t tof) const
	{
		return tof >= cfg_.tof_min && tof <= cfg_.tof_max;
	}

	MeterConfig cfg_;
};

} /* namespace uflow::calibration */
