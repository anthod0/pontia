#!/bin/sh
set -eu

RELEASE_ORIGIN="https://get.pontia.dev"
RELEASE_PUBLIC_KEY='-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAFg9NXrmnbFNdwOtPQiOzp5ZPhFTaSjiEPq/RFcPwq4c=
-----END PUBLIC KEY-----'
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
    manifest_path="channels/stable.json"
    ;;
  v[0-9]*)
    if ! printf '%s' "$VERSION" | grep -Eq '^v[0-9][0-9A-Za-z.+-]*$'; then
      fail "invalid PONTIA_EDGE_VERSION: $VERSION"
    fi
    manifest_path="releases/$VERSION/manifest.json"
    ;;
  *)
    fail "PONTIA_EDGE_VERSION must be latest or a version beginning with v"
    ;;
esac

for command_name in curl grep jq openssl rm tar sha256sum mktemp install mv; do
  require_command "$command_name"
done

archive_name="pontia-edge-$target.tar.gz"
temporary_dir="$(mktemp -d)"
staged_binary=""

cleanup() {
  rm -rf "$temporary_dir"
  if [ -n "$staged_binary" ]; then
    rm -f "$staged_binary"
  fi
}
trap cleanup EXIT HUP INT TERM

curl -fsSL "$RELEASE_ORIGIN/$manifest_path" -o "$temporary_dir/envelope.json"
printf '%s\n' "$RELEASE_PUBLIC_KEY" > "$temporary_dir/public.pem"
jq -er '.manifest' "$temporary_dir/envelope.json" | openssl base64 -d -A > "$temporary_dir/manifest.json"
jq -er '.signature' "$temporary_dir/envelope.json" | openssl base64 -d -A > "$temporary_dir/signature"
openssl pkeyutl -verify -rawin -pubin -inkey "$temporary_dir/public.pem" -sigfile "$temporary_dir/signature" -in "$temporary_dir/manifest.json" >/dev/null || fail "release signature verification failed"
jq -e '.schema_version == 1 and (.version | test("^v[0-9][0-9A-Za-z.+-]*$"))' "$temporary_dir/manifest.json" >/dev/null || fail "invalid release manifest"
release_version="$(jq -er '.version' "$temporary_dir/manifest.json")"
[ "$VERSION" = latest ] || [ "$VERSION" = "$release_version" ] || fail "release version mismatch"
archive_url="$RELEASE_ORIGIN/releases/$release_version/$archive_name"
manifest_url="$(jq -er --arg archive "$archive_name" '.artifacts[$archive].url' "$temporary_dir/manifest.json")"
[ "$manifest_url" = "$archive_url" ] || fail "unexpected artifact URL"
printf 'Downloading pontia-edge for %s...\n' "$target"
curl -fsSL "$archive_url" -o "$temporary_dir/$archive_name"

expected_checksum="$(jq -er --arg archive "$archive_name" '.artifacts[$archive].sha256' "$temporary_dir/manifest.json")"
actual_checksum="$(sha256sum "$temporary_dir/$archive_name")"
actual_checksum="${actual_checksum%% *}"
[ "$actual_checksum" = "$expected_checksum" ] || fail "checksum verification failed for $archive_name"

tar -xzf "$temporary_dir/$archive_name" -C "$temporary_dir"
[ -f "$temporary_dir/pontia-edge" ] || fail "release archive does not contain pontia-edge"

install -d -m 0755 "$INSTALL_DIR"
staged_binary="$(mktemp "$INSTALL_DIR/.pontia-edge.XXXXXX")"
install -m 0755 "$temporary_dir/pontia-edge" "$staged_binary"
mv -f "$staged_binary" "$INSTALL_DIR/pontia-edge"
staged_binary=""

printf 'Installed pontia-edge to %s/pontia-edge\n' "$INSTALL_DIR"
