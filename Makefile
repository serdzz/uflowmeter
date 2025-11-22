.PHONY: build test test-release ui-examples clean help

help:
	@echo "Available targets:"
	@echo "  make build         - Build release binary (embedded target)"
	@echo "  make test          - Run tests on host"
	@echo "  make test-release  - Run release tests on host"
	@echo "  make ui-examples   - Run UI examples on host"
	@echo "  make clean         - Clean build artifacts"

build:
	@echo "Building embedded binary for thumbv7m-none-eabi..."
	cargo build --release

test:
	@echo "Running tests with host target..."
	@sed -i.bak '/^target = /d' .cargo/config.toml && \
	cargo test --lib --release && \
	mv .cargo/config.toml.bak .cargo/config.toml || \
	(mv .cargo/config.toml.bak .cargo/config.toml; exit 1)

clippy:
	@echo "Running clippy with host target..."
        @sed -i.bak '/^target = /d' .cargo/config.toml && \
        cargo clippy --lib --release -- -D warnings && \
        mv .cargo/config.toml.bak .cargo/config.toml || \
        (mv .cargo/config.toml.bak .cargo/config.toml; exit 1)
test-release: test

ui-examples:
	@echo "Running UI examples on host..."
	@sed -i.bak '/^target = /d' .cargo/config.toml && \
	cargo run --example ui_examples --release && \
	mv .cargo/config.toml.bak .cargo/config.toml || \
	(mv .cargo/config.toml.bak .cargo/config.toml; exit 1)

clean:
	cargo clean
