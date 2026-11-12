#!/usr/bin/env bash
# Stage the @pontia/cli npm packages from release binary archives.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
artifacts=${1:?Usage: stage-npm-cli.sh ARTIFACTS_DIR NEW_STAGING_DIR}
staging=${2:?Usage: stage-npm-cli.sh ARTIFACTS_DIR NEW_STAGING_DIR}
[[ ! -e "$staging" ]] || { echo "Staging directory already exists: $staging" >&2; exit 1; }

mkdir -p "$staging"
for entry in \
  linux-x64:x86_64-unknown-linux-gnu \
  linux-arm64:aarch64-unknown-linux-gnu \
  darwin-x64:x86_64-apple-darwin \
  darwin-arm64:aarch64-apple-darwin; do
  platform=${entry%%:*}
  target=${entry#*:}
  package="$staging/cli-$platform"
  mkdir "$package"
  cp "$root/npm/cli-$platform/package.json" "$package/"
  for binary in pontia pontiad; do
    tar -xzf "$artifacts/$binary-$target.tar.gz" -C "$package" "$binary"
  done
  python3 "$root/scripts/release-version.py" materialize-npm "$package/package.json"
done

mkdir "$staging/cli"
cp "$root/npm/cli/package.json" "$root/LICENSE" "$staging/cli/"
cp -R "$root/npm/cli/bin" "$staging/cli/bin"
python3 "$root/scripts/release-version.py" materialize-npm "$staging/cli/package.json"
