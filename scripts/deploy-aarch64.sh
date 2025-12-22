#!/bin/bash
# Deploy remotelink to aarch64 remote target
#
# Usage: ./scripts/deploy-aarch64.sh user@host [dest_dir]
#
# This will copy both binaries to the destination directory (default: ~/)

set -e

if [ -z "$1" ]; then
    echo "Usage: $0 user@host [dest_dir]"
    echo ""
    echo "Example: $0 pi@raspberrypi.local"
    echo "         $0 pi@raspberrypi.local /opt/remotelink"
    exit 1
fi

TARGET_HOST="$1"
DEST_DIR="${2:-~}"
TARGET="aarch64-unknown-linux-gnu"
RELEASE_DIR="target/${TARGET}/release"

BINARY="${RELEASE_DIR}/remotelink"
PRELOAD="${RELEASE_DIR}/libremotelink_preload.so"

# Check if binaries exist
if [ ! -f "$BINARY" ] || [ ! -f "$PRELOAD" ]; then
    echo "Binaries not found. Building first..."
    ./scripts/build-aarch64.sh
fi

echo "Deploying to ${TARGET_HOST}:${DEST_DIR}..."

# Copy both binaries to same directory
echo "  Copying remotelink binary..."
scp "$BINARY" "${TARGET_HOST}:${DEST_DIR}/remotelink"

echo "  Copying preload library..."
scp "$PRELOAD" "${TARGET_HOST}:${DEST_DIR}/libremotelink_preload.so"

echo ""
echo "Deployment complete!"
echo ""
echo "To start the remote runner:"
echo "  ssh ${TARGET_HOST} '${DEST_DIR}/remotelink --remote-runner'"
