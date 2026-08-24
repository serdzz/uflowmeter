.PHONY: build bootloader imgtool image test test-release test-modbus test-modbus-verbose \
        ui-examples clippy clippy-host clippy-embedded clippy-bootloader clippy-tools \
        test-fwimage flash flash-bootloader flash-app clean help

# Flashing speed: at the default SWD rate this board fails to connect,
# reproducibly and in several different ways. See CLAUDE.md.
SPEED  ?= 500
CHIP   ?= STM32L151RC
HOST   ?= $(shell rustc -vV | sed -n 's/^host: //p')

APP_ELF  = target/thumbv7m-none-eabi/release/uflowmeter
BOOT_ELF = target/thumbv7m-none-eabi/release/bootloader
IMGTOOL  = target/$(HOST)/release/imgtool

# Key used to sign update images. Override on the command line; the
# bootloader will not build without one unless BOOT_FEATURES=dev-key.
KEYFILE       ?= ufw.key
BOOT_FEATURES ?=
IMG_VERSION   ?= 1

help:
	@echo "Build:"
	@echo "  make build              - Application (slot A, 0x08004000)"
	@echo "  make bootloader         - Bootloader (0x08000000) — needs UFW_AES_KEY"
	@echo "  make image              - Pack + encrypt the application into app.ufw"
	@echo ""
	@echo "Flash (two separate binaries — see docs/BOOTLOADER.md):"
	@echo "  make flash              - Bootloader + application, from scratch"
	@echo "  make flash-bootloader   - Bootloader only"
	@echo "  make flash-app          - Application only (leaves the bootloader alone)"
	@echo ""
	@echo "Test / lint:"
	@echo "  make test               - Host tests for the application library"
	@echo "  make test-fwimage       - Host tests for the image format + AES-GCM"
	@echo "  make clippy             - Every crate, both targets"
	@echo "  make ui-examples        - Run UI examples on host"
	@echo "  make clean              - Clean build artifacts"

build:
	@echo "Building application for thumbv7m-none-eabi..."
	cargo build --release

bootloader:
	@echo "Building bootloader for thumbv7m-none-eabi..."
	cargo build --release -p bootloader $(if $(BOOT_FEATURES),--features $(BOOT_FEATURES),)

imgtool:
	cargo build --release -p imgtool --target $(HOST)

# The application binary has to be raw for this: the ELF carries load
# addresses the bootloader neither needs nor could use, since it writes
# slot A from offset zero.
image: build imgtool
	@test -f $(KEYFILE) || { \
		echo "No $(KEYFILE). Create one with:"; \
		echo "  $(IMGTOOL) keygen --out $(KEYFILE)"; \
		exit 1; }
	cargo objcopy --release --bin uflowmeter -- -O binary app.bin
	$(IMGTOOL) pack --key $(KEYFILE) --in app.bin --out app.ufw --version $(IMG_VERSION)

# ── Flashing ─────────────────────────────────────────────────────────
#
# Over SWD the two binaries are separate downloads; each ELF already
# carries its own addresses, so no offset needs to be given here. Field
# updates do not use this path at all — they go over the serial line via
# `firmware_update`, which is the whole point of the bootloader.

flash: flash-bootloader flash-app

flash-bootloader: bootloader
	probe-rs download --chip $(CHIP) --speed $(SPEED) $(BOOT_ELF)

flash-app: build
	probe-rs download --chip $(CHIP) --speed $(SPEED) $(APP_ELF)
	probe-rs reset --chip $(CHIP) --speed $(SPEED)

# ── Tests and lints ──────────────────────────────────────────────────

test:
	@echo "Running tests on host..."
	bash run_host.sh test

test-fwimage:
	cargo test -p fwimage --target $(HOST)

# `clippy-host` covers src/lib.rs only — the embedded-only modules
# (main.rs, drivers/*) are cfg'd out there, so they need a separate
# pass against the real target. Run all four by default.
clippy: clippy-host clippy-embedded clippy-bootloader clippy-tools

clippy-host:
	@echo "Running clippy on host lib..."
	bash run_host.sh clippy

clippy-embedded:
	@echo "Running clippy on embedded binary..."
	cargo clippy --release --bin uflowmeter -- -D warnings

clippy-bootloader:
	@echo "Running clippy on bootloader..."
	cargo clippy --release -p bootloader --features dev-key -- -D warnings

clippy-tools:
	@echo "Running clippy on fwimage + imgtool..."
	cargo clippy -p fwimage -p imgtool --target $(HOST) -- -D warnings

test-modbus:
	@echo "Running Modbus unit tests..."
	bash run_host.sh test-modbus

test-modbus-verbose:
	@echo "Running Modbus unit tests (verbose)..."
	bash run_host.sh test-modbus

ui-examples:
	@echo "Running UI examples on host..."
	bash run_host.sh ui-examples

test-release: test

clean:
	cargo clean
	rm -f app.bin app.ufw
