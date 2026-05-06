#!/usr/bin/env bash
# Download the official LadybugDB shared C/C++ prebuilt used by the Rust lbug crate.
set -euo pipefail

VERSION="${LBUG_VERSION:-0.16.1}"
LIB_KIND="${LBUG_LIB_KIND:-shared}"
TARGET_DIR="${LBUG_TARGET_DIR:-.cache/ladybug/lib}"
INCLUDE_DIR="${LBUG_INCLUDE_DIR:-.cache/ladybug/include}"
REPOSITORY="${LBUG_GITHUB_REPOSITORY:-LadybugDB/ladybug}"

if [ "$LIB_KIND" != "shared" ]; then
  echo "This project expects the shared Ladybug library; set LBUG_LIB_KIND=shared or leave it unset." >&2
  exit 1
fi

OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
  Darwin)
    case "$ARCH" in
      x86_64) ARCHIVE="liblbug-osx-x86_64.tar.gz"; LIB_NAME="liblbug.dylib" ;;
      arm64) ARCHIVE="liblbug-osx-arm64.tar.gz"; LIB_NAME="liblbug.dylib" ;;
      *) echo "Unsupported macOS architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  Linux)
    case "$ARCH" in
      x86_64) ARCHIVE="liblbug-linux-x86_64.tar.gz"; LIB_NAME="liblbug.so" ;;
      aarch64|arm64) ARCHIVE="liblbug-linux-aarch64.tar.gz"; LIB_NAME="liblbug.so" ;;
      *) echo "Unsupported Linux architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*)
    case "$ARCH" in
      x86_64|AMD64) ARCHIVE="liblbug-windows-x86_64.zip"; LIB_NAME="lbug_shared.dll" ;;
      *) echo "Unsupported Windows architecture: $ARCH" >&2; exit 1 ;;
    esac
    ;;
  *) echo "Unsupported OS: $OS" >&2; exit 1 ;;
esac

TMPDIR="$(mktemp -d)"
trap 'rm -rf "$TMPDIR"' EXIT

if [ -f "$TARGET_DIR/$LIB_NAME" ]; then
  echo "Ladybug prebuilt already exists in $TARGET_DIR"
else
  mkdir -p "$TARGET_DIR"
  URL="https://github.com/${REPOSITORY}/releases/download/v${VERSION}/${ARCHIVE}"
  echo "Downloading $URL"
  curl -fSL "$URL" -o "$TMPDIR/$ARCHIVE"

  case "$ARCHIVE" in
    *.zip) unzip -o "$TMPDIR/$ARCHIVE" -d "$TARGET_DIR" ;;
    *) tar xzf "$TMPDIR/$ARCHIVE" -C "$TARGET_DIR" ;;
  esac

  echo "Installed $ARCHIVE to $TARGET_DIR"
fi

if [ -f "$INCLUDE_DIR/common/enums/statement_type.h" ] && [ -f "$INCLUDE_DIR/lbug.hpp" ]; then
  echo "Ladybug headers already exist in $INCLUDE_DIR"
  exit 0
fi

mkdir -p "$INCLUDE_DIR"
SOURCE_ARCHIVE="ladybug-${VERSION}.tar.gz"
SOURCE_URL="https://github.com/${REPOSITORY}/archive/refs/tags/v${VERSION}.tar.gz"
echo "Downloading $SOURCE_URL"
curl -fSL "$SOURCE_URL" -o "$TMPDIR/$SOURCE_ARCHIVE"
mkdir -p "$TMPDIR/source"
tar xzf "$TMPDIR/$SOURCE_ARCHIVE" --strip-components=1 -C "$TMPDIR/source"
cp -R "$TMPDIR/source/src/include/"* "$INCLUDE_DIR/"
# The Rust bridge also includes the public C/C++ headers shipped in the binary archive.
cp -f "$TARGET_DIR"/lbug.h "$TARGET_DIR"/lbug.hpp "$INCLUDE_DIR/"
echo "Installed Ladybug headers to $INCLUDE_DIR"
