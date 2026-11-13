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

is_musl() {
  # ldd on musl prints "musl libc"; the loader file is a more reliable signal.
  if ls /lib/ld-musl-*.so.1 >/dev/null 2>&1; then return 0; fi
  ldd --version 2>&1 | grep -qi musl
}

detect_target() {
  local os arch
  os="$(uname -s)"
  arch="$(uname -m)"
  case "$os" in
    Linux)
      # Prefer the static musl build on musl systems (e.g. Alpine). Only
      # x86_64 musl is published; other arches keep the gnu build.
      os="unknown-linux-gnu"
      case "$(uname -m)" in
        x86_64|amd64) is_musl && os="unknown-linux-musl" ;;
      esac
      ;;
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

# Verify the archive against the release's SHA256SUMS before extracting it.
verify_checksum() {
  curl -fLSL "https://github.com/${REPO}/releases/download/${VERSION}/SHA256SUMS" \
    -o "$TMP/SHA256SUMS" || { echo "error: could not download SHA256SUMS" >&2; exit 1; }
  local expected actual
  expected="$(awk -v f="$ASSET" '$2 == f { print $1 }' "$TMP/SHA256SUMS")"
  if [ -z "$expected" ]; then
    echo "error: ${ASSET} is not listed in SHA256SUMS" >&2; exit 1
  fi
  if command -v sha256sum >/dev/null 2>&1; then
    actual="$(sha256sum "$TMP/$ASSET" | awk '{print $1}')"
  else
    actual="$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')"
  fi
  if [ "$expected" != "$actual" ]; then
    echo "error: checksum mismatch for ${ASSET} (expected $expected, got $actual)" >&2
    exit 1
  fi
  echo "checksum OK for ${ASSET}" >&2
}

if [ "$DRY_RUN" -eq 0 ]; then
  verify_checksum
fi

run tar -xzf "$TMP/$ASSET" -C "$TMP"
run mkdir -p "$INSTALL_DIR"
run install -m 0755 "$TMP/$BIN" "$INSTALL_DIR/$BIN"

echo "Installed to ${INSTALL_DIR}/${BIN}" >&2
case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *) echo "Note: add ${INSTALL_DIR} to your PATH to run 'bastion' from anywhere." >&2 ;;
esac
