# SPDX-License-Identifier: Apache-2.0
#
# Top-level CMakeLists for the uflowmeter Zephyr application.
# Custom board lives in boards/arm/uflowmeter_v1/.

cmake_minimum_required(VERSION 3.20.0)

# Tell Zephyr to look for our custom board before the upstream tree.
list(APPEND BOARD_ROOT ${CMAKE_CURRENT_SOURCE_DIR})
list(APPEND DTS_ROOT   ${CMAKE_CURRENT_SOURCE_DIR})

find_package(Zephyr REQUIRED HINTS $ENV{ZEPHYR_BASE})
project(uflowmeter LANGUAGES C CXX)

target_sources(app PRIVATE
    src/main.cpp
    src/calibration.cpp
    src/datetime.cpp
    src/history.cpp
    src/measurement.cpp
    # history_lib.hpp is template-only — header-included by history.cpp
    src/modbus.cpp
    src/modbus_handler.cpp
    src/options.cpp
    src/power.cpp
    src/timer/uflowmeter_rtc_timer.c
    src/drivers/cyrillic.cpp
    src/drivers/eeprom_power.cpp
    src/drivers/hd44780.cpp
    src/drivers/keypad.cpp
    src/drivers/tdc1000.cpp
    src/drivers/tdc7200.cpp
    src/ui/events.cpp
    src/ui/menu_controller.cpp
    src/ui/menu_list.cpp
    src/ui/render.cpp
)

target_include_directories(app PRIVATE
    ${CMAKE_CURRENT_SOURCE_DIR}/include    # vendored zpp/ headers
    ${CMAKE_CURRENT_SOURCE_DIR}/src
)
