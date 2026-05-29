/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * RAM-backed fake EEPROM for host tests. Implements the two Zephyr
 * functions the production code uses: eeprom_read / eeprom_write
 * (declared by tests/shims/zephyr/drivers/eeprom.h).
 *
 * Usage:
 *
 *     FakeEeprom fake(131072);          // 25LC1024 = 128 KiB
 *     fake.install();                   // wire it into the shim funcs
 *     hour_ring.add(fake.dev(), 100, 1234567);
 *     fake.uninstall();
 *
 * Only one FakeEeprom can be installed at a time; install() asserts
 * the previous slot is empty. Tests that need to swap should
 * uninstall() between them.
 *
 * No thread safety — the host test harness runs single-threaded.
 */

#pragma once

#include <cstddef>
#include <cstdint>
#include <vector>

struct device;  /* opaque — declared by the eeprom.h shim */

class FakeEeprom {
public:
	explicit FakeEeprom(std::size_t size_bytes);
	~FakeEeprom();

	FakeEeprom(const FakeEeprom&)            = delete;
	FakeEeprom& operator=(const FakeEeprom&) = delete;

	/* Install/uninstall as the active backing store. install() also
	 * returns the device pointer to pass into production code. */
	const struct device* install();
	void                 uninstall();

	const struct device* dev() const;

	/* Direct access for test setup / assertions. */
	std::uint8_t* data();
	std::size_t   size() const;

	/* Inject a one-shot read error on the next eeprom_read call. The
	 * code returns this value (e.g. -EIO) and the flag is cleared. */
	void inject_read_error(int err);
	void inject_write_error(int err);

	/* For shim use — not part of the public API. */
	int do_read (std::size_t offset,       void* dst, std::size_t len);
	int do_write(std::size_t offset, const void* src, std::size_t len);

private:
	std::vector<std::uint8_t> bytes_;
	int                       pending_read_err_  = 0;
	int                       pending_write_err_ = 0;
};

/* Currently installed instance (nullptr when none). The shim functions
 * dispatch to this. Declared here so tests can poke at it for
 * diagnostics. */
extern FakeEeprom* g_fake_eeprom;
