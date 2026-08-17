# syntax=docker/dockerfile:1.7

# KGP self-contained local Vibe Kanban.
#
# Derived from the repository-root ./Dockerfile, with two deliberate changes:
#   1. VK_SHARED_API_BASE / VK_SHARED_RELAY_API_BASE are never set, so
#      crates/local-deployment logs "remote features disabled" and the app talks
#      to nothing outside this container.
#   2. The runtime stage ships Node.js, so `npx -y @anthropic-ai/claude-code`
#      actually runs inside the container (the root image has no Node and
#      therefore cannot execute any coding agent).
#
# Build context is the repository root. See ./docker-compose.yml.

# ---------------------------------------------------------------- frontend ---
FROM node:22-bookworm-slim AS fe-builder

WORKDIR /app

ENV PNPM_HOME=/pnpm
ENV PATH=${PNPM_HOME}:${PATH}
ENV NODE_OPTIONS=--max-old-space-size=4096

RUN corepack enable && pnpm config set store-dir /pnpm/store

COPY pnpm-lock.yaml pnpm-workspace.yaml package.json ./
COPY packages/local-web/package.json packages/local-web/package.json
COPY packages/ui/package.json packages/ui/package.json
COPY packages/web-core/package.json packages/web-core/package.json

RUN --mount=type=cache,id=kgp-pnpm,target=/pnpm/store \
    pnpm install --frozen-lockfile

COPY packages/local-web/ packages/local-web/
COPY packages/public/ packages/public/
COPY packages/ui/ packages/ui/
COPY packages/web-core/ packages/web-core/
COPY shared/ shared/

# No VITE_VK_SHARED_API_BASE => remoteApi.ts BUILD_TIME_API_BASE is '' => the
# frontend never points at a cloud instance.
RUN pnpm -C packages/local-web build

# ----------------------------------------------------------------- backend ---
FROM rust:1.93-slim-bookworm AS builder

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_NET_GIT_FETCH_WITH_CLI=true
ENV CARGO_TARGET_DIR=/app/target

WORKDIR /app

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    build-essential \
    ca-certificates \
    git \
    libclang-dev \
    libssl-dev \
    pkg-config \
  && rm -rf /var/lib/apt/lists/*

# Pre-install the pinned toolchain (rust-toolchain.toml is nightly-2025-12-04)
# on its own layer so source edits don't re-trigger the rustup download.
COPY rust-toolchain.toml ./
RUN cargo --version >/dev/null

COPY . .

# Must land after `COPY . .` so the freshly built assets win over anything
# stale that came in from the build context.
COPY --from=fe-builder /app/packages/local-web/dist packages/local-web/dist

# Only the server bin: a bare workspace build pulls in crates/tauri-app, which
# needs glib/GTK dev libs this image does not have.
RUN --mount=type=cache,id=kgp-cargo-registry,target=/usr/local/cargo/registry \
    --mount=type=cache,id=kgp-cargo-git,target=/usr/local/cargo/git \
    --mount=type=cache,id=kgp-workspace-target,target=/app/target \
    cargo build --locked --release --bin server \
 && cp /app/target/release/server /usr/local/bin/server

# ----------------------------------------------------------------- runtime ---
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
  && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    less \
    openssh-client \
    procps \
    ripgrep \
    tini \
    wget \
  && rm -rf /var/lib/apt/lists/* \
  && useradd --system --create-home --uid 10001 appuser

# Node.js for the coding agents. Both images are bookworm-based, so the glibc
# ABI matches and the binaries can simply be copied across.
COPY --from=node:22-bookworm-slim /usr/local/bin/node /usr/local/bin/node
COPY --from=node:22-bookworm-slim /usr/local/lib/node_modules /usr/local/lib/node_modules
RUN ln -sf /usr/local/lib/node_modules/npm/bin/npm-cli.js /usr/local/bin/npm \
 && ln -sf /usr/local/lib/node_modules/npm/bin/npx-cli.js /usr/local/bin/npx \
 && node --version && npx --version

COPY --from=builder /usr/local/bin/server /usr/local/bin/server
COPY docker/kgp-local/entrypoint.sh /usr/local/bin/kgp-entrypoint

# These paths become named-volume mount points in docker-compose.yml. Creating
# and chowning them here matters: Docker seeds an empty named volume from the
# image directory and preserves its ownership, so appuser stays able to write.
RUN mkdir -p /repos \
             /home/appuser/.local/share/vibe-kanban \
             /home/appuser/.cache \
             /home/appuser/.claude \
             /home/appuser/.config \
  && sed -i 's/\r$//' /usr/local/bin/kgp-entrypoint \
  && chmod +x /usr/local/bin/kgp-entrypoint \
  && chown -R appuser:appuser /repos /home/appuser

USER appuser
WORKDIR /repos

ENV HOME=/home/appuser
ENV HOST=0.0.0.0
ENV PORT=3000
ENV PREVIEW_PROXY_PORT=3001
# Keeps npx package downloads on the persisted cache volume instead of in the
# container layer, so agents are not re-downloaded on every restart.
ENV npm_config_cache=/home/appuser/.cache/npm

EXPOSE 3000 3001

# /api/health, not /health: routes/mod.rs nests the health route under /api, so
# /health falls through to the SPA catch-all and would pass unconditionally.
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s --retries=3 \
  CMD ["/bin/sh", "-c", "wget --spider -q http://127.0.0.1:${PORT:-3000}/api/health"]

ENTRYPOINT ["/usr/bin/tini", "--", "/usr/local/bin/kgp-entrypoint"]
