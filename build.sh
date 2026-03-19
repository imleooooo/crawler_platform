#!/bin/bash
set -e

echo "Step 1: Building rustfs..."
docker build -t rustfs rustfs/

echo "Step 2: Building backend-api..."
docker build -t backend-api backend-api/

echo "Step 3: Building frontend..."
docker build -t frontend frontend/

echo "Build complete!"
echo "Step 4: Building Python wheels for crawl..."
cd crawl

# Build macOS wheel (local)
echo "Building macOS wheel (Python 3.11)..."
if ! command -v python3.11 &> /dev/null; then
    echo "Python 3.11 could not be found, skipping macOS wheel build."
else
    if [ ! -d ".venv311" ]; then
        python3.11 -m venv .venv311
    fi
    ./.venv311/bin/pip install -q maturin
    ./.venv311/bin/maturin build --release --interpreter python3.11
fi

# Build Manylinux wheel (Docker)
echo "Building Manylinux wheel (Python 3.11)..."
docker run --rm -v $(pwd):/io ghcr.io/pyo3/maturin build --release --interpreter python3.11

cd ..
echo "All builds complete!"
