default:
    just --list

dev:
    ./scripts/dev-dashboard.sh

backend:
    DATABASE_URL="$(./scripts/sqlx-check-db.sh)" PONTIA_EXTERNAL_API_TOKEN=${PONTIA_EXTERNAL_API_TOKEN:-dev-token} cargo run

dashboard:
    pnpm --dir=apps/dashboard run dev

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

sqlx-db:
    ./scripts/sqlx-check-db.sh

sqlx-check:
    DATABASE_URL="$(./scripts/sqlx-check-db.sh)" cargo check --all-targets --all-features

clippy:
    DATABASE_URL="$(./scripts/sqlx-check-db.sh)" cargo clippy --all-targets --all-features -- -D warnings

test:
    DATABASE_URL="$(./scripts/sqlx-check-db.sh)" cargo test

dashboard-check:
    pnpm --dir=apps/dashboard run check

check: fmt-check sqlx-check clippy test dashboard-check
