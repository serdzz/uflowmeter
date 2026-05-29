/*
 * SPDX-License-Identifier: Apache-2.0
 */

#include "framework.hpp"

#include <cstdio>

namespace uflow::test {

namespace {

constexpr std::size_t MAX_TESTS = 256;

TestCase   tests_[MAX_TESTS];
std::size_t test_count_ = 0;
bool        current_failed_ = false;

} /* namespace */

void register_test(const TestCase& tc)
{
	if (test_count_ >= MAX_TESTS) {
		std::fprintf(stderr, "test: registry overflow at %s\n", tc.name);
		return;
	}
	tests_[test_count_++] = tc;
}

void mark_failed(const char* file, int line, const char* expr)
{
	std::fprintf(stderr, "  %s:%d: %s\n", file, line, expr);
	current_failed_ = true;
}

int run_all()
{
	std::size_t passed = 0;
	std::size_t failed = 0;
	for (std::size_t i = 0; i < test_count_; i++) {
		const TestCase& tc = tests_[i];
		current_failed_ = false;
		std::printf("RUN  %s\n", tc.name);
		tc.fn();
		if (current_failed_) {
			std::printf("FAIL %s\n", tc.name);
			failed++;
		} else {
			std::printf("PASS %s\n", tc.name);
			passed++;
		}
	}
	std::printf("\n%zu passed, %zu failed (of %zu)\n",
		passed, failed, test_count_);
	return (failed == 0) ? 0 : 1;
}

} /* namespace uflow::test */

int main()
{
	return uflow::test::run_all();
}
