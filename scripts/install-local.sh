#!/usr/bin/env bash
set -Eeuo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
HOME_DIR="${HOME:?HOME must be set}"
PREFIX="${PREFIX:-$HOME_DIR/.local}"
PONTIA_HOME="${PONTIA_HOME:-$HOME_DIR/.pontia}"
BIN_DIR="$PREFIX/bin"
DASHBOARD_DIR="$PREFIX/share/pontia/dashboard"
CONFIG_FILE="$PONTIA_HOME/config.toml"
TARGET_DIR="${CARGO_TARGET_DIR:-target}"

validate_root() {
  local name="$1"
  local value="$2"
  if [[ "$value" != /* || "$value" == "/" || "$value/" == *"/../"* ]]; then
    echo "$name must be a non-root absolute path without parent traversal: $value" >&2
    exit 1
  fi
}

validate_root PREFIX "$PREFIX"
validate_root PONTIA_HOME "$PONTIA_HOME"

for command in cargo install pnpm; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "Required command not found: $command" >&2
    exit 1
  fi
done

cd "$REPO_ROOT"

echo "Installing Dashboard dependencies..."
pnpm --dir apps/dashboard install --frozen-lockfile

echo "Building Dashboard..."
pnpm --dir apps/dashboard run build

echo "Building release binaries..."
SQLX_OFFLINE=true cargo build --release -p pontia -p pontiad

if [[ "$TARGET_DIR" != /* ]]; then
  TARGET_DIR="$REPO_ROOT/$TARGET_DIR"
fi

mkdir -p "$BIN_DIR" "$(dirname "$DASHBOARD_DIR")"
for binary in pontia pontiad; do
  source_path="$TARGET_DIR/release/$binary"
  destination="$BIN_DIR/$binary"
  temporary="$destination.tmp.$$"
  install -m 0755 "$source_path" "$temporary"
  mv -f "$temporary" "$destination"
  echo "Installed $binary: $destination"
done

staged_dashboard="$DASHBOARD_DIR.tmp.$$"
previous_dashboard="$DASHBOARD_DIR.previous.$$"
cleanup() {
  rm -rf "$staged_dashboard"
  if [[ -e "$previous_dashboard" ]]; then
    if [[ -e "$DASHBOARD_DIR" ]]; then
      rm -rf "$previous_dashboard"
    else
      mv "$previous_dashboard" "$DASHBOARD_DIR" || true
    fi
  fi
}
trap cleanup EXIT

cp -a "$REPO_ROOT/apps/dashboard/dist" "$staged_dashboard"
if [[ -e "$DASHBOARD_DIR" ]]; then
  mv "$DASHBOARD_DIR" "$previous_dashboard"
fi
mv "$staged_dashboard" "$DASHBOARD_DIR"
rm -rf "$previous_dashboard"
echo "Installed Dashboard: $DASHBOARD_DIR"

if [[ ! -e "$CONFIG_FILE" ]]; then
  mkdir -p "$(dirname "$CONFIG_FILE")"
  escaped_dashboard=${DASHBOARD_DIR//\\/\\\\}
  escaped_dashboard=${escaped_dashboard//\"/\\\"}
  temporary_config="$CONFIG_FILE.tmp.$$"
  cat > "$temporary_config" <<EOF
[dashboard]
source = "$escaped_dashboard"
EOF
  mv "$temporary_config" "$CONFIG_FILE"
  echo "Created config: $CONFIG_FILE"
else
  echo "Existing config left unchanged: $CONFIG_FILE"
  echo "Ensure [dashboard].source points to: $DASHBOARD_DIR"
fi

printf '\nLocal installation complete. Start it explicitly when ready:\n  PONTIA_HOME=%q %q up\n' \
  "$PONTIA_HOME" "$BIN_DIR/pontia"
