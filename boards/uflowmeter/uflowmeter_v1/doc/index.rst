.. _uflowmeter_v1:

uflowmeter v1
#############

Overview
********

Custom ultrasonic flow-meter board built around an STM32L151RC
(Cortex-M3, 256 KB flash, 32 KB SRAM). External 8 MHz crystal drives both
the system clock (via PLL to 16 MHz) and the MCO output on ``PA8``, which
in turn feeds the TDC1000 + TDC7200 ultrasonic timing chain on the rear
of the board.

Supported peripherals
*********************

============== =================================================
Function       Pins
============== =================================================
HD44780 LCD    RS=PC1, RW=PC2, E=PC3, D4-D7=PA4-PA7
LCD power      PC0 (active-LOW)
Backlight      PC5 (active-LOW)
Keypad         PB6 (Config), PB7 (Enter), PB8 (Down/Left), PB9 (Up/Right)
SPI2 bus       SCK=PB13, MISO=PB14, MOSI=PB15
USART1         TX=PA9, RX=PA10 (console + future Modbus RTU)
RS-485 power   PC9 (active-LOW)
MCO out        PA8 → TDC1000/TDC7200 CLK
============== =================================================

The SPI2 bus is shared between the 25LC1024 EEPROM (CS=PC10), TDC1000
(CS=PB11), and TDC7200 (CS=PB12). Driver bring-up for those parts lands
in follow-up commits.

Programming
***********

Default runner is probe-rs::

    west flash

OpenOCD fallback is commented out in ``board.cmake``.

References
**********

* Source pinmap: ``boards/uflowmeter/uflowmeter_v1/uflowmeter_v1.dts``
* Mirror in C++: ``src/pinout.hpp``
* Embassy-era reference: ``git show rework/embassy:src/main.rs``
