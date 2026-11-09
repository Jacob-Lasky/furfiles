# --- frontend build ---
FROM node:24-slim AS frontend
WORKDIR /fe
# pnpm via corepack (bundled with node; no `npm install -g` bootstrap). Version
# is pinned by the package.json "packageManager" field. pnpm is preferred over
# npm here to shrink supply-chain exposure: a committed pnpm-lock.yaml with
# --frozen-lockfile pins the full tree, and pnpm does NOT run dependency build
# scripts unless explicitly approved.
RUN corepack enable
COPY frontend/package.json frontend/pnpm-lock.yaml ./
RUN pnpm install --frozen-lockfile
COPY frontend/ ./
RUN pnpm run build

# --- backend build ---
# Full (non-slim) image: libsqlite3-sys compiles bundled C, so a cc toolchain
# is required. trixie (Debian 13) matches the runtime base below so the built
# binary links against the same glibc it runs on.
FROM rust:1-trixie AS backend
WORKDIR /app
COPY backend/Cargo.toml backend/Cargo.lock ./
COPY backend/migrations ./migrations
COPY backend/src ./src
RUN cargo build --release --locked --bin furfiles

# --- runtime ---
FROM debian:trixie-slim
# ca-certificates is load-bearing: reqwest's rustls verifies HTTPS against the
# OS trust store (backend/Cargo.toml), so without it every Anthropic call and
# facility-website fetch fails. DO NOT remove it to slim the image.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
COPY --from=backend /app/target/release/furfiles /usr/local/bin/furfiles
COPY --from=frontend /fe/dist ./static
ENV FURFILES_STATIC=/app/static \
    FURFILES_DB=/data/furfiles.db \
    PORT=8080
EXPOSE 8080
VOLUME ["/data"]
CMD ["furfiles"]
