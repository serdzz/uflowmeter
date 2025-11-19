.PHONY: build test test-release clean help

help:
	@echo "Available targets:"
	@echo "  make build         - Build release binary (embedded target)"
	@echo "  make test          - Run tests on host"
	@echo "  make test-release  - Run release tests on host"
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

test-release: test

clean:
	cargo clean
