# Build Rust bindings
FROM rust:1.85-slim-bookworm as builder

# Install build dependencies for maturin and python extension compilation
# clang/libclang needed for bindgen if used, python3-dev for headers
RUN apt-get update && apt-get install -y \
    python3 \
    python3-pip \
    python3-venv \
    python3-dev \
    git \
    pkg-config \
    libssl-dev \
    patchelf \
    && rm -rf /var/lib/apt/lists/*

ENV PATH="/opt/venv/bin:$PATH"
RUN python3 -m venv /opt/venv

RUN pip3 install maturin

WORKDIR /crawl
COPY . .

# Build the wheel
# We use --release for optimized build
RUN maturin build --release --out dist
