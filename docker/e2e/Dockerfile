# syntax=docker/dockerfile:1
FROM oven/bun:1 AS bun
FROM caddy:2 AS caddy

FROM node:24-bookworm-slim AS base
COPY --from=bun /usr/local/bin/bun /usr/local/bin/bun
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates curl openssl bash python3 \
    && rm -rf /var/lib/apt/lists/*

FROM base AS claude-build
WORKDIR /src
COPY clients/claude ./clients/claude
COPY clients/shared ./clients/shared
COPY scripts/pack-claude-client.sh scripts/release-version.py ./scripts/
COPY VERSION LICENSE ./
RUN bash scripts/pack-claude-client.sh /out/claude-client.tgz \
    && npm install --prefix /opt/pontia/claude-client --omit=dev --no-audit --no-fund /out/claude-client.tgz \
    && node /opt/pontia/claude-client/node_modules/@pontia/claude-client/dist/src/main.js preflight

FROM base AS claude-proxy
ARG TARGETARCH
RUN python3 - "$TARGETARCH" <<'PY'
import hashlib
import io
import json
from pathlib import Path
import sys
import tarfile
import urllib.request

base = "https://api.github.com/repos/router-for-me/CLIProxyAPI/releases/latest"
release = json.load(urllib.request.urlopen(base))
arch = {"amd64": "amd64", "arm64": "aarch64"}[sys.argv[1]]
asset = next(a for a in release["assets"] if a["name"].endswith(f"_linux_{arch}_no-plugin.tar.gz"))
checksums = next(a for a in release["assets"] if a["name"] == "checksums.txt")
archive = urllib.request.urlopen(asset["browser_download_url"]).read()
checks = urllib.request.urlopen(checksums["browser_download_url"]).read().decode()
expected = next(line.split()[0] for line in checks.splitlines() if line.endswith(asset["name"]))
if hashlib.sha256(archive).hexdigest() != expected:
    raise ValueError("CLIProxyAPI checksum mismatch")
with tarfile.open(fileobj=io.BytesIO(archive)) as package:
    binary = Path("/usr/local/bin/CLIProxyAPI")
    binary.write_bytes(package.extractfile("cli-proxy-api").read())
    binary.chmod(0o755)
print("Installed CLIProxyAPI", release["tag_name"])
PY

FROM base AS grok
ARG TARGETARCH
RUN set -eu; \
    case "$TARGETARCH" in \
        amd64) arch=x86_64; sha=add4d201dbf2c0bd6077a36feb55a76939de30bae48dbb06f16b2307d38ceacb ;; \
        arm64) arch=aarch64; sha=8c74f1dc6cdc1befff6b9faada06bec4cf6759d04f2a60cdc28ece5ff696b44f ;; \
        *) exit 1 ;; \
    esac; \
    curl -fsSL "https://storage.googleapis.com/grok-build-public-artifacts/cli/grok-1.0.92-linux-${arch}" -o /usr/local/bin/grok; \
    echo "$sha  /usr/local/bin/grok" | sha256sum -c -; \
    chmod 755 /usr/local/bin/grok

FROM rust:1-bookworm AS rust-build
WORKDIR /src
COPY Cargo.toml Cargo.lock VERSION ./
COPY crates ./crates
COPY scripts/build-dashboard-wasm ./scripts/build-dashboard-wasm
RUN rustup target add wasm32-unknown-unknown
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target \
    cargo build --locked --release -p pontia -p pontiad -p pontia-edge \
    && mkdir -p /out \
    && cp target/release/pontia target/release/pontiad target/release/pontia-edge /out/ \
    && bash scripts/build-dashboard-wasm --install-bindgen

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
# The banner's npm update check can keep a socket alive after local D1 commands finish.
ENV WRANGLER_HIDE_BANNER=true WRANGLER_SEND_METRICS=false CI=true
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
    && npm install --global @earendil-works/pi-coding-agent @openai/codex @anthropic-ai/claude-code \
    && usermod --login e2e --home /home/e2e --move-home node \
    && groupmod --new-name e2e node
COPY --from=rust-build /out/pontia /out/pontiad /usr/local/bin/
COPY --from=grok /usr/local/bin/grok /usr/local/bin/grok
COPY --from=claude-proxy /usr/local/bin/CLIProxyAPI /usr/local/bin/CLIProxyAPI
COPY --from=dashboard-build /out/local /opt/pontia/dashboard
COPY clients/pi /opt/pontia/clients/pi
COPY clients/shared /opt/pontia/clients/shared
COPY clients/shared /opt/pontia/shared
COPY --from=claude-build /src/clients/claude /opt/pontia/claude-validation
COPY --from=claude-build /opt/pontia/claude-client /opt/pontia/claude-client
COPY docker/e2e/device-entrypoint /usr/local/bin/device-entrypoint
COPY docker/e2e/configure-grok.py /usr/local/lib/configure-grok.py
COPY docker/e2e/configure-pi-auth.py /usr/local/lib/configure-pi-auth.py
COPY docker/e2e/configure-claude-proxy.py /usr/local/lib/configure-claude-proxy.py
COPY docker/e2e/device.toml /opt/pontia/config.toml
RUN mkdir -p /home/e2e/.pontia /home/e2e/.pi/agent /home/e2e/.codex /home/e2e/.claude /home/e2e/workspace \
    && chown -R 1000:1000 /home/e2e
ENV HOME=/home/e2e PONTIA_HOME=/home/e2e/.pontia
ENV CLAUDE_CONFIG_DIR=/home/e2e/.claude
ENV GROK_DISABLE_AUTOUPDATER=1
ENV PONTIA_CLAUDE_HOST_PATH=/opt/pontia/claude-client/node_modules/@pontia/claude-client/dist/src/main.js
USER 1000:1000
WORKDIR /home/e2e/workspace
ENTRYPOINT ["bash", "/usr/local/bin/device-entrypoint"]

FROM base AS browser
RUN apt-get update && apt-get install -y --no-install-recommends \
    chromium xvfb x11vnc novnc websockify openbox dbus-x11 x11-utils fonts-liberation fonts-noto-cjk \
    && rm -rf /var/lib/apt/lists/* \
    && usermod --login e2e --home /home/e2e --move-home node \
    && groupmod --new-name e2e node \
    && chown -R 1000:1000 /home/e2e
COPY --from=caddy /usr/bin/caddy /usr/local/bin/caddy
COPY docker/e2e/browser.Caddyfile /etc/caddy/Caddyfile
COPY docker/e2e/browser-entrypoint /usr/local/bin/browser-entrypoint
COPY docker/e2e/browser-login.mjs /usr/local/lib/browser-login.mjs
COPY docker/e2e/chromium-policy.json /etc/chromium/policies/managed/e2e.json
ENV HOME=/home/e2e DISPLAY=:99
USER 1000:1000
ENTRYPOINT ["bash", "/usr/local/bin/browser-entrypoint"]
