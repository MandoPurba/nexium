FROM rust:slim AS builder
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
RUN cargo install sqlx-cli --no-default-features --features postgres

FROM debian:trixie-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/sqlx /usr/local/bin/sqlx
COPY postgres/ /migrations/postgres/
COPY timescale/ /migrations/timescale/
