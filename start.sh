#!/bin/bash

# Quick start script for Stratum V2 JDC

set -e

echo "🚀 Stratum V2 Job Declarator Client - Quick Start"
echo "=================================================="
echo ""

# Check if config exists
if [ ! -f "config.toml" ]; then
    echo " config.toml not found!"
    echo "Please copy config.toml.example and edit it with your settings."
    exit 1
fi

# Check if Rust is installed
if ! command -v cargo &> /dev/null; then
    echo " Rust is not installed!"
    echo "Install from: https://rustup.rs/"
    exit 1
fi

echo "✓ Configuration found"
echo "✓ Rust toolchain detected"
echo ""

# Build the project
echo "📦 Building project (this may take a few minutes)..."
cargo build --release

echo ""
echo "✅ Build complete!"
echo ""
echo "Starting JDC..."
echo "Press 'q' or ESC to quit"
echo ""

# Run the application
cargo run --release
