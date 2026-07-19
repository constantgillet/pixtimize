# syntax=docker/dockerfile:1

# ---- Build stage ----
FROM rust:1-bookworm AS builder

WORKDIR /app

# Build dependencies: cmake + C toolchain are required by aws-lc-rs (TLS) and
# the libwebp bindings compiled by the `webp` crate.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake build-essential perl pkg-config \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY src ./src

RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pixtimize /usr/local/bin/pixtimize

ENV PORT=3000
EXPOSE 3000

CMD ["pixtimize"]
