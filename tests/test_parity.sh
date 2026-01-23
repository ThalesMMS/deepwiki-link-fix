#!/bin/bash
# Test script to verify Rust and Python outputs are IDENTICAL
# Usage: ./tests/test_parity.sh

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_DIR"

# Clean output directories
echo "==> Cleaning output directories..."
rm -rf ./output_python ./output_rust

# Create fresh output directories
mkdir -p ./output_python ./output_rust

# Run Python version
echo "==> Running Python version..."
python3 scripts/fix_docs.py ./input ./output_python

# Build and run Rust version
echo "==> Building Rust version..."
cargo build --release

echo "==> Running Rust version..."
./target/release/fix-docs ./input ./output_rust

# Compare outputs
echo "==> Comparing outputs..."
if diff -r ./output_python ./output_rust > /dev/null 2>&1; then
    echo ""
    echo "✅ SUCCESS: All outputs are IDENTICAL"
    exit 0
else
    echo ""
    echo "❌ FAILURE: Outputs differ!"
    echo ""
    echo "Differences:"
    diff -r ./output_python ./output_rust || true
    exit 1
fi
