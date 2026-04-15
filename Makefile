.PHONY: build test test-release test-modbus test-modbus-verbose ui-examples clean help

help:
	@echo "Available targets:"
	@echo "  make build              - Build release binary (embedded target)"
	@echo "  make test               - Run all tests on host"
	@echo "  make test-release       - Run release tests on host"
	@echo "  make test-modbus        - Run Modbus unit tests only"
	@echo "  make test-modbus-verbose - Run Modbus tests with verbose output"
	@echo "  make ui-examples        - Run UI examples on host"
	@echo "  make clean              - Clean build artifacts"

build:
	@echo "Building embedded binary for thumbv7m-none-eabi..."
	cargo build --release

test:
	@echo "Running tests on host..."
	bash run_host.sh test

clippy:
	@echo "Running clippy on host..."
	bash run_host.sh clippy

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
