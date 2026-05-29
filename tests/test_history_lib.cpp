/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Tests for src/history_lib.hpp::RingStorage against a RAM-backed
 * fake EEPROM. Covers the four behaviors the embassy port + our
 * SIZE_ON_FLASH-correction touch:
 *
 *   1. Empty-ring load: fresh 0xFF chip → CRC mismatch → empty ring.
 *   2. add/find round-trip on a small ring.
 *   3. SIZE_ON_FLASH layout: chained rings DON'T overlap when offsets
 *      are computed as `prev_offset + prev::SIZE_ON_FLASH`.
 *   4. Wrap-around: writing more elements than SIZE rotates correctly.
 *   5. Gap-fill: forward delta within MAX_GAP_FILL writes zero slots
 *      for the missed periods; delta beyond resets the ring.
 *   6. Persistence: load() after a fresh instance reads back the
 *      same data + CRC (proves CRC layout is stable across writes).
 *
 * The 25LC1024 used in production is 128 KiB; we allocate the same
 * here so any code accidentally indexing beyond the configured rings
 * still hits valid backing store (catches off-by-ones rather than
 * masking them).
 */

#include "framework.hpp"
#include "fake_eeprom.hpp"

#include "../src/history_lib.hpp"

#include <array>
#include <cstdint>

using uflow::history_lib::RingStorage;
using uflow::history_lib::HEADER_BYTES;
using uflow::history_lib::ELEMENT_BYTES;
using uflow::history_lib::OFFSET_OF_STAT_PAGE;

namespace {

/* Tiny ring for tight wrap testing: 4 slots × 60 s period. */
using TinyRing = RingStorage<0, 4, 60>;

/* Two chained rings whose offsets stack — verifies SIZE_ON_FLASH math
 * keeps them disjoint. */
using ChainA = RingStorage<0, 8, 60>;
using ChainB = RingStorage<ChainA::SIZE_ON_FLASH, 8, 60>;

constexpr std::size_t EEPROM_BYTES = 128 * 1024;

} /* namespace */

TEST(history_size_on_flash_formula)
{
	/* HEADER_BYTES (16) + 4 bytes per slot. Verifies the embassy bug
	 * fix — embassy used SIZE+20 (treated SIZE as bytes); we use
	 * 16 + 4*SIZE. */
	static_assert(TinyRing::SIZE_ON_FLASH == 16 + 4 * 4,
		"tiny ring layout drift");
	static_assert(ChainA::SIZE_ON_FLASH == 16 + 4 * 8, "chain A drift");
	static_assert(ChainB::START_ADDR ==
		ChainA::START_ADDR + ChainA::SIZE_ON_FLASH, "chains overlap");
	ASSERT_EQ(TinyRing::START_ADDR, OFFSET_OF_STAT_PAGE);
}

TEST(history_load_fresh_is_empty)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	int rc = ring.load(dev);
	ASSERT_EQ(rc, 0);
	ASSERT_TRUE(ring.empty());
	ASSERT_EQ(ring.size(), 0u);

	fake.uninstall();
}

TEST(history_add_then_find_roundtrip)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	ring.load(dev);

	/* time=120, value=42 — time gets snapped to multiple of 60. */
	int rc = ring.add(dev, 42, 120);
	ASSERT_EQ(rc, 0);
	ASSERT_EQ(ring.size(), 1u);
	ASSERT_EQ(ring.time_of_last(), 120u);

	auto found = ring.find(dev, 120);
	ASSERT_TRUE(found.has_value());
	ASSERT_EQ(*found, 42);

	/* Reading a time that wasn't written returns nullopt. */
	auto missing = ring.find(dev, 60);
	ASSERT_TRUE(!missing.has_value());

	fake.uninstall();
}

TEST(history_chained_rings_do_not_overlap)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	ChainA a;
	ChainB b;
	a.load(dev);
	b.load(dev);

	/* Distinct sentinel values written into each ring's first slot.
	 * If SIZE_ON_FLASH math were wrong they'd alias. */
	ASSERT_EQ(a.add(dev, 0x1111'1111, 60), 0);
	ASSERT_EQ(b.add(dev, 0x2222'2222, 60), 0);

	/* Round-trip into fresh ring instances reading from the same
	 * backing — proves the writes landed at disjoint offsets. */
	ChainA a2;
	ChainB b2;
	a2.load(dev);
	b2.load(dev);
	auto va = a2.find(dev, 60);
	auto vb = b2.find(dev, 60);
	ASSERT_TRUE(va.has_value());
	ASSERT_TRUE(vb.has_value());
	ASSERT_EQ(*va, 0x1111'1111);
	ASSERT_EQ(*vb, 0x2222'2222);

	fake.uninstall();
}

TEST(history_wraps_when_more_than_size_writes)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	ring.load(dev);

	/* SIZE=4, period=60 — write 6 contiguous entries. The last 4
	 * (300..480) must remain findable; the first 2 (60..180) must
	 * have been overwritten. */
	for (std::uint32_t t = 60; t <= 480; t += 60) {
		int rc = ring.add(dev, static_cast<std::int32_t>(t), t);
		ASSERT_EQ(rc, 0);
	}
	ASSERT_EQ(ring.size(), 4u);
	ASSERT_EQ(ring.time_of_last(), 480u);

	/* Retained slots: 240/300/360/420/480? Need to think about this
	 * carefully — SIZE=4, time_of_last=480, so the window covers
	 * exactly 4 entries at times 300, 360, 420, 480 (each 60 s
	 * apart). */
	for (std::uint32_t t : {300u, 360u, 420u, 480u}) {
		auto v = ring.find(dev, t);
		ASSERT_TRUE(v.has_value());
		ASSERT_EQ(*v, static_cast<std::int32_t>(t));
	}
	/* Evicted slots return nullopt. */
	for (std::uint32_t t : {60u, 120u, 180u, 240u}) {
		auto v = ring.find(dev, t);
		ASSERT_TRUE(!v.has_value());
	}

	fake.uninstall();
}

TEST(history_gap_fill_zeroes_skipped_periods)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	ring.load(dev);

	/* First write at t=60. */
	ring.add(dev, 100, 60);

	/* Skip two periods (120, 180) — jump to t=240. add() should
	 * fill 120 + 180 with zero entries then write 99 at 240. */
	ring.add(dev, 99, 240);
	ASSERT_EQ(ring.size(), 4u);  /* clamped to SIZE */
	ASSERT_EQ(ring.time_of_last(), 240u);

	/* The original 100 has fallen off the front (size clamped). The
	 * two zeroes and the 99 should be findable. */
	auto z120 = ring.find(dev, 120);
	auto z180 = ring.find(dev, 180);
	auto last = ring.find(dev, 240);
	ASSERT_TRUE(z120.has_value());
	ASSERT_TRUE(z180.has_value());
	ASSERT_TRUE(last.has_value());
	ASSERT_EQ(*z120, 0);
	ASSERT_EQ(*z180, 0);
	ASSERT_EQ(*last, 99);

	fake.uninstall();
}

TEST(history_large_jump_resets_ring)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	ring.load(dev);

	ring.add(dev, 42, 60);
	/* Jump by more than SIZE*ELEMENT_SIZE = 4*60 = 240 s →
	 * embassy's MAX_GAP_FILL=24 doesn't matter here, slots-jumped
	 * exceeds SIZE first → full ring reset. */
	ring.add(dev, 7, 60 + 600);
	ASSERT_EQ(ring.size(), 1u);
	ASSERT_EQ(ring.time_of_last(), 660u);

	/* Old entry must be gone. */
	auto old = ring.find(dev, 60);
	ASSERT_TRUE(!old.has_value());
	auto fresh = ring.find(dev, 660);
	ASSERT_TRUE(fresh.has_value());
	ASSERT_EQ(*fresh, 7);

	fake.uninstall();
}

TEST(history_persists_across_reload)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	{
		TinyRing w;
		w.load(dev);
		w.add(dev, 11, 60);
		w.add(dev, 22, 120);
		w.add(dev, 33, 180);
	}

	/* New instance over the same fake — load() must reconstruct
	 * state from the on-disk ServiceData (proves the CRC + header
	 * layout is internally consistent). */
	TinyRing r;
	int rc = r.load(dev);
	ASSERT_EQ(rc, 0);
	ASSERT_EQ(r.size(), 3u);
	ASSERT_EQ(r.time_of_last(), 180u);

	auto v60  = r.find(dev, 60);
	auto v120 = r.find(dev, 120);
	auto v180 = r.find(dev, 180);
	ASSERT_TRUE(v60.has_value()  && *v60  == 11);
	ASSERT_TRUE(v120.has_value() && *v120 == 22);
	ASSERT_TRUE(v180.has_value() && *v180 == 33);

	fake.uninstall();
}

TEST(history_corrupted_crc_starts_fresh)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	{
		TinyRing w;
		w.load(dev);
		w.add(dev, 77, 60);
	}

	/* Corrupt one byte of the on-disk ServiceData → CRC mismatch.
	 * The corruption byte is at `OFFSET_OF_STAT_PAGE + 0` (the
	 * `size` u32) so the CRC, computed over bytes [0..12], will
	 * stop matching. */
	fake.data()[OFFSET_OF_STAT_PAGE] ^= 0x55;

	TinyRing r;
	int rc = r.load(dev);
	ASSERT_EQ(rc, 0);
	ASSERT_TRUE(r.empty());

	fake.uninstall();
}

TEST(history_read_error_is_surfaced)
{
	FakeEeprom fake(EEPROM_BYTES);
	auto* dev = fake.install();

	TinyRing ring;
	fake.inject_read_error(-5);  /* -EIO */
	int rc = ring.load(dev);
	ASSERT_NE(rc, 0);  /* load() returns the eeprom_read errno */

	fake.uninstall();
}
