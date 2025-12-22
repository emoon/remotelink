#!/bin/bash
# Build remotelink for aarch64 (e.g., Raspberry Pi)
#
# Prerequisites:
#   sudo apt install gcc-aarch64-linux-gnu
#   rustup target add aarch64-unknown-linux-gnu

set -e

TARGET="aarch64-unknown-linux-gnu"
RELEASE_DIR="target/${TARGET}/release"

echo "Building remotelink for ${TARGET}..."

# Build main binary (static)
echo "  Building main binary (static)..."
RUSTFLAGS="-C target-feature=+crt-static" cargo build --release --target "${TARGET}" -p remotelink

# Build preload library (shared)
echo "  Building preload library (shared)..."
cargo build --release --target "${TARGET}" -p remotelink_preload

echo ""
echo "Build complete! Output files:"
echo "  ${RELEASE_DIR}/remotelink"
echo "  ${RELEASE_DIR}/libremotelink_preload.so"
echo ""
echo "Deploy to remote target:"
echo "  scp ${RELEASE_DIR}/remotelink user@remote:~/"
echo "  scp ${RELEASE_DIR}/libremotelink_preload.so user@remote:/usr/local/lib/"
