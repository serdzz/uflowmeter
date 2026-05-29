/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Calibration tests. Port of the test_* functions in
 * rework/embassy:src/calibration.rs (#[cfg(test)] mod tests block).
 */

#include "framework.hpp"

#include "../src/calibration.hpp"

using uflow::calibration::CalibData;
using uflow::calibration::CalibTable;
using uflow::calibration::Calculator;
using uflow::calibration::MeterConfig;

namespace {

CalibTable default_table()
{
	return CalibTable{
		.dtof0 = 0.0f,
		.data  = {
			CalibData{ 100.0f, 1.0f  },
			CalibData{ 300.0f, 1.05f },
			CalibData{ 500.0f, 1.10f },
		},
	};
}

MeterConfig default_config()
{
	return MeterConfig{
		.const_val = 1000.0f,
		.vneg      = -0.01f,
		.vmin      =  0.01f,
		.vmax      =  500.0f,
		.tof_min   =  100,
		.tof_max   =  5'000'000,
	};
}

} /* namespace */

TEST(dead_zone)
{
	Calculator calc(default_config());
	const float r = calc.apply_ratio(default_table(), 0.005f);
	ASSERT_NEAR(r, 0.0, 1e-6);
}

TEST(zone1_constant_k)
{
	Calculator calc(default_config());
	/* |val| < V0=100, K0=1.0 → identity. */
	const float r = calc.apply_ratio(default_table(), 50.0f);
	ASSERT_NEAR(r, 50.0, 1e-3);
}

TEST(zone2_interpolation)
{
	Calculator calc(default_config());
	const float val = 200.0f;
	/* Interpolate K0..K1 across V0..V1 at val=200 (midpoint).
	 * expected = val * (K0 + (K1-K0) * (val-V0)/(V1-V0))
	 *          = 200 * (1.0 + 0.05 * (100/200)) = 200 * 1.025 = 205. */
	const float r = calc.apply_ratio(default_table(), val);
	ASSERT_NEAR(r, 205.0, 0.01);
}

TEST(zone3_interpolation)
{
	Calculator calc(default_config());
	const float val = 400.0f;
	/* expected = 400 * (1.05 + 0.05 * (100/200)) = 400 * 1.075 = 430. */
	const float r = calc.apply_ratio(default_table(), val);
	ASSERT_NEAR(r, 430.0, 0.01);
}

TEST(clamp_at_vmax)
{
	Calculator calc(default_config());
	const float r = calc.apply_ratio(default_table(), 600.0f);
	ASSERT_NEAR(r, 500.0, 1e-3);  /* clamped to vmax */
}

TEST(clamp_at_neg_vmax)
{
	Calculator calc(default_config());
	const float r = calc.apply_ratio(default_table(), -600.0f);
	ASSERT_NEAR(r, -500.0, 1e-3);
}

TEST(negative_zone1)
{
	Calculator calc(default_config());
	const float r = calc.apply_ratio(default_table(), -50.0f);
	ASSERT_NEAR(r, -50.0, 1e-3);
}

TEST(get_raw_volume_positive)
{
	Calculator calc(default_config());
	const float r = calc.get_raw_volume(default_table(), 100000.0f, 99900.0f);
	ASSERT_TRUE(r > 0.0f);
}

TEST(zero_dtof0_offset_yields_zero_flow)
{
	Calculator calc(default_config());
	CalibTable t = default_table();
	t.dtof0 = 100.0f;  /* offset = 100ns */
	/* dtof = up - down = 100, equals dtof0 → zero flow. */
	const float r = calc.get_raw_volume(t, 100100.0f, 100000.0f);
	ASSERT_NEAR(r, 0.0, 0.01);
}

TEST(invalid_tof_returns_zero)
{
	Calculator calc(default_config());
	/* tof below tof_min=100 → reject. */
	const float r = calc.get_raw_volume(default_table(), 50.0f, 50.0f);
	ASSERT_NEAR(r, 0.0, 1e-6);
}

TEST(get_volume_combines_raw_and_ratio)
{
	Calculator calc(default_config());
	/* Raw + ratio: should match raw scaled through apply_ratio. */
	const float raw = calc.get_raw_volume(default_table(), 100050.0f, 99950.0f);
	const float combined = calc.get_volume(default_table(), 100050.0f, 99950.0f);
	const float expected = calc.apply_ratio(default_table(), raw);
	ASSERT_NEAR(combined, expected, 1e-3);
}
