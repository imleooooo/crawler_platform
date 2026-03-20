# ── Stage 1: Build ──────────────────────────────────────────────────────────
# Use the same Python minor version as the runtime stage so maturin produces
# a cp312 wheel that is installable into Python 3.12.
FROM python:3.12-slim-bookworm AS builder

RUN apt-get update && apt-get install -y \
    curl \
    git \
    pkg-config \
    libssl-dev \
    patchelf \
    && rm -rf /var/lib/apt/lists/*

# Install Rust toolchain
RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain 1.85.0
ENV PATH="/root/.cargo/bin:$PATH"

RUN pip install --no-cache-dir maturin

WORKDIR /crawl
COPY . .

RUN maturin build --release --out dist

# ── Stage 2: Runtime ─────────────────────────────────────────────────────────
FROM python:3.12-slim-bookworm

# Chromium is required by chromiumoxide for browser-based crawling
RUN apt-get update && apt-get install -y \
    chromium \
    && rm -rf /var/lib/apt/lists/*

# Run as non-root
RUN useradd -ms /bin/bash crawler
USER crawler
WORKDIR /home/crawler

# Copy and install the wheel built in Stage 1
COPY --from=builder /crawl/dist/*.whl ./
RUN pip install --no-cache-dir *.whl && rm *.whl

# Point chromiumoxide at the system chromium binary
ENV CHROME_PATH=/usr/bin/chromium

# lab_crawl is now importable as a Python extension module
# Mount a Python server script at /home/crawler/server.py and override CMD
# to serve HTTP, or import lab_crawl directly from another container.
CMD ["python3", "-c", "import lab_crawl; print('lab_crawl ready')"]
