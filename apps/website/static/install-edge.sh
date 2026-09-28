#!/bin/sh
set -eu

REPOSITORY="anthod0/pontia"
INSTALL_DIR="${PONTIA_EDGE_INSTALL_DIR:-/usr/local/bin}"
VERSION="${PONTIA_EDGE_VERSION:-latest}"

fail() {
  printf 'pontia-edge installer: %s\n' "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || fail "required command not found: $1"
}

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
      fail "invalid PONTIA_EDGE_VERSION: $VERSION"
    fi
    release_path="download/$VERSION"
    ;;
  *)
    fail "PONTIA_EDGE_VERSION must be latest or a version beginning with v"
    ;;
esac

for command_name in curl tar sha256sum mktemp install mv; do
  require_command "$command_name"
done

archive_name="pontia-edge-$target.tar.gz"
release_url="https://github.com/$REPOSITORY/releases/$release_path"
temporary_dir="$(mktemp -d)"
staged_binary=""

cleanup() {
  rm -rf "$temporary_dir"
  if [ -n "$staged_binary" ]; then
    rm -f "$staged_binary"
  fi
}
trap cleanup EXIT HUP INT TERM

printf 'Downloading pontia-edge for %s...\n' "$target"
curl -fsSL "$release_url/$archive_name" -o "$temporary_dir/$archive_name"
curl -fsSL "$release_url/SHA256SUMS" -o "$temporary_dir/SHA256SUMS"

expected_checksum="$(awk -v archive="$archive_name" '$2 == archive || $2 == "*" archive { print $1; exit }' "$temporary_dir/SHA256SUMS")"
[ -n "$expected_checksum" ] || fail "release checksum not found for $archive_name"
actual_checksum="$(sha256sum "$temporary_dir/$archive_name" | awk '{ print $1 }')"
[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed for $archive_name"

tar -xzf "$temporary_dir/$archive_name" -C "$temporary_dir"
[ -f "$temporary_dir/pontia-edge" ] || fail "release archive does not contain pontia-edge"

install -d -m 0755 "$INSTALL_DIR"
staged_binary="$(mktemp "$INSTALL_DIR/.pontia-edge.XXXXXX")"
install -m 0755 "$temporary_dir/pontia-edge" "$staged_binary"
mv -f "$staged_binary" "$INSTALL_DIR/pontia-edge"
staged_binary=""

printf 'Installed pontia-edge to %s/pontia-edge\n' "$INSTALL_DIR"
printf '\nInitialize this edge with:\n  sudo pontia-edge init\n'
