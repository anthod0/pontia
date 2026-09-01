default:
    just --list

dev:
    ./scripts/dev.sh

dev-backend:
    SQLX_OFFLINE=true PONTIA_EXTERNAL_API_TOKEN=${PONTIA_EXTERNAL_API_TOKEN:-dev-token} cargo run -p pontiad

dev-dashboard:
    pnpm --dir=apps/dashboard run dev

dev-website:
    pnpm --dir=apps/website run dev

install-local:
    ./scripts/install-local.sh

fmt:
    cargo fmt

fmt-check:
    cargo fmt --check

sqlx-prepare:
    ./scripts/sqlx-prepare.sh

sqlx-prepare-check:
    ./scripts/sqlx-prepare.sh --check

sqlx-check:
    SQLX_OFFLINE=true cargo check --workspace --all-targets --all-features

clippy:
    SQLX_OFFLINE=true cargo clippy --workspace --all-targets --all-features -- -D warnings

test:
    SQLX_OFFLINE=true cargo test --workspace

dashboard-check:
    pnpm --dir=apps/dashboard run check

dashboard-test:
    pnpm --dir=apps/dashboard run test

pi-client-test:
    pnpm --dir=clients/pi run test
    pnpm --dir=clients/pi run typecheck

website-check:
    pnpm --dir=apps/website run check

check: fmt-check sqlx-check clippy test dashboard-check dashboard-test pi-client-test website-check
