#!/bin/bash
# Deploy remotelink to aarch64 remote target
#
# Usage: ./scripts/deploy-aarch64.sh user@host
#
# This will copy:
#   - remotelink binary to ~/remotelink
#   - libremotelink_preload.so to /usr/local/lib/ (requires sudo)

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 user@host"
    echo ""
    echo "Example: $0 pi@raspberrypi.local"
    exit 1
fi

TARGET_HOST="$1"
TARGET="aarch64-unknown-linux-gnu"
RELEASE_DIR="target/${TARGET}/release"

BINARY="${RELEASE_DIR}/remotelink"
PRELOAD="${RELEASE_DIR}/libremotelink_preload.so"

# Check if binaries exist
if [ ! -f "$BINARY" ] || [ ! -f "$PRELOAD" ]; then
    echo "Binaries not found. Building first..."
    ./scripts/build-aarch64.sh
fi

echo "Deploying to ${TARGET_HOST}..."

# Copy main binary
echo "  Copying remotelink binary..."
scp "$BINARY" "${TARGET_HOST}:~/remotelink"

# Copy preload library (to /usr/local/lib requires sudo)
echo "  Copying preload library..."
scp "$PRELOAD" "${TARGET_HOST}:/tmp/libremotelink_preload.so"
ssh "$TARGET_HOST" "sudo mv /tmp/libremotelink_preload.so /usr/local/lib/ && sudo ldconfig"

echo ""
echo "Deployment complete!"
echo ""
echo "To start the remote runner:"
echo "  ssh ${TARGET_HOST} '~/remotelink --remote-runner'"
