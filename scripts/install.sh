#!/bin/sh
set -eu

RELEASE_ORIGIN="https://get.pontia.dev"
RELEASE_PUBLIC_KEY='-----BEGIN PUBLIC KEY-----
MCowBQYDK2VwAyEAFg9NXrmnbFNdwOtPQiOzp5ZPhFTaSjiEPq/RFcPwq4c=
-----END PUBLIC KEY-----'
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

for command_name in curl grep install jq mktemp mv rm tar tr uname wc; do
  require_command "$command_name"
done

system="$(uname -s)"
architecture="$(uname -m)"
case "$system" in
  Linux)
    require_command sha256sum
    checksum() {
      sha256sum "$1" | { read -r digest _; printf '%s\n' "$digest"; }
    }
    case "$architecture" in
      x86_64|amd64) target="x86_64-unknown-linux-gnu" ;;
      aarch64|arm64) target="aarch64-unknown-linux-gnu" ;;
      *) fail "unsupported architecture: $architecture" ;;
    esac
    ;;
  Darwin)
    require_command shasum
    checksum() {
      shasum -a 256 "$1" | { read -r digest _; printf '%s\n' "$digest"; }
    }
    case "$architecture" in
      x86_64|amd64) target="x86_64-apple-darwin" ;;
      aarch64|arm64) target="aarch64-apple-darwin" ;;
      *) fail "unsupported architecture: $architecture" ;;
    esac
    ;;
  *)
    fail "unsupported operating system: $system"
    ;;
esac

OPENSSL="${PONTIA_OPENSSL:-}"
if [ -z "$OPENSSL" ] && [ "$system" = Darwin ]; then
  for candidate in /opt/homebrew/opt/openssl@3/bin/openssl /usr/local/opt/openssl@3/bin/openssl; do
    if [ -x "$candidate" ]; then
      OPENSSL="$candidate"
      break
    fi
  done
fi
if [ -z "$OPENSSL" ]; then
  OPENSSL="$(command -v openssl || true)"
fi
[ -n "$OPENSSL" ] || fail "required command not found: openssl"

MAX_MANIFEST_SIZE=1048576
MAX_ARCHIVE_SIZE=536870912

download() {
  url="$1"
  output="$2"
  limit="$3"
  blocks=$(( (limit + 511) / 512 ))
  (ulimit -f "$blocks"; curl -fsSL "$url" -o "$output" --max-filesize "$limit")
}

case "$VERSION" in
  latest)
    manifest_path="channels/stable.json"
    ;;
  v[0-9]*)
    if ! printf '%s' "$VERSION" | grep -Eq '^v[0-9][0-9A-Za-z.+-]*$'; then
      fail "invalid PONTIA_VERSION: $VERSION"
    fi
    manifest_path="releases/$VERSION/manifest.json"
    ;;
  *)
    fail "PONTIA_VERSION must be latest or a version beginning with v"
    ;;
esac

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

download "$RELEASE_ORIGIN/$manifest_path" "$temporary_dir/envelope.json" "$MAX_MANIFEST_SIZE"
printf '%s\n' "$RELEASE_PUBLIC_KEY" > "$temporary_dir/public.pem"
jq -er '.manifest' "$temporary_dir/envelope.json" | "$OPENSSL" base64 -d -A > "$temporary_dir/manifest.json"
jq -er '.signature' "$temporary_dir/envelope.json" | "$OPENSSL" base64 -d -A > "$temporary_dir/signature"
"$OPENSSL" pkeyutl -verify -rawin -pubin -inkey "$temporary_dir/public.pem" -sigfile "$temporary_dir/signature" -in "$temporary_dir/manifest.json" >/dev/null || fail "release signature verification failed"
jq -e '.schema_version == 1 and (.version | test("^v[0-9][0-9A-Za-z.+-]*$"))' "$temporary_dir/manifest.json" >/dev/null || fail "invalid release manifest"
release_version="$(jq -er '.version' "$temporary_dir/manifest.json")"
[ "$VERSION" = latest ] || [ "$VERSION" = "$release_version" ] || fail "release version mismatch"

for binary in pontia pontiad; do
  archive_name="$binary-$target.tar.gz"
  printf 'Downloading %s for %s...\n' "$binary" "$target"
  archive_url="$RELEASE_ORIGIN/releases/$release_version/$archive_name"
  manifest_url="$(jq -er --arg archive "$archive_name" '.artifacts[$archive].url' "$temporary_dir/manifest.json")"
  [ "$manifest_url" = "$archive_url" ] || fail "unexpected artifact URL"
  expected_size="$(jq -er --arg archive "$archive_name" --argjson maximum "$MAX_ARCHIVE_SIZE" '.artifacts[$archive].size | select(type == "number" and . > 0 and . <= $maximum and floor == .)' "$temporary_dir/manifest.json")"
  download "$archive_url" "$temporary_dir/$archive_name" "$expected_size"

  actual_size="$(wc -c < "$temporary_dir/$archive_name" | tr -d '[:space:]')"
  [ "$actual_size" = "$expected_size" ] || fail "size verification failed for $archive_name"
  expected_checksum="$(jq -er --arg archive "$archive_name" '.artifacts[$archive].sha256' "$temporary_dir/manifest.json")"
  actual_checksum="$(checksum "$temporary_dir/$archive_name")"
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
