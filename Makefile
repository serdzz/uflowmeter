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
	@cp .cargo/config.toml .cargo/config.toml.bak && \
	sed '/^target = /d' .cargo/config.toml.bak > .cargo/config.toml && \
	cargo test --lib --release || \
	(mv .cargo/config.toml.bak .cargo/config.toml && exit 1); \
	mv .cargo/config.toml.bak .cargo/config.toml

clippy:
	@echo "Running clippy on host..."
	@cp .cargo/config.toml .cargo/config.toml.bak && \
	sed '/^target = /d' .cargo/config.toml.bak > .cargo/config.toml && \
	cargo clippy --lib --release -- -D warnings || \
	(mv .cargo/config.toml.bak .cargo/config.toml && exit 1); \
	mv .cargo/config.toml.bak .cargo/config.toml

test-modbus:
	@echo "Running Modbus unit tests..."
	@cp .cargo/config.toml .cargo/config.toml.bak && \
	sed '/^target = /d' .cargo/config.toml.bak > .cargo/config.toml && \
	cargo test --lib modbus --release -- --test-threads=1 || \
	(mv .cargo/config.toml.bak .cargo/config.toml && exit 1); \
	mv .cargo/config.toml.bak .cargo/config.toml

test-modbus-verbose:
	@echo "Running Modbus unit tests (verbose)..."
	@cp .cargo/config.toml .cargo/config.toml.bak && \
	sed '/^target = /d' .cargo/config.toml.bak > .cargo/config.toml && \
	cargo test --lib modbus --release -- --test-threads=1 --nocapture || \
	(mv .cargo/config.toml.bak .cargo/config.toml && exit 1); \
	mv .cargo/config.toml.bak .cargo/config.toml

test-release: test

ui-examples:
	@echo "Running UI examples on host..."
	@cp .cargo/config.toml .cargo/config.toml.bak && \
	sed '/^target = /d' .cargo/config.toml.bak > .cargo/config.toml && \
	cargo run --example ui_examples --release || \
	(mv .cargo/config.toml.bak .cargo/config.toml && exit 1); \
	mv .cargo/config.toml.bak .cargo/config.toml

clean:
	cargo clean
