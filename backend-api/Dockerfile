# Builder stage
FROM rust:latest AS builder

WORKDIR /usr/src/app
COPY . .

# Build dependencies first to cache them? 
# For simplicity in this migration, just copy all and build.
RUN cargo build --release

# Runtime stage
FROM debian:testing-slim

# Install OpenSSL/Ca-certificates as they are often needed for HTTPS (reqwest)
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*

COPY --from=builder /usr/src/app/target/release/backend-api /usr/local/bin/backend-api

WORKDIR /usr/local/bin

EXPOSE 8000

CMD ["./backend-api"]
