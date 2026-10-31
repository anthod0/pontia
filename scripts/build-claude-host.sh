#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
destination=${1:?Usage: build-claude-host.sh NEW_ARCHIVE_PATH}
[[ ! -e "$destination" ]] || { echo "Destination already exists: $destination" >&2; exit 1; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
cp "$root/clients/claude/package.json" "$root/clients/claude/bun.lock" "$stage/"
bun install --cwd "$root/clients/claude" --frozen-lockfile
bun run --cwd "$root/clients/claude" build
bun install --cwd "$stage" --production --frozen-lockfile
mkdir -p "$stage/dist"
cp -R "$root/clients/claude/dist/src" "$stage/dist/src"
python3 "$root/scripts/release-version.py" materialize-npm "$stage/package.json"
node "$stage/dist/src/main.js" preflight
# Dereference package-manager links; the signed binary carries all runtime dependencies.
mkdir -p "$(dirname "$destination")"
tar -czhf "$destination" -C "$stage" package.json dist node_modules
