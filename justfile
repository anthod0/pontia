default:
    just --list

dev:
    ./scripts/dev.sh

dev-backend:
    SQLX_OFFLINE=true PONTIA_EXTERNAL_API_TOKEN=${PONTIA_EXTERNAL_API_TOKEN:-dev-token} cargo run -p pontiad

dev-dashboard:
    bun run --cwd apps/dashboard dev

dev-dashboard-public:
    VITE_DASHBOARD_MODE=public bun run --cwd apps/dashboard dev

dev-cloud:
    bun run --cwd apps/cloud dev

install-local:
    ./scripts/install-local.sh

fmt:
    cargo fmt
    bun run --cwd apps/dashboard fmt
    bun run --cwd apps/cloud fmt
    bun run --cwd clients/pi fmt

fmt-check:
    cargo fmt --check
    bun run --cwd apps/dashboard fmt:check
    bun run --cwd apps/cloud fmt:check
    bun run --cwd clients/pi fmt:check

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
    bun run --cwd apps/dashboard check

dashboard-test:
    bun run --cwd apps/dashboard test

pi-client-test:
    bun run --cwd clients/pi test
    bun run --cwd clients/pi typecheck

cloud-check:
    bun run --cwd apps/cloud check

check: fmt-check sqlx-check clippy test dashboard-check dashboard-test pi-client-test cloud-check
