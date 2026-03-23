# Builder stage
FROM rust:1.88.0-slim-bookworm AS builder

WORKDIR /usr/src/app

# Install OpenSSL build-time dependencies required by openssl-sys / reqwest native-tls
RUN apt-get update && apt-get install -y pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*

COPY . .
RUN cargo build --release

# Runtime stage
FROM debian:bookworm-slim

# Install runtime OpenSSL and CA certificates (needed for HTTPS via reqwest)
RUN apt-get update && apt-get install -y libssl3 ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/backend-api /usr/local/bin/backend-api

WORKDIR /usr/local/bin

EXPOSE 8000

CMD ["./backend-api"]
