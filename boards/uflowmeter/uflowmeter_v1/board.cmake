# SPDX-License-Identifier: Apache-2.0
#
# Flashing: Zephyr 4.1 ships openocd, jlink, pyocd, stm32cubeprogrammer
# as upstream runners. probe-rs ISN'T upstream (we used it via .embed.toml
# in the Rust era); to use it post-build, invoke directly:
#
#     probe-rs run --chip STM32L151RC build/zephyr/zephyr.elf
#
# Default in-tree runner here is openocd via the ST-Link interface.

board_runner_args(openocd "--cmd-pre-init=transport select hla_swd")
include(${ZEPHYR_BASE}/boards/common/openocd.board.cmake)
