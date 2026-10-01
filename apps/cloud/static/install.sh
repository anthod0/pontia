#!/bin/sh
set -eu

REPOSITORY="anthod0/pontia"
HOME_DIR="${HOME:?HOME must be set}"
INSTALL_DIR="${PONTIA_INSTALL_DIR:-$HOME_DIR/.local/bin}"
VERSION="${PONTIA_VERSION:-latest}"

fail() {
  printf 'pontia installer: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

for command_name in awk curl grep install mktemp mv rm sha256sum tar uname; do
  require_command "$command_name"
done

[ "$(uname -s)" = "Linux" ] || fail "only Linux is supported"

case "$(uname -m)" in
  x86_64|amd64)
    target="x86_64-unknown-linux-gnu"
    ;;
  aarch64|arm64)
    target="aarch64-unknown-linux-gnu"
    ;;
  *)
    fail "unsupported architecture: $(uname -m)"
    ;;
esac

case "$VERSION" in
  latest)
    release_path="latest/download"
    ;;
  v[0-9]*)
    if ! printf '%s' "$VERSION" | grep -Eq '^v[0-9][0-9A-Za-z.+-]*$'; then
      fail "invalid PONTIA_VERSION: $VERSION"
    fi
    release_path="download/$VERSION"
    ;;
  *)
    fail "PONTIA_VERSION must be latest or a version beginning with v"
    ;;
esac

release_url="https://github.com/$REPOSITORY/releases/$release_path"
temporary_dir="$(mktemp -d)"
staged_pontia=""
staged_pontiad=""

cleanup() {
  rm -rf "$temporary_dir"
  if [ -n "$staged_pontia" ]; then
    rm -f "$staged_pontia"
  fi
  if [ -n "$staged_pontiad" ]; then
    rm -f "$staged_pontiad"
  fi
}
trap cleanup EXIT HUP INT TERM

curl -fsSL "$release_url/SHA256SUMS" -o "$temporary_dir/SHA256SUMS"

for binary in pontia pontiad; do
  archive_name="$binary-$target.tar.gz"
  printf 'Downloading %s for %s...\n' "$binary" "$target"
  curl -fsSL "$release_url/$archive_name" -o "$temporary_dir/$archive_name"

  expected_checksum="$(awk -v archive="$archive_name" '$2 == archive || $2 == "*" archive { print $1; exit }' "$temporary_dir/SHA256SUMS")"
  [ -n "$expected_checksum" ] || fail "release checksum not found for $archive_name"
  actual_checksum="$(sha256sum "$temporary_dir/$archive_name" | awk '{ print $1 }')"
  [ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed for $archive_name"

  tar -xzf "$temporary_dir/$archive_name" -C "$temporary_dir"
  [ -f "$temporary_dir/$binary" ] || fail "release archive does not contain $binary"
done

install -d -m 0755 "$INSTALL_DIR"
staged_pontia="$(mktemp "$INSTALL_DIR/.pontia.XXXXXX")"
staged_pontiad="$(mktemp "$INSTALL_DIR/.pontiad.XXXXXX")"
install -m 0755 "$temporary_dir/pontia" "$staged_pontia"
install -m 0755 "$temporary_dir/pontiad" "$staged_pontiad"
mv -f "$staged_pontia" "$INSTALL_DIR/pontia"
staged_pontia=""
mv -f "$staged_pontiad" "$INSTALL_DIR/pontiad"
staged_pontiad=""

printf 'Installed pontia and pontiad to %s\n' "$INSTALL_DIR"
