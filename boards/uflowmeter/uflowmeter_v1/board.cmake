# SPDX-License-Identifier: Apache-2.0
#
# Flashing on this Mac uses probe-rs (the same toolchain the previous Rust
# build used; the user already has it installed and configured). openocd is
# provided as a fallback for hosts without probe-rs.

board_runner_args(probe_rs "--chip=STM32L151RC")
include(${ZEPHYR_BASE}/boards/common/probe_rs.board.cmake)

# Fallback: OpenOCD via the ST-Link interface. Uncomment to use.
# board_runner_args(openocd "--cmd-pre-init=transport select hla_swd")
# include(${ZEPHYR_BASE}/boards/common/openocd.board.cmake)
