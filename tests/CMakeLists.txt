# SPDX-License-Identifier: Apache-2.0
#
# Standalone host build for the pure-logic test harness. Independent
# of Zephyr — uses the system's default C++ compiler. Build and run:
#
#   cmake -S tests -B build-tests
#   cmake --build build-tests
#   ./build-tests/uflowmeter_tests
#
# What's covered: src/modbus.cpp, src/shell.cpp, src/calibration.cpp.
# What's NOT: src/datetime.cpp (STM32L1 RTC registers),
#             src/options.cpp (Zephyr eeprom_read/write),
#             src/history.cpp, src/history_lib.hpp (Zephyr eeprom),
#             src/power.cpp, src/timer/* (chip registers + Zephyr PM),
#             everything under src/drivers/* and src/ui/render.cpp
#             (Zephyr HAL).
# Decoupling those for host tests is its own future commit.

cmake_minimum_required(VERSION 3.20)
project(uflowmeter_tests LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_CXX_EXTENSIONS OFF)

if(NOT CMAKE_BUILD_TYPE)
    set(CMAKE_BUILD_TYPE Debug)
endif()

add_compile_options(
    -Wall
    -Wextra
    -Wpedantic
    -Wno-c++17-extensions   # we use nested namespaces; project standard
)

add_executable(uflowmeter_tests
    framework.cpp
    test_modbus.cpp
    test_shell.cpp
    test_calibration.cpp
    # Production sources under test
    ${CMAKE_CURRENT_SOURCE_DIR}/../src/modbus.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/../src/shell.cpp
    ${CMAKE_CURRENT_SOURCE_DIR}/../src/calibration.cpp
)

target_include_directories(uflowmeter_tests PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}
    ${CMAKE_CURRENT_SOURCE_DIR}/../src
)

enable_testing()
add_test(NAME uflowmeter_tests COMMAND uflowmeter_tests)
