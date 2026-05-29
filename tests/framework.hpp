/*
 * SPDX-License-Identifier: Apache-2.0
 *
 * Bare-minimum host test framework. Tests register via the TEST(name)
 * macro which uses an __attribute__((constructor)) to push the test
 * into a global list before main() runs. main() in framework.cpp
 * walks the list, runs each, reports.
 *
 * Failures are non-aborting — ASSERT_* logs the failure (file:line +
 * expression) and returns from the test function. The runner counts
 * each function as one PASS or FAIL based on whether any assertion
 * tripped during execution.
 *
 * Why not gtest / ztest / Catch2: zero external deps, zero install
 * step, ~80 lines of framework total. Trades feature richness for
 * "build and run with the system C++ compiler". When the suite grows
 * past a few hundred cases, revisit.
 */

#pragma once

#include <cstddef>
#include <cstdint>

namespace uflow::test {

using TestFn = void (*)();

struct TestCase {
	const char* name;
	const char* file;
	TestFn      fn;
};

void register_test(const TestCase& tc);
void mark_failed(const char* file, int line, const char* expr);
int  run_all();

} /* namespace uflow::test */

/* TEST(name) — declares + registers a test function. */
#define TEST(name)                                                          \
	static void test_##name();                                          \
	namespace {                                                         \
	struct Register_##name {                                            \
		Register_##name() {                                         \
			::uflow::test::register_test(                       \
				{#name, __FILE__, &test_##name});           \
		}                                                           \
	};                                                                  \
	static Register_##name register_##name{};                           \
	}                                                                   \
	static void test_##name()

/* Soft-fail assertions — they log + return from the test fn. */
#define ASSERT_TRUE(expr)                                                   \
	do {                                                                \
		if (!(expr)) {                                              \
			::uflow::test::mark_failed(__FILE__, __LINE__,      \
				"ASSERT_TRUE(" #expr ")");                  \
			return;                                             \
		}                                                           \
	} while (0)

#define ASSERT_FALSE(expr) ASSERT_TRUE(!(expr))

#define ASSERT_EQ(a, b)                                                     \
	do {                                                                \
		if (!((a) == (b))) {                                        \
			::uflow::test::mark_failed(__FILE__, __LINE__,      \
				"ASSERT_EQ(" #a ", " #b ")");               \
			return;                                             \
		}                                                           \
	} while (0)

#define ASSERT_NE(a, b)                                                     \
	do {                                                                \
		if ((a) == (b)) {                                           \
			::uflow::test::mark_failed(__FILE__, __LINE__,      \
				"ASSERT_NE(" #a ", " #b ")");               \
			return;                                             \
		}                                                           \
	} while (0)

/* Float equality within `eps`. */
#define ASSERT_NEAR(a, b, eps)                                              \
	do {                                                                \
		double _a = (a), _b = (b), _eps = (eps);                    \
		double _d = (_a > _b) ? (_a - _b) : (_b - _a);              \
		if (_d > _eps) {                                            \
			::uflow::test::mark_failed(__FILE__, __LINE__,      \
				"ASSERT_NEAR(" #a ", " #b ", " #eps ")");   \
			return;                                             \
		}                                                           \
	} while (0)
