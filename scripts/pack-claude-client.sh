#!/usr/bin/env bash
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
destination=${1:?Usage: pack-claude-client.sh NEW_PACKAGE_PATH}
[[ ! -e "$destination" ]] || { echo "Destination already exists: $destination" >&2; exit 1; }
stage=$(mktemp -d)
trap 'rm -rf "$stage"' EXIT
bun install --cwd "$root/clients/claude" --frozen-lockfile
bun run --cwd "$root/clients/claude" build
cp "$root/clients/claude/package.json" "$root/LICENSE" "$stage/"
mkdir "$stage/dist"
cp -R "$root/clients/claude/dist/src" "$stage/dist/src"
python3 "$root/scripts/release-version.py" materialize-npm "$stage/package.json"
package=$(cd "$stage" && npm pack --ignore-scripts --silent)
mkdir -p "$(dirname "$destination")"
mv "$stage/$package" "$destination"
