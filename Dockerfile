# signal-cli (Rust) — daemon sidecar / standalone container.
#
#   docker run -d --name signal-cli \
#     -e SIGNAL_ACCOUNT=+15551234567 \
#     -v signal-data:/data -p 8080:8080 \
#     ghcr.io/wnoronha/signal-cli:v0.1.3-dev
#
# Env:
#   SIGNAL_ACCOUNT  E.164 account for `-a` (single-account mode). Unset = multi-account.
#   SIGNAL_CONFIG   config dir (default /data).
# Any CMD/args are appended after the global flags, e.g. `receive` or `daemon --tcp ...`.

FROM rust:1-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        git g++ make libclang-dev clang libsqlite3-dev pkg-config protobuf-compiler cmake libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release

FROM debian:bookworm-slim

RUN useradd --system --uid 10000 --home-dir /data --create-home signal \
    && apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /build/target/release/signal-cli /usr/local/bin/signal-cli
COPY docker-entrypoint.sh /usr/local/bin/docker-entrypoint.sh

RUN mkdir -p /data && chown signal:signal /data
VOLUME /data
EXPOSE 8080
USER signal

ENTRYPOINT ["/usr/local/bin/docker-entrypoint.sh"]
CMD ["daemon", "--http", "0.0.0.0:8080"]
