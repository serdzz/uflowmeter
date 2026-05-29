/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * RingStorage — EEPROM-backed ring buffer with 14-byte ServiceData
 * header + i32 elements. Used by the three history rings (Hour, Day,
 * Month) to store flow accumulators indexed by 60s-aligned timestamps.
 *
 * On-disk layout per ring (bytes from OFFSET_OF_STAT_PAGE + OFFSET):
 *     [0..4]    size            u32  — number of stored elements (≤ SIZE)
 *     [4..8]    offset_of_last  u32  — slot index of the most recent entry
 *     [8..12]   time_of_last    u32  — timestamp of the most recent entry
 *     [12..14]  crc             u16  — CRC16-CCITT-FALSE over bytes [0..12]
 *     [14..16]  padding         u16
 *     [16..16+4*SIZE]            i32 × SIZE — elements (little-endian)
 *
 * Total bytes per ring: 16 + 4 * SIZE.
 *
 * ⚠ This differs from rework/embassy:src/history_lib.rs which had a
 * bug in SIZE_ON_FLASH (treated SIZE as a byte count instead of an
 * element count → chained rings would have overlapped at 4× the
 * intended layout density). Our SIZE_ON_FLASH is `16 + 4*SIZE` — the
 * correct value. Embassy-written EEPROM data won't read cleanly on
 * the new layout (CRC will mismatch); the rings start fresh on first
 * boot of this firmware on previously-embassy hardware. Acceptable
 * since the embassy data was self-inconsistent anyway.
 *
 * Source: git show rework/embassy:src/history_lib.rs.
 */

#pragma once

#include <cstddef>
#include <cstdint>
#include <cstring>
#include <optional>

#include <zephyr/drivers/eeprom.h>
#include <zephyr/logging/log.h>

#include "crc.hpp"

namespace uflow::history_lib {

constexpr std::size_t OFFSET_OF_STAT_PAGE = 4096;
constexpr std::size_t SERVICE_DATA_BYTES  = 14;
constexpr std::size_t HEADER_BYTES        = 16;  /* ServiceData + 2-byte padding */
constexpr std::size_t ELEMENT_BYTES       = 4;   /* i32 per slot */

struct ServiceData {
	std::uint32_t size{0};
	std::uint32_t offset_of_last{0};
	std::uint32_t time_of_last{0};
	std::uint16_t crc{0};

	void to_bytes(std::uint8_t out[SERVICE_DATA_BYTES]) const
	{
		std::memcpy(&out[0],  &size,           4);
		std::memcpy(&out[4],  &offset_of_last, 4);
		std::memcpy(&out[8],  &time_of_last,   4);
		std::memcpy(&out[12], &crc,            2);
	}

	void from_bytes(const std::uint8_t in[SERVICE_DATA_BYTES])
	{
		std::memcpy(&size,           &in[0],  4);
		std::memcpy(&offset_of_last, &in[4],  4);
		std::memcpy(&time_of_last,   &in[8],  4);
		std::memcpy(&crc,            &in[12], 2);
	}

	/* CRC over first 12 bytes (everything except the crc slot). */
	std::uint16_t computed_crc() const
	{
		std::uint8_t buf[SERVICE_DATA_BYTES];
		to_bytes(buf);
		return crc::crc16_ccitt_false(buf, 12);
	}
};

template <std::size_t OFFSET, std::int32_t SIZE, std::int32_t ELEMENT_SIZE>
class RingStorage {
public:
	static constexpr std::uint32_t START_ADDR =
		static_cast<std::uint32_t>(OFFSET_OF_STAT_PAGE + OFFSET);
	static constexpr std::uint32_t ELEMENTS_ADDR =
		START_ADDR + static_cast<std::uint32_t>(HEADER_BYTES);
	static constexpr std::size_t SIZE_ON_FLASH =
		HEADER_BYTES + ELEMENT_BYTES * static_cast<std::size_t>(SIZE);

	/* Load ServiceData from EEPROM. CRC mismatch → start fresh
	 * (empty ring), which is harmless: subsequent add() calls will
	 * begin populating from index 0. Returns 0 on success, negative
	 * errno on EEPROM read failure. */
	int load(const struct device* eeprom)
	{
		std::uint8_t buf[SERVICE_DATA_BYTES];
		int rc = eeprom_read(eeprom, START_ADDR, buf, sizeof(buf));
		if (rc < 0) {
			return rc;
		}
		ServiceData candidate;
		candidate.from_bytes(buf);
		const std::uint16_t expected = candidate.computed_crc();
		std::uint16_t on_disk;
		std::memcpy(&on_disk, &buf[12], 2);
		if (on_disk != expected) {
			data_ = ServiceData{};
			return 0;
		}
		data_ = candidate;
		return 0;
	}

	std::uint32_t size() const            { return data_.size; }
	std::uint32_t time_of_last() const    { return data_.time_of_last; }
	std::uint32_t offset_of_last() const  { return data_.offset_of_last; }

	bool empty() const { return data_.size == 0; }

	/* Look up the value at `time` (rounded down to 60s). Returns
	 * nullopt if no slot matches. */
	std::optional<std::int32_t> find(const struct device* eeprom, std::uint32_t time)
	{
		if (data_.size == 0) {
			return std::nullopt;
		}
		const std::uint32_t target = time - (time % 60u);
		std::int32_t index = static_cast<std::int32_t>(data_.offset_of_last);
		for (std::uint32_t step = 0; step < data_.size; step++) {
			const std::uint32_t expected_time = data_.time_of_last -
				(data_.size - 1u - static_cast<std::uint32_t>(index)) *
				static_cast<std::uint32_t>(ELEMENT_SIZE);
			if (expected_time == target) {
				std::int32_t value = 0;
				const std::uint32_t addr =
					ELEMENTS_ADDR + ELEMENT_BYTES * static_cast<std::uint32_t>(index);
				int rc = eeprom_read(eeprom, addr, &value, sizeof(value));
				if (rc < 0) {
					return std::nullopt;
				}
				return value;
			}
			if (index == 0) {
				index = SIZE - 1;
			} else {
				index--;
			}
		}
		return std::nullopt;
	}

	/* Append a (value, time) entry. Handles three cases per the
	 * embassy add() logic:
	 *   1. Empty ring → write at index 0.
	 *   2. Forward delta within MAX_GAP_FILL slots → write fill-
	 *      zero entries for missed periods, then the new value.
	 *   3. Delta exceeds gap-fill budget (or whole ring) → reset
	 *      and start fresh from the new value.
	 *
	 * Returns 0 on success, negative errno on EEPROM failure. */
	int add(const struct device* eeprom, std::int32_t value, std::uint32_t time)
	{
		std::uint32_t t = time - (time % 60u);
		if (empty()) {
			return write_entry(eeprom, value, t);
		}
		const std::int64_t delta = static_cast<std::int64_t>(t) -
		                           static_cast<std::int64_t>(data_.time_of_last);
		if (delta > 0) {
			const std::int64_t slots = delta / ELEMENT_SIZE;
			if (slots >= SIZE) {
				data_ = ServiceData{};
				return write_entry(eeprom, value, t);
			}
			if (slots > MAX_GAP_FILL) {
				/* Gap too large for fill — skip filling, just
				 * advance to the new value. Matches embassy
				 * behavior with a warning. */
				return write_entry(eeprom, value, t);
			}
			std::int64_t remaining = delta;
			while (remaining > ELEMENT_SIZE) {
				const std::uint32_t gap_time =
					data_.time_of_last + static_cast<std::uint32_t>(ELEMENT_SIZE);
				int rc = write_entry(eeprom, 0, gap_time);
				if (rc < 0) return rc;
				remaining -= ELEMENT_SIZE;
			}
			return write_entry(eeprom, value, t);
		}
		/* delta <= 0 — clock went backwards. The embassy code zeros
		 * out trailing entries here; we follow suit. */
		std::int64_t back = -delta;
		while (back >= ELEMENT_SIZE) {
			const std::uint32_t addr =
				ELEMENTS_ADDR + ELEMENT_BYTES *
				static_cast<std::uint32_t>(data_.offset_of_last);
			const std::int32_t zero = 0;
			int rc = eeprom_write(eeprom, addr, &zero, sizeof(zero));
			if (rc < 0) return rc;
			if (data_.offset_of_last == data_.size - 1u) {
				data_.size = data_.size - 1u;
			}
			if (data_.offset_of_last == 0) {
				data_.offset_of_last = SIZE - 1;
			} else {
				data_.offset_of_last--;
			}
			data_.time_of_last -= ELEMENT_SIZE;
			back -= ELEMENT_SIZE;
		}
		return write_service(eeprom);
	}

private:
	static constexpr std::int64_t MAX_GAP_FILL = 24;

	int write_entry(const struct device* eeprom, std::int32_t value, std::uint32_t time)
	{
		if (data_.size < static_cast<std::uint32_t>(SIZE)) {
			data_.size++;
		}
		data_.time_of_last = time;
		const std::uint32_t addr =
			ELEMENTS_ADDR + ELEMENT_BYTES *
			static_cast<std::uint32_t>(data_.offset_of_last);
		int rc = eeprom_write(eeprom, addr, &value, sizeof(value));
		if (rc < 0) return rc;
		return write_service(eeprom);
	}

	int write_service(const struct device* eeprom)
	{
		/* Embassy advances offset_of_last in write_service_data;
		 * we mirror that — the slot just written gets the index,
		 * and the next write goes into the slot after. */
		data_.offset_of_last++;
		if (data_.offset_of_last == static_cast<std::uint32_t>(SIZE)) {
			data_.offset_of_last = 0;
		}
		data_.crc = data_.computed_crc();
		std::uint8_t buf[SERVICE_DATA_BYTES];
		data_.to_bytes(buf);
		return eeprom_write(eeprom, START_ADDR, buf, sizeof(buf));
	}

	ServiceData data_;
};

} /* namespace uflow::history_lib */
