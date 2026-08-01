.PHONY: build test test-release test-modbus test-modbus-verbose ui-examples \
        clippy clippy-host clippy-embedded clean help

help:
	@echo "Available targets:"
	@echo "  make build              - Build release binary (embedded target)"
	@echo "  make test               - Run all tests on host"
	@echo "  make test-release       - Run release tests on host"
	@echo "  make test-modbus        - Run Modbus unit tests only"
	@echo "  make test-modbus-verbose - Run Modbus tests with verbose output"
	@echo "  make clippy             - Run clippy on both host lib and embedded bin"
	@echo "  make clippy-host        - Run clippy on the host-testable lib only"
	@echo "  make clippy-embedded    - Run clippy on the embedded binary (main + drivers)"
	@echo "  make ui-examples        - Run UI examples on host"
	@echo "  make clean              - Clean build artifacts"

build:
	@echo "Building embedded binary for thumbv7m-none-eabi..."
	cargo build --release

test:
	@echo "Running tests on host..."
	bash run_host.sh test

# `clippy-host` covers src/lib.rs only — the embedded-only modules
# (main.rs, drivers/*) are cfg'd out there, so they need a separate
# pass against the real target. Run both by default.
clippy: clippy-host clippy-embedded

clippy-host:
	@echo "Running clippy on host lib..."
	bash run_host.sh clippy

clippy-embedded:
	@echo "Running clippy on embedded binary..."
	cargo clippy --release --bin uflowmeter -- -D warnings

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
