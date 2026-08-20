#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dashboard_dir="$repo_root/apps/dashboard"
artifact="$repo_root/pontia-dashboard.tar.gz"

command -v corepack >/dev/null 2>&1 || {
  echo "corepack is required to build the Dashboard release artifact" >&2
  exit 1
}

(
  cd "$dashboard_dir"
  corepack pnpm install --frozen-lockfile
  corepack pnpm run build
)

tar -czf "$artifact" -C "$dashboard_dir/dist" .
echo "Created $artifact"
