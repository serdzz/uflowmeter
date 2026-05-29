/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Persisted configuration ("Options") — calibration table, serial
 * number, sensor type, communication settings. Stored in the on-board
 * 25LC1024 EEPROM as TWO 1024-byte pages (primary at offset 0,
 * secondary at offset 1024); load() falls back to the secondary copy on
 * CRC mismatch, save() writes both. CRC is CRC16-CCITT-FALSE over the
 * 1022 bytes after the CRC field (the entire 1024-byte page minus the
 * CRC slot itself — NOT just the Options-sized portion).
 *
 * Byte layout is bit-exact with the embassy-era Rust Options
 * (modular_bitfield struct, default LSB-first packing). See
 *   git show rework/embassy:src/options.rs
 * Maintaining byte parity matters because:
 *   1. Existing units in the field have Options data written by the
 *      Rust firmware — first boot on the Zephyr firmware MUST load it.
 *   2. The Modbus register map exposes raw byte windows into this
 *      struct (see docs/MODBUS_MAP.md). Re-shuffling fields breaks the
 *      external interface.
 *
 * Anything that adds/reorders/resizes a field changes the EEPROM wire
 * format AND the Modbus map — do not do it without versioning.
 */

#pragma once

#include <cstddef>
#include <cstdint>

struct device;

namespace uflow::options {

enum class CommType : std::uint8_t {
	None   = 0,
	MBus   = 1,
	ModBus = 2,
	Analog = 3,
};

/* Packed POD mirroring the Rust modular_bitfield layout. All multi-byte
 * fields are little-endian (the only endianness Cortex-M3 runs in;
 * modular_bitfield with default config also packs LE). Total: 116 bytes.
 *
 * Field offsets (decimal):
 *    0  crc (u16)
 *    2  serial_number (u32)
 *    6  sensor_type (u8)
 *    7  tdc1000_regs[10]
 *   17  tdc7200_regs[10]
 *   27  zero1, zero2 (u32 ×2)
 *   35  v11..v23 (u32 ×6)
 *   59  k11..k23 (u32 ×6)
 *   83  uptime, total, hour_total, day_total, month_total, rest (u32 ×6)
 *  107  enable_negative, slave_address, comm_type, modbus_mode (u8 ×4)
 *  111  const_val (u32; speed-of-sound geometry constant L²/(2·cos α),
 *       stored as f32 bits — type-pun on read)
 *  115  reserved (u8 padding to even-byte boundary; see Rust comment
 *       about Modbus end-byte OOB check)
 */
struct __attribute__((packed)) Options {
	std::uint16_t crc;
	std::uint32_t serial_number;
	std::uint8_t  sensor_type;
	std::uint8_t  tdc1000_regs[10];
	std::uint8_t  tdc7200_regs[10];
	std::uint32_t zero1;
	std::uint32_t zero2;
	std::uint32_t v11;
	std::uint32_t v12;
	std::uint32_t v13;
	std::uint32_t v21;
	std::uint32_t v22;
	std::uint32_t v23;
	std::uint32_t k11;
	std::uint32_t k12;
	std::uint32_t k13;
	std::uint32_t k21;
	std::uint32_t k22;
	std::uint32_t k23;
	std::uint32_t uptime;
	std::uint32_t total;
	std::uint32_t hour_total;
	std::uint32_t day_total;
	std::uint32_t month_total;
	std::uint32_t rest;
	std::uint8_t  enable_negative;
	std::uint8_t  slave_address;
	std::uint8_t  comm_type;
	std::uint8_t  modbus_mode;
	std::uint32_t const_val;    /* f32 bits */
	std::uint8_t  reserved;
};

static_assert(sizeof(Options) == 116,
	"Options layout drift — must stay byte-exact with the Rust modular_bitfield struct");

constexpr std::size_t OPTIONS_PAGE_SIZE   = 1024;   /* one Options copy on the chip */
constexpr std::uint32_t OPTIONS_OFFSET_PRIMARY   = 0;
constexpr std::uint32_t OPTIONS_OFFSET_SECONDARY = 1024;

enum class LoadResult {
	OkPrimary,         /* primary copy verified */
	OkSecondary,       /* primary CRC failed, secondary verified */
	BothCorrupt,       /* both copies failed CRC — caller should reset to default */
	IoError,           /* eeprom_read returned an error */
};

/* Caller-provided 1024-byte scratch buffer (avoids stack overflow with the
 * 8 KB main-stack budget). Buffer is clobbered. */
LoadResult load(const struct device* eeprom,
                Options&             out,
                std::uint8_t         scratch[OPTIONS_PAGE_SIZE]);

/* Update the CRC field, then write both 1024-byte copies. Returns 0 on
 * success, negative errno from eeprom_write on failure. */
int save(const struct device* eeprom,
         Options&             opts,
         std::uint8_t         scratch[OPTIONS_PAGE_SIZE]);

/* CRC16-CCITT-FALSE (poly 0x1021, init 0xFFFF, no reflect, no XOR-out).
 * Exposed for tests + the Modbus handler's whole-register CRC needs. */
std::uint16_t crc16_ccitt_false(const std::uint8_t* data, std::size_t len);

} /* namespace uflow::options */
