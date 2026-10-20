# syntax=docker/dockerfile:1.7
# Copyright © 2026 Tess Dev Team <https://tessheadless.com>
# SPDX-License-Identifier: LicenseRef-Tess-Community-Enterprise
# Community edition: personal use. Enterprise edition: commercial use. See LICENSE.md.

# Multi-stage build: admin SPA (Node) + CMS server (Rust) -> slim runtime.
#
#   docker build -t tess .
#   docker build --build-arg FEATURES=sql,s3 -t tess:sql .

FROM node:24-trixie-slim AS admin
WORKDIR /admin
COPY admin/package.json admin/package-lock.json ./
RUN npm ci --no-audit --no-fund
COPY admin/ ./
RUN npm run build

FROM rust:1-trixie AS server
# clang/cmake: RocksDB is compiled from source.
RUN apt-get update && apt-get install -y --no-install-recommends clang cmake && rm -rf /var/lib/apt/lists/*
ARG FEATURES=sql,mongodb,dynamodb,rocksdb,s3
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY assets/Tess-logo.svg assets/
COPY crates/ crates/
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/src/target \
    cargo build --release --locked -p tess-server --no-default-features --features "$FEATURES" \
    && cargo build --release --locked -p tess-transfer \
    && cp target/release/tess /tess && cp target/release/tess-transfer /tess-transfer

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --home /app tess \
    && mkdir -p /data && chown tess /data
COPY --from=server /tess /usr/local/bin/tess
COPY --from=server /tess-transfer /usr/local/bin/tess-transfer
LABEL org.opencontainers.image.title="Tess" \
      org.opencontainers.image.description="Tess, the GraphQL-first headless CMS in Rust" \
      org.opencontainers.image.vendor="Tess Dev Team" \
      org.opencontainers.image.licenses="LicenseRef-Tess-Community-Enterprise"
COPY --from=admin /admin/dist/client /app/admin
WORKDIR /app
USER tess
ENV PORT=8080 \
    TESS_ADMIN_DIR=/app/admin \
    TESS_DATABASE=rocksdb \
    ROCKSDB_PATH=/data/rocksdb \
    TESS_MEDIA_STORE=fs \
    TESS_MEDIA_DIR=/data/media \
    TESS_LOG_FORMAT=json
VOLUME ["/data"]
EXPOSE 8080
ENTRYPOINT ["/usr/local/bin/tess"]
