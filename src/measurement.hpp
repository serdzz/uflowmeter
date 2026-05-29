/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Background flow-measurement thread.
 *
 * Runs a 5-second cycle:
 *   1. Power up TDC1000 + TDC7200.
 *   2. Wait ~1 ms for regulator settle.
 *   3. Reload register configs from Options (chips lose state at
 *      power-off).
 *   4. Clear TDC1000 error flags.
 *   5. Channel 0 (downstream): start measurement, wait for INT,
 *      read TIME1.
 *   6. Channel 1 (upstream): same.
 *   7. Power both chips off.
 *   8. Run the Calculator over (tof_up, tof_down) → m³/h.
 *   9. Publish to `latest_flow_m3h` + update LCD row 1 under the
 *      shared LCD mutex.
 *
 * Calibration data is captured from `options::g_options` on each
 * cycle. Live-reload from UI/Modbus writes works automatically as
 * soon as g_options is updated atomically (today: only main writes,
 * only at boot — see options.hpp note).
 *
 * Thread sizing: 1 KB stack is enough — calculator + SPI + LCD ops
 * are all small. Priority is preemptible at K_PRIO_PREEMPT(7) — well
 * below main (priority 0), so keypress feedback is never delayed by
 * a measurement cycle.
 */

#pragma once

#include <atomic>

namespace uflow::measurement {

/* Latest flow reading, m³/h. NaN until the first successful cycle. */
extern std::atomic<float> latest_flow_m3h;

/* Spawn the measurement thread. Idempotent — only the first call
 * actually starts the thread; subsequent calls return 0. Returns 0 on
 * success, negative errno if hardware init failed. */
int start();

} /* namespace uflow::measurement */
