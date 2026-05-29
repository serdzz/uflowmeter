/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Implementation of the RAM-backed fake EEPROM + the eeprom_read /
 * eeprom_write shim that history_lib.hpp consumes through
 * <zephyr/drivers/eeprom.h> (host shim under tests/shims/).
 */

#include "fake_eeprom.hpp"

#include <zephyr/drivers/eeprom.h>  /* the shim header */

#include <cstdio>
#include <cstdlib>
#include <cstring>

FakeEeprom* g_fake_eeprom = nullptr;

FakeEeprom::FakeEeprom(std::size_t size_bytes)
	: bytes_(size_bytes, 0xFF)  /* fresh chip reads 0xFF */
{
}

FakeEeprom::~FakeEeprom()
{
	if (g_fake_eeprom == this) {
		g_fake_eeprom = nullptr;
	}
}

const struct device* FakeEeprom::install()
{
	if (g_fake_eeprom != nullptr) {
		std::fprintf(stderr,
			"FakeEeprom::install: another instance already installed\n");
		std::abort();
	}
	g_fake_eeprom = this;
	return dev();
}

void FakeEeprom::uninstall()
{
	if (g_fake_eeprom == this) {
		g_fake_eeprom = nullptr;
	}
}

const struct device* FakeEeprom::dev() const
{
	/* The production code only treats this as an opaque pointer — it
	 * never dereferences. Anything non-null + globally distinct works.
	 * We return a cast of `this` so each FakeEeprom has its own
	 * sentinel address. */
	return reinterpret_cast<const struct device*>(this);
}

std::uint8_t* FakeEeprom::data() { return bytes_.data(); }
std::size_t   FakeEeprom::size() const { return bytes_.size(); }

void FakeEeprom::inject_read_error(int err)  { pending_read_err_  = err; }
void FakeEeprom::inject_write_error(int err) { pending_write_err_ = err; }

int FakeEeprom::do_read(std::size_t offset, void* dst, std::size_t len)
{
	if (pending_read_err_ != 0) {
		int e = pending_read_err_;
		pending_read_err_ = 0;
		return e;
	}
	if (offset + len > bytes_.size()) {
		return -22;  /* -EINVAL */
	}
	std::memcpy(dst, &bytes_[offset], len);
	return 0;
}

int FakeEeprom::do_write(std::size_t offset, const void* src, std::size_t len)
{
	if (pending_write_err_ != 0) {
		int e = pending_write_err_;
		pending_write_err_ = 0;
		return e;
	}
	if (offset + len > bytes_.size()) {
		return -22;  /* -EINVAL */
	}
	std::memcpy(&bytes_[offset], src, len);
	return 0;
}

/* --- Shim symbols consumed via <zephyr/drivers/eeprom.h> ---------------- */

extern "C" int eeprom_read(const struct device* dev, off_t offset,
                           void* data, size_t len)
{
	(void)dev;
	if (g_fake_eeprom == nullptr) {
		std::fprintf(stderr, "eeprom_read called with no fake installed\n");
		std::abort();
	}
	return g_fake_eeprom->do_read(static_cast<std::size_t>(offset), data, len);
}

extern "C" int eeprom_write(const struct device* dev, off_t offset,
                            const void* data, size_t len)
{
	(void)dev;
	if (g_fake_eeprom == nullptr) {
		std::fprintf(stderr, "eeprom_write called with no fake installed\n");
		std::abort();
	}
	return g_fake_eeprom->do_write(static_cast<std::size_t>(offset), data, len);
}
