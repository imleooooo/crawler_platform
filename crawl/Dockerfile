# ── Stage 1: Build ──────────────────────────────────────────────────────────
FROM rust:1.85-slim-bookworm AS builder

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
