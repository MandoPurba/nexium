FROM rust:slim AS builder

RUN apt-get update && apt-get install -y pkg-config libssl-dev curl && rm -rf /var/lib/apt/lists/*

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY services/ services/
COPY config/ config/

RUN cargo build --release \
    -p auth-service \
    -p wallet-service \
    -p order-service \
    -p market-data-service \
    -p gateway

FROM debian:trixie-slim AS runtime

RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /app/target/release/auth-service /usr/local/bin/
COPY --from=builder /app/target/release/wallet-service /usr/local/bin/
COPY --from=builder /app/target/release/order-service /usr/local/bin/
COPY --from=builder /app/target/release/market-data-service /usr/local/bin/
COPY --from=builder /app/target/release/gateway /usr/local/bin/
COPY config/ /app/config/

WORKDIR /app
ENV NEXIUM_ENV=production

EXPOSE 8080
