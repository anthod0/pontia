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
    ./apps/dashboard/node_modules/.bin/oxfmt clients/claude/src clients/claude/test

fmt-check:
    cargo fmt --check
    bun run --cwd apps/dashboard fmt:check
    bun run --cwd apps/cloud fmt:check
    bun run --cwd clients/pi fmt:check
    ./apps/dashboard/node_modules/.bin/oxfmt --check clients/claude/src clients/claude/test

sqlx-prepare:
    ./scripts/sqlx-prepare.sh

sqlx-prepare-check:
    ./scripts/sqlx-prepare.sh --check

sqlx-check:
    SQLX_OFFLINE=true cargo check --workspace --all-targets --all-features

macos-check:
    just macos-check-target aarch64-apple-darwin
    just macos-check-target x86_64-apple-darwin

[private]
macos-check-target target:
    docker run --rm --network host --security-opt label=disable \
        --mount type=bind,source="{{ justfile_directory() }}",target=/io,readonly \
        --mount type=volume,source=pontia-macos-cargo-registry,target=/usr/local/cargo/registry \
        --mount type=volume,source=pontia-macos-cargo-git,target=/usr/local/cargo/git \
        --mount type=volume,source=pontia-macos-rustup,target=/usr/local/rustup \
        --mount type=volume,source=pontia-macos-zig,target=/opt/pontia-zig \
        --mount type=volume,source=pontia-macos-target,target=/target \
        --workdir /io \
        --env CARGO_TARGET_DIR=/target \
        --env SQLX_OFFLINE=true \
        --env MACOSX_DEPLOYMENT_TARGET=13.5 \
        ghcr.io/rust-cross/cargo-zigbuild@sha256:d8313491ec5798de0633fdc1c5753761bff79967bea69076020dc78121b2cca8 \
        sh -c 'set -eu; zig_dir="/opt/pontia-zig/zig-$(uname -m)-linux-0.15.2"; if [ ! -x "$zig_dir/zig" ]; then curl -fsSL "https://ziglang.org/download/0.15.2/zig-$(uname -m)-linux-0.15.2.tar.xz" | tar -xJ -C /opt/pontia-zig; fi; export CARGO_ZIGBUILD_ZIG_PATH="$zig_dir/zig"; rustup toolchain install stable --profile minimal; rustup target add --toolchain stable "$1"; cargo +stable zigbuild --locked --target "$1" --package pontia --package pontiad' \
        sh "{{ target }}"

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

claude-client-test:
    bun run --cwd clients/claude typecheck
    bun run --cwd clients/claude test

claude-delivery-test:
    #!/usr/bin/env bash
    set -euo pipefail
    test_root=$(mktemp -d)
    trap 'rm -rf "$test_root"' EXIT
    bash scripts/build-claude-host.sh "$test_root/host.tar.gz"
    PONTIA_CLAUDE_BUNDLE="$test_root/host.tar.gz" SQLX_OFFLINE=true cargo test -p pontia-client-claude --test delivery -- --ignored

cloud-check:
    bun run --cwd apps/cloud check

check: fmt-check sqlx-check clippy test dashboard-check dashboard-test pi-client-test claude-client-test cloud-check
