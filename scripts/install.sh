#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0 OR MIT
#
# Install script for the TPT Bastion CLI.
#
#   curl -fsSL https://raw.githubusercontent.com/tpt/bastion/master/scripts/install.sh | sh
#
# Downloads the latest (or a pinned --version) release asset for the current
# OS/architecture from GitHub, extracts it, and places `bastion` on your PATH
# (default: ~/.local/bin). Supports --version, --to <dir>, and --dry-run.

set -euo pipefail

REPO="tpt/bastion"
BIN="bastion"
INSTALL_DIR="${BASTION_INSTALL_DIR:-$HOME/.local/bin}"
VERSION=""
DRY_RUN=0

usage() {
  cat <<'EOF'
usage: install.sh [--version vX.Y.Z] [--to DIR] [--dry-run] [--help]
  --version vX.Y.Z   install a specific release (default: latest)
  --to DIR           install directory (default: $HOME/.local/bin)
  --dry-run          print actions instead of performing them
  --help             show this help
EOF
}

while [ $# -gt 0 ]; do
  case "$1" in
    --version) VERSION="$2"; shift 2 ;;
    --to) INSTALL_DIR="$2"; shift 2 ;;
    --dry-run) DRY_RUN=1; shift ;;
    --help|-h) usage; exit 0 ;;
    *) echo "unknown argument: $1" >&2; usage >&2; exit 1 ;;
  esac
done

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux) os="unknown-linux-gnu" ;;
    Darwin) os="apple-darwin" ;;
    *) echo "unsupported OS: $os" >&2; exit 1 ;;
  esac
  case "$arch" in
    x86_64|amd64) arch="x86_64" ;;
    arm64|aarch64) arch="aarch64" ;;
    *) echo "unsupported arch: $arch" >&2; exit 1 ;;
  esac
  echo "${arch}-${os}"
}

TARGET="$(detect_target)"

if [ -z "$VERSION" ]; then
  echo "Resolving latest release..." >&2
  VERSION="$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep -m1 '"tag_name"' | sed -E 's/.*"tag_name": *"([^"]+)".*/\1/')"
fi

ASSET="${BIN}-${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/${REPO}/releases/download/${VERSION}/${ASSET}"

echo "Installing ${BIN} ${VERSION} for ${TARGET}" >&2

run() {
  if [ "$DRY_RUN" -eq 1 ]; then
    echo "+ $*" >&2
  else
    "$@"
  fi
}

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

run curl -fLSL "$URL" -o "$TMP/$ASSET"
run tar -xzf "$TMP/$ASSET" -C "$TMP"
run mkdir -p "$INSTALL_DIR"
run install -m 0755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"

echo "Installed to ${INSTALL_DIR}/${BIN}" >&2
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Note: add ${INSTALL_DIR} to your PATH to run 'bastion' from anywhere." >&2 ;;
esac
