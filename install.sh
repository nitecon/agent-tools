#!/usr/bin/env bash
set -euo pipefail

REPO="nitecon/agent-tools"
INSTALL_DIR="/opt/agentic/bin"
BINARY_NAMES=("agent-tools" "agent-tools-mcp")
SYMLINK_DIR="/usr/local/bin"

# --- Helpers ----------------------------------------------------------------

info()  { printf '\033[1;32m[INFO]\033[0m  %s\n' "$*"; }
warn()  { printf '\033[1;33m[WARN]\033[0m  %s\n' "$*"; }
error() { printf '\033[1;31m[ERROR]\033[0m %s\n' "$*" >&2; exit 1; }

# --- Pre-flight checks ------------------------------------------------------

if [ "$(id -u)" -ne 0 ]; then
  error "This script must be run as root. Try: curl -fsSL <url> | sudo bash"
fi

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS" in
  linux)  PLATFORM="linux" ;;
  darwin) PLATFORM="macos" ;;
  *)      error "Unsupported OS: $OS" ;;
esac

case "$ARCH" in
  x86_64)        ARCH="x86_64" ;;
  aarch64|arm64) ARCH="aarch64" ;;
  *)             error "Unsupported architecture: $ARCH" ;;
esac

# --- Resolve latest version -------------------------------------------------

info "Resolving latest release..."
if command -v curl &>/dev/null; then
  DOWNLOAD="curl -fsSL"
  DOWNLOAD_OUT="curl -fsSL -o"
elif command -v wget &>/dev/null; then
  DOWNLOAD="wget -qO-"
  DOWNLOAD_OUT="wget -qO"
else
  error "Neither curl nor wget found. Install one and retry."
fi

LATEST_TAG=$($DOWNLOAD "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
  error "Could not determine latest release from GitHub."
fi

info "Latest version: ${LATEST_TAG}"

ARCHIVE_NAME="agent-tools-${PLATFORM}-${ARCH}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${ARCHIVE_NAME}"

# --- Check existing installation --------------------------------------------

if [ -f "${INSTALL_DIR}/agent-tools" ]; then
  CURRENT_VERSION=$(${INSTALL_DIR}/agent-tools --version 2>/dev/null || echo "unknown")
  info "Existing installation found: ${CURRENT_VERSION}"
  info "Upgrading to ${LATEST_TAG}..."
else
  info "No existing installation found. Installing fresh."
fi

# --- Download and extract ---------------------------------------------------

TMPDIR=$(mktemp -d)
trap 'rm -rf "$TMPDIR"' EXIT

info "Downloading ${ARCHIVE_NAME}..."
$DOWNLOAD_OUT "${TMPDIR}/${ARCHIVE_NAME}" "$DOWNLOAD_URL"

info "Extracting..."
tar xzf "${TMPDIR}/${ARCHIVE_NAME}" -C "$TMPDIR"

# --- Install ----------------------------------------------------------------

mkdir -p "$INSTALL_DIR"

for BIN in "${BINARY_NAMES[@]}"; do
  # The archive may contain the binary at the top level or in a subdirectory
  BIN_PATH=$(find "$TMPDIR" -name "$BIN" -type f | head -1)
  if [ -n "$BIN_PATH" ]; then
    mv "$BIN_PATH" "${INSTALL_DIR}/${BIN}"
    chmod +x "${INSTALL_DIR}/${BIN}"
    info "Installed ${INSTALL_DIR}/${BIN}"
  else
    warn "Binary ${BIN} not found in archive"
  fi
done

# --- Tools data directory ---------------------------------------------------

TOOLS_DIR="/opt/agentic/tools"
mkdir -p "$TOOLS_DIR"
chmod 1777 "$TOOLS_DIR"
info "Created data directory ${TOOLS_DIR} (mode 1777)"

# --- Symlinks ---------------------------------------------------------------

for BIN in "${BINARY_NAMES[@]}"; do
  if [ -f "${INSTALL_DIR}/${BIN}" ]; then
    ln -sf "${INSTALL_DIR}/${BIN}" "${SYMLINK_DIR}/${BIN}"
    info "Symlinked ${SYMLINK_DIR}/${BIN} -> ${INSTALL_DIR}/${BIN}"
  fi
done

# --- Done -------------------------------------------------------------------

echo ""
info "Installation complete!"
echo ""
echo "  Binaries:  ${INSTALL_DIR}/agent-tools"
echo "             ${INSTALL_DIR}/agent-tools-mcp"
echo "  Symlinks:  ${SYMLINK_DIR}/agent-tools"
echo "             ${SYMLINK_DIR}/agent-tools-mcp"
echo "  Version:   ${LATEST_TAG}"
echo ""
echo "Quick start (CLI):"
echo "  agent-tools tree"
echo "  agent-tools symbols src/main.rs"
echo "  agent-tools search MyFunction"
echo ""
echo "Register as MCP server for Claude Code:"
echo "  claude mcp add -s user agent-tools -- ${INSTALL_DIR}/agent-tools-mcp"
echo ""
