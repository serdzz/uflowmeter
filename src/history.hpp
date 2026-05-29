/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * History query interface — abstracts the three EEPROM ring buffers
 * (Hour / Day / Month accumulators) behind a single
 * "give me the flow value at timestamp T" call.
 *
 * Backing in THIS commit is a stub: query() always returns nullopt
 * and last_result() always reports kind/ts mismatch. The HistoryWidget
 * UI works end-to-end against this stub (user navigates dates, screen
 * shows "None"). The real ring lookup lands in the history-rings
 * commit; the API here is intentionally stable so the swap is
 * source-only.
 *
 * Source for the eventual ring layout:
 *   git show rework/embassy:src/history_lib.rs
 *     - HourHistory:  2160 slots × 3600 s ≈ 90 days
 *     - DayHistory:   1116 slots × 86400 s ≈ 3 years
 *     - MonthHistory:  120 slots × 2592000 s ≈ 10 years
 */

#pragma once

#include <cstdint>
#include <optional>

struct device;

namespace uflow::history {

enum class HistoryType : std::uint8_t {
	Hour = 0,
	Day,
	Month,
};

struct QueryResult {
	HistoryType type;
	std::uint32_t timestamp;            /* seconds since 2000-01-01 */
	std::optional<float> flow;          /* m³ accumulated in slot, or nullopt */
};

/* Look up the slot containing `timestamp` for `type`. Stub today
 * always returns nullopt; preserved interface for the real ring
 * impl. */
std::optional<float> query(HistoryType type, std::uint32_t timestamp);

/* Last computed result — populated by main's SetHistory dispatcher
 * via set_last_result; consumed by the renderer. Returns the
 * default-constructed QueryResult before the first query. */
const QueryResult& last_result();

/* Update the cached last_result. Called by main after each query()
 * so the renderer can read the value on the next refresh tick. */
void set_last_result(const QueryResult& r);

/* Load all three ring buffers from EEPROM. Call once at boot AFTER
 * Options load + BEFORE EEPROM enters deep-power-down. Returns 0 on
 * success — CRC mismatches start that ring fresh (harmless). */
int init(const struct device* eeprom);

/* Spawn the history_tick thread. Idempotent. Reads
 * measurement::latest_flow_m3h every 60 s, accumulates into hour/day/
 * month buckets, writes to the corresponding ring on minute=0 /
 * hour=0 / day=1 boundaries. */
int start();

/* In-flight accumulators — the running sum since the last ring write
 * for each kind. Single-writer (history_tick thread); reads are
 * lock-free and may see a slightly stale value. Used by the Modbus
 * register handler to expose hour/day/month flow over the wire.
 *
 * Units: m³ (each measurement cycle's `latest_flow_m3h` value summed
 * once per minute over the active period). */
float hour_accumulator();
float day_accumulator();
float month_accumulator();

} /* namespace uflow::history */
