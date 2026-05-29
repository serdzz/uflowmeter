/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Host-side shim for <zephyr/drivers/eeprom.h>. Lets src/history_lib.hpp
 * compile in the standalone host test build without dragging Zephyr in.
 *
 * The two functions are linked from tests/fake_eeprom.cpp where they
 * read/write a flat std::vector<uint8_t>. struct device is opaque —
 * the test hands the address of a FakeEeprom (which inherits from
 * `struct device`) into eeprom_read/eeprom_write, and the shim
 * downcasts via the global g_fake_eeprom pointer.
 *
 * Real Zephyr signatures (drivers/include/zephyr/drivers/eeprom.h):
 *     int eeprom_read (const struct device*, off_t, void*,       size_t);
 *     int eeprom_write(const struct device*, off_t, const void*, size_t);
 */

#pragma once

#include <stddef.h>
#include <sys/types.h>  /* off_t */

#ifdef __cplusplus
extern "C" {
#endif

struct device;

int eeprom_read (const struct device* dev, off_t offset,
                 void*       data, size_t len);
int eeprom_write(const struct device* dev, off_t offset,
                 const void* data, size_t len);

#ifdef __cplusplus
}
#endif
