#!/usr/bin/env bash
set -e

DEST="$1"

cargo build --release

if [ -n "$DEST" ]; then
    mkdir -p "$DEST"
    cp target/release/claude-tools target/release/claude-tools-mcp "$DEST/" 2>/dev/null || \
    cp target/release/claude-tools.exe target/release/claude-tools-mcp.exe "$DEST/"
    echo "Copied binaries to $DEST"
fi
