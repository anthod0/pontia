#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

SQLX_PREPARE_DIR="$(mktemp -d /tmp/pontia-sqlx-prepare.XXXXXX)"
SQLX_PREPARE_DB="$SQLX_PREPARE_DIR/prepare.db"

cleanup() {
  rm -f "$SQLX_PREPARE_DB" "$SQLX_PREPARE_DB-shm" "$SQLX_PREPARE_DB-wal"
  rmdir "$SQLX_PREPARE_DIR"
}
trap cleanup EXIT

for migration_file in control/storage-sqlite/migrations/*.sql; do
  sqlite3 "$SQLX_PREPARE_DB" < "$migration_file"
done

DATABASE_URL="sqlite://$SQLX_PREPARE_DB" \
  cargo sqlx prepare --workspace "$@" -- --all-targets --all-features
