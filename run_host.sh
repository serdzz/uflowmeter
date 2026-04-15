#!/bin/bash
# Run cargo command with host target by temporarily removing embedded target from config
# Restores config even on failure
set -e

CARGO_CMD="${1:-test}"
shift || true

CONFIG=".cargo/config.toml"
BACKUP=".cargo/config.toml.bak"

cleanup() {
    if [ -f "$BACKUP" ]; then
        mv "$BACKUP" "$CONFIG"
    fi
}
trap cleanup EXIT

cp "$CONFIG" "$BACKUP"
sed '/^target = /d' "$BACKUP" > "$CONFIG"

case "$CARGO_CMD" in
    test)
        cargo test --lib --release "$@"
        ;;
    clippy)
        cargo clippy --lib --release -- -D warnings "$@"
        ;;
    test-modbus)
        cargo test --lib modbus --release -- --test-threads=1 "$@"
        ;;
    ui-examples)
        cargo run --example ui_examples --release "$@"
        ;;
    *)
        cargo $CARGO_CMD --release "$@"
        ;;
esac