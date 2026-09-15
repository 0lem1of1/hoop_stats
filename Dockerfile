# Build with the committed .sqlx cache so no database is needed at compile time.
FROM rust:1.90-slim-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
        pkg-config libssl-dev ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Cache the dependency build separately from the source.
COPY Cargo.toml Cargo.lock ./
RUN mkdir -p src/bin \
    && echo 'fn main() {}' > src/main.rs \
    && echo 'fn main() {}' > src/bin/mcp.rs \
    && cargo build --release \
    && rm -rf src

COPY .sqlx ./.sqlx
COPY migrations ./migrations
COPY src ./src

ENV SQLX_OFFLINE=true
# Cargo skips rebuilding if the stub's mtime looks newer than the real source.
RUN touch src/main.rs src/bin/mcp.rs && cargo build --release


FROM debian:bookworm-slim AS runtime

RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/* \
    && useradd -m -u 10001 app

WORKDIR /app
COPY --from=builder /build/target/release/hoop_stats /usr/local/bin/hoop_stats
COPY --from=builder /build/target/release/mcp /usr/local/bin/hoop-stats-mcp

USER app
EXPOSE 3000
CMD ["hoop_stats"]
