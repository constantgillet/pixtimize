# syntax=docker/dockerfile:1

# ---- Build stage ----
FROM rust:1-bookworm AS builder

WORKDIR /app

# Build dependencies:
# - cmake + C toolchain + perl: required by aws-lc-rs (TLS)
# - libvips-dev + pkg-config: image processing (headers, static/shared libs, .pc)
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        cmake build-essential perl pkg-config libvips-dev \
    && rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock build.rs ./
COPY src ./src

RUN cargo build --release

# ---- Runtime stage ----
FROM debian:bookworm-slim

# ca-certificates for outbound HTTPS; libvips42 (+ libglib2.0-0) are the shared
# libraries the binary links against at runtime.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates libvips42 libglib2.0-0 \
    && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/pixtimize /usr/local/bin/pixtimize

ENV PORT=3000
EXPOSE 3000

CMD ["pixtimize"]
