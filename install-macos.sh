#!/usr/bin/env bash
set -euo pipefail

REPO="nitecon/agent-tools"
INSTALL_DIR="/opt/agentic/bin"
BINARY_NAMES=("agent-tools" "agent-tools-mcp" "agent-sync")
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

if [ "$OS" != "darwin" ]; then
  error "This script is for macOS only. Use install.sh for Linux."
fi

case "${ARCH}" in
  x86_64)   TARGET="x86_64-apple-darwin" ;;
  arm64)    TARGET="aarch64-apple-darwin" ;;
  aarch64)  TARGET="aarch64-apple-darwin" ;;
  *)        error "Unsupported architecture: ${ARCH}" ;;
esac

# --- Determine the real user (the one who invoked sudo) ---------------------

REAL_USER="${SUDO_USER:-$(logname 2>/dev/null || echo "$USER")}"
REAL_UID=$(id -u "$REAL_USER")
REAL_GID=$(id -g "$REAL_USER")

if [ -z "$REAL_USER" ] || [ "$REAL_USER" = "root" ]; then
  error "Could not determine the non-root user. Run with: sudo bash install-macos.sh"
fi

info "Installing for user: ${REAL_USER} (uid=${REAL_UID}, gid=${REAL_GID})"

# --- Create /opt/agentic owned by the current user -------------------------

mkdir -p "$INSTALL_DIR"
chown -R "${REAL_USER}:staff" /opt/agentic
chmod -R 755 /opt/agentic
info "Set /opt/agentic ownership to ${REAL_USER}:staff"

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

ARCHIVE_NAME="agent-tools-${LATEST_TAG}-${TARGET}.tar.gz"
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

for BIN in "${BINARY_NAMES[@]}"; do
  # The archive may contain the binary at the top level or in a subdirectory
  BIN_PATH=$(find "$TMPDIR" -name "$BIN" -type f | head -1)
  if [ -n "$BIN_PATH" ]; then
    mv "$BIN_PATH" "${INSTALL_DIR}/${BIN}"
    chown "${REAL_USER}:staff" "${INSTALL_DIR}/${BIN}"
    chmod 755 "${INSTALL_DIR}/${BIN}"
    info "Installed ${INSTALL_DIR}/${BIN}"
  else
    warn "Binary ${BIN} not found in archive"
  fi
done

# --- Tools data directory ---------------------------------------------------

TOOLS_DIR="/opt/agentic/tools"
mkdir -p "$TOOLS_DIR"
chown "${REAL_USER}:staff" "$TOOLS_DIR"
chmod 755 "$TOOLS_DIR"
info "Created data directory ${TOOLS_DIR}"

CONFIG_DIR="/opt/agentic/agent-tools"
mkdir -p "$CONFIG_DIR"
chown "${REAL_USER}:staff" "$CONFIG_DIR"
chmod 755 "$CONFIG_DIR"
info "Created config directory ${CONFIG_DIR}"

# --- Symlinks ---------------------------------------------------------------

mkdir -p "$SYMLINK_DIR"

for BIN in "${BINARY_NAMES[@]}"; do
  if [ -f "${INSTALL_DIR}/${BIN}" ]; then
    ln -sf "${INSTALL_DIR}/${BIN}" "${SYMLINK_DIR}/${BIN}"
    info "Symlinked ${SYMLINK_DIR}/${BIN} -> ${INSTALL_DIR}/${BIN}"
  fi
done

# --- Done -------------------------------------------------------------------

# --- Gateway configuration check ----------------------------------------------

GATEWAY_CONF="/opt/agentic/agent-tools/gateway.conf"
if [ ! -f "$GATEWAY_CONF" ]; then
  echo ""
  warn "No gateway configuration found at ${GATEWAY_CONF}"
  echo "  To enable agent communication, run:"
  echo "    agent-tools setup gateway"
  echo ""
fi

echo ""
info "Installation complete!"
echo ""
echo "  Binaries:  ${INSTALL_DIR}/agent-tools"
echo "             ${INSTALL_DIR}/agent-tools-mcp"
echo "             ${INSTALL_DIR}/agent-sync"
echo "  Symlinks:  ${SYMLINK_DIR}/agent-tools"
echo "             ${SYMLINK_DIR}/agent-tools-mcp"
echo "             ${SYMLINK_DIR}/agent-sync"
echo "  Config:    ${CONFIG_DIR}/ (global)"
echo "             ~/.agentic/ (user override)"
echo "  Version:   ${LATEST_TAG}"
echo ""
echo "Quick start (CLI):"
echo "  agent-tools tree"
echo "  agent-tools symbols src/main.rs"
echo "  agent-tools search MyFunction"
echo ""
echo "Configure gateway connection (optional):"
echo "  agent-tools setup gateway"
echo ""
echo "Register as MCP server (includes code tools + comms):"
echo "  claude mcp add -s user agent-tools -- ${INSTALL_DIR}/agent-tools-mcp"
echo ""
