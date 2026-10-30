# syntax=docker/dockerfile:1
FROM oven/bun:1 AS bun
FROM caddy:2 AS caddy

FROM rust:1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock VERSION ./
COPY crates ./crates
COPY .sqlx ./.sqlx
COPY scripts/build-dashboard-wasm ./scripts/build-dashboard-wasm
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    SQLX_OFFLINE=true cargo build --locked --release -p pontia -p pontiad -p pontia-edge \
    && mkdir -p /out \
    && cp target/release/pontia target/release/pontiad target/release/pontia-edge /out/ \
    && bash scripts/build-dashboard-wasm --install-bindgen

FROM node:24-bookworm-slim AS base
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl openssl bash \
    && rm -rf /var/lib/apt/lists/*

FROM base AS dashboard-build
WORKDIR /src/apps/dashboard
COPY apps/dashboard ./
COPY crates /src/crates
COPY --from=rust-build /src/apps/dashboard/src/e2e-wasm ./src/e2e-wasm
RUN bun install --frozen-lockfile \
    && bun run build && mkdir -p /out && cp -a dist /out/local \
    && bun run build:public && cp -a dist /out/public

FROM base AS cloud
WORKDIR /app
COPY apps/cloud ./
RUN bun install --frozen-lockfile && bun run build
COPY docker/e2e/cloud-entrypoint /usr/local/bin/cloud-entrypoint
COPY docker/e2e/cloud-seed.sql /app/e2e-seed.sql
ENV WRANGLER_SEND_METRICS=false CI=true
ENTRYPOINT ["bash", "/usr/local/bin/cloud-entrypoint"]

FROM base AS dashboard
COPY --from=caddy /usr/bin/caddy /usr/local/bin/caddy
COPY --from=dashboard-build /out/public /srv/dashboard
COPY docker/e2e/Caddyfile /etc/caddy/Caddyfile
CMD ["caddy", "run", "--config", "/etc/caddy/Caddyfile", "--adapter", "caddyfile"]

FROM base AS edge
COPY --from=rust-build /out/pontia-edge /usr/local/bin/pontia-edge
CMD ["pontia-edge"]

FROM base AS device
RUN apt-get update && apt-get install -y --no-install-recommends \
    git tmux ripgrep procps python3 \
    && rm -rf /var/lib/apt/lists/* \
    && npm install --global @earendil-works/pi-coding-agent @openai/codex \
    && usermod --login e2e --home /home/e2e --move-home node \
    && groupmod --new-name e2e node
COPY --from=rust-build /out/pontia /out/pontiad /usr/local/bin/
COPY --from=dashboard-build /out/local /opt/pontia/dashboard
COPY clients/pi /opt/pontia/clients/pi
COPY docker/e2e/device-entrypoint /usr/local/bin/device-entrypoint
COPY docker/e2e/device.toml /opt/pontia/config.toml
RUN mkdir -p /home/e2e/.pontia /home/e2e/.pi/agent /home/e2e/.codex /home/e2e/workspace \
    && chown -R 1000:1000 /home/e2e
ENV HOME=/home/e2e PONTIA_HOME=/home/e2e/.pontia
USER 1000:1000
WORKDIR /home/e2e/workspace
ENTRYPOINT ["bash", "/usr/local/bin/device-entrypoint"]

FROM base AS browser
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium xvfb x11vnc novnc websockify openbox dbus-x11 x11-utils fonts-liberation \
    && rm -rf /var/lib/apt/lists/* \
    && usermod --login e2e --home /home/e2e --move-home node \
    && groupmod --new-name e2e node \
    && chown -R 1000:1000 /home/e2e
COPY --from=caddy /usr/bin/caddy /usr/local/bin/caddy
COPY docker/e2e/browser.Caddyfile /etc/caddy/Caddyfile
COPY docker/e2e/browser-entrypoint /usr/local/bin/browser-entrypoint
COPY docker/e2e/chromium-policy.json /etc/chromium/policies/managed/e2e.json
ENV HOME=/home/e2e DISPLAY=:99
USER 1000:1000
ENTRYPOINT ["bash", "/usr/local/bin/browser-entrypoint"]
