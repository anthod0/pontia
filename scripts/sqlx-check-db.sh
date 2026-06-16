#!/usr/bin/env bash
set -euo pipefail

DB_PATH="${PONTIA_SQLX_CHECK_DB_PATH:-/tmp/pontia_sqlx_check.db}"

rm -f "$DB_PATH" "$DB_PATH-shm" "$DB_PATH-wal"

for migration in migrations/*.sql; do
  sqlite3 "$DB_PATH" < "$migration"
done

printf 'sqlite://%s\n' "$DB_PATH"
