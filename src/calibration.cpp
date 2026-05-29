/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Calibration math. See calibration.hpp for the model. Algorithm is
 * a 1:1 port of rework/embassy:src/calibration.rs apply_ratio —
 * keep the four-zone structure unchanged; field-deployed units rely
 * on it byte-equivalent.
 */

#include "calibration.hpp"

#include <cmath>

namespace uflow::calibration {

float Calculator::get_raw_volume(const CalibTable& tbl, float tof_up, float tof_down) const
{
	if (!check_tof(static_cast<std::uint32_t>(tof_up)) ||
	    !check_tof(static_cast<std::uint32_t>(tof_down))) {
		return 0.0f;
	}

	const float sum_tof = tof_up + tof_down;
	const float dtof    = tof_up - tof_down;
	const float vm      = ((dtof - tbl.dtof0) * cfg_.const_val) / (sum_tof * sum_tof);
	return vm * 3600.0f;
}

float Calculator::get_volume(const CalibTable& tbl, float tof_up, float tof_down) const
{
	return apply_ratio(tbl, get_raw_volume(tbl, tof_up, tof_down));
}

float Calculator::apply_ratio(const CalibTable& tbl, float val) const
{
	const float k0 = tbl.data[0].k;
	const float k1 = tbl.data[1].k;
	const float k2 = tbl.data[2].k;
	const float v0 = tbl.data[0].v;
	const float v1 = tbl.data[1].v;
	const float v2 = tbl.data[2].v;

	/* Dead zone: positive side of zero, below vmin. */
	if (val < cfg_.vmin && val > cfg_.vneg) {
		return 0.0f;
	}

	/* Zone 1 — |val| < V0: constant K0. */
	if (val < v0 && val > -v0) {
		return val * k0;
	}

	/* Zone 2 — |val| < V1: interpolate K0 → K1. */
	if (val < v1 && val > -v1) {
		const float denom = v1 - v0;
		if (std::fabs(denom) < 1e-7f) {
			return val * k0;
		}
		return val * (k0 + (k1 - k0) * ((val - v0) / denom));
	}

	/* Zone 3 — |val| < V2: interpolate K1 → K2. */
	if (val < v2 && val > -v2) {
		const float denom = v2 - v1;
		if (std::fabs(denom) < 1e-7f) {
			return val * k1;
		}
		return val * (k1 + (k2 - k1) * ((val - v1) / denom));
	}

	/* Zone 4 — within vmax: constant K2. */
	if (val < cfg_.vmax && val > -cfg_.vmax) {
		return val * k2;
	}

	/* Clamp at vmax / -vmax. */
	return (val >= 0.0f) ? cfg_.vmax : -cfg_.vmax;
}

} /* namespace uflow::calibration */
