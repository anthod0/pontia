#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
dashboard_dir="$repo_root/apps/dashboard"
artifact="$repo_root/pontia-dashboard.tar.gz"

command -v bun >/dev/null 2>&1 || {
  echo "bun is required to build the Dashboard release artifact" >&2
  exit 1
}

(
  cd "$dashboard_dir"
  bun install --frozen-lockfile
  bun run build
)

tar -czf "$artifact" -C "$dashboard_dir/dist" .
echo "Created $artifact"
