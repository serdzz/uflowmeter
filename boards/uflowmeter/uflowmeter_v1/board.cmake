# SPDX-License-Identifier: Apache-2.0
#
# Flashing: Zephyr 4.1 ships openocd, jlink, pyocd, stm32cubeprogrammer
# upstream. probe-rs ISN'T upstream (we used it via .embed.toml in the
# Rust era); to use it post-build, invoke directly:
#
#     probe-rs run --chip STM32L151RC build/zephyr/zephyr.elf
#
# Default runners declared here (in priority order; first one selected
# is the default for `west flash`):
#
#   1. stm32cubeprogrammer — if the user has STM32CubeProgrammer
#      installed (typically the most reliable runner for ST chips).
#   2. openocd via openocd-stm32.board.cmake — pulls in the ST-Link
#      interface + stm32l1x target config automatically.
#
# Matches the pattern from boards/st/nucleo_l152re/board.cmake.

board_runner_args(stm32cubeprogrammer "--port=swd" "--reset-mode=hw")

include(${ZEPHYR_BASE}/boards/common/stm32cubeprogrammer.board.cmake)
include(${ZEPHYR_BASE}/boards/common/openocd-stm32.board.cmake)
