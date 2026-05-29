/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Cyrillic glyph table for HD44780 CGRAM (5x8 patterns) +
 * Cyrillic→Latin lookalike substitutions for ROM-resident glyphs.
 *
 * The HD44780 standard character set ROM (A00) has very limited
 * Cyrillic. We use 8 user-definable CGRAM slots for the glyphs we
 * actually need on each rendered frame, with the lookalike table
 * cutting CGRAM pressure (А↔A, Р↔P, etc. are identical glyphs in
 * the Latin ROM).
 *
 * Source: git show rework/embassy:src/drivers/cyrillic.rs.
 * Bitmaps lifted verbatim from
 * git show rework/embassy:src/hardware/display.rs::preload_russian_chars.
 */

#pragma once

#include <cstdint>
#include <optional>

namespace uflow::drivers::cyrillic {

/* Look up the 8-byte CGRAM pattern for a Cyrillic codepoint. Returns
 * nullopt for codepoints we don't have a glyph for. */
std::optional<const std::uint8_t*> lookup(char32_t codepoint);

/* Returns the ROM byte to print directly when a Cyrillic codepoint
 * matches a Latin glyph visually (saves a CGRAM slot). nullopt for
 * codepoints without a lookalike. */
std::optional<std::uint8_t> latin_lookalike(char32_t codepoint);

} /* namespace uflow::drivers::cyrillic */
