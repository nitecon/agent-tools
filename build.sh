#!/usr/bin/env bash
set -e

DEST="$1"

cargo build --release

if [ -n "$DEST" ]; then
    mkdir -p "$DEST"
    cp target/release/agent-tools target/release/agent-tools-mcp "$DEST/" 2>/dev/null || \
    cp target/release/agent-tools.exe target/release/agent-tools-mcp.exe "$DEST/"
    echo "Copied binaries to $DEST"
fi
