default:
    just --list

dev:
    ./scripts/dev-dashboard.sh

backend:
    SQLX_OFFLINE=true PONTIA_EXTERNAL_API_TOKEN=${PONTIA_EXTERNAL_API_TOKEN:-dev-token} cargo run

dashboard:
    pnpm --dir=apps/dashboard run dev

website:
    pnpm --dir=apps/website run dev

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

sqlx-prepare:
    ./scripts/sqlx-prepare.sh

sqlx-prepare-check:
    ./scripts/sqlx-prepare.sh --check

sqlx-check:
    SQLX_OFFLINE=true cargo check --all-targets --all-features

clippy:
    SQLX_OFFLINE=true cargo clippy --all-targets --all-features -- -D warnings

test:
    SQLX_OFFLINE=true cargo test

dashboard-check:
    pnpm --dir=apps/dashboard run check

website-check:
    pnpm --dir=apps/website run check

check: fmt-check sqlx-check clippy test dashboard-check website-check
