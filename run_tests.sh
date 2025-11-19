#!/bin/bash
# Script to run tests with host target

# Temporarily remove the default build target
BUILD_TARGET=$(grep -A1 "^\[build\]" .cargo/config.toml | grep "^target" | cut -d'"' -f2)

if [ -z "$BUILD_TARGET" ]; then
    echo "No default build target found"
    exit 1
fi

# Create a temporary config without default target
cp .cargo/config.toml .cargo/config.toml.bak

# Remove the target line from [build]
sed -i '' '/^\[build\]/,/^\[/s/^target = .*/# target disabled for tests/' .cargo/config.toml

# Run tests
echo "Running tests..."
cargo test --lib history_lib --release "$@"
TEST_RESULT=$?

# Restore the original config
mv .cargo/config.toml.bak .cargo/config.toml

exit $TEST_RESULT
