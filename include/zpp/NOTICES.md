# Vendored: lowlander/zpp

This directory contains a verbatim copy of the C++ headers from
[lowlander/zpp](https://github.com/lowlander/zpp) ("Zephyr++"). Used here
as a thin C++20 wrapper layer over Zephyr kernel primitives (threads,
mutexes, message queues, timers, atomics).

| Field         | Value |
|---------------|-------|
| Upstream      | https://github.com/lowlander/zpp |
| Commit        | `f6ca2ae67b4998da1dbba1fedea8e51ac5a588c4` |
| Date          | 2023-01-13 |
| License       | Apache-2.0 (see `LICENSE`) |
| Author        | Erwin Rol (lowlander) |

## Why vendored

The upstream project has had no commits since January 2023. Pulling it
into our tree as plain headers — rather than as a west submodule — means
we don't depend on its repo staying live, can patch it in-place when
Zephyr API drift breaks something, and don't need an extra west remote
that contributors must clone.

## Update procedure

```sh
git clone --depth 1 https://github.com/lowlander/zpp.git /tmp/zpp
cp /tmp/zpp/include/zpp/*.hpp include/zpp/
cp /tmp/zpp/include/zpp.hpp    include/
cp /tmp/zpp/LICENSE            include/zpp/LICENSE
# Then update the commit SHA above.
```

## Scope reminder

zpp wraps OS primitives only — GPIO, SPI, UART, I2C drivers are written
against Zephyr's plain C device API. See `src/drivers/*.cpp` for examples.
