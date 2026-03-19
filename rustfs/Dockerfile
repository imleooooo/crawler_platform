FROM debian:bookworm-slim

# Install dependencies
RUN apt-get update && apt-get install -y curl unzip && rm -rf /var/lib/apt/lists/*

# Create user
RUN useradd -ms /bin/bash rustfs

WORKDIR /home/rustfs

# Download and install RustFS binary based on architecture
RUN ARCH=$(uname -m) && \
    if [ "$ARCH" = "x86_64" ]; then \
    URL="https://dl.rustfs.com/artifacts/rustfs/release/rustfs-linux-x86_64-gnu-latest.zip"; \
    elif [ "$ARCH" = "aarch64" ]; then \
    URL="https://dl.rustfs.com/artifacts/rustfs/release/rustfs-linux-aarch64-gnu-latest.zip"; \
    else \
    echo "Unsupported architecture: $ARCH"; exit 1; \
    fi && \
    curl -L -o rustfs.zip "$URL" && \
    unzip rustfs.zip && \
    chmod +x rustfs && \
    mv rustfs /usr/local/bin/rustfs && \
    rm rustfs.zip

# Create directories
RUN mkdir -p /var/log/rustfs /data/rustfs0 && \
    chown -R rustfs:rustfs /var/log/rustfs /data/rustfs0

USER rustfs

# Environment variables
ENV RUSTFS_PORT=9000 \
    RUSTFS_CONSOLE_PORT=9001 \
    RUSTFS_VOLUMES="/data/rustfs0" \
    RUSTFS_ACCESS_KEY="rustfsadmin" \
    RUSTFS_SECRET_KEY="rustfsadmin" \
    RUSTFS_ADDRESS=":9000" \
    RUSTFS_CONSOLE_ADDRESS=":9001" \
    RUSTFS_CONSOLE_ENABLE="true" \
    RUSTFS_OBS_LOGGER_LEVEL="error" \
    RUSTFS_OBS_LOG_DIRECTORY="/var/log/rustfs/"

# Expose ports
EXPOSE 9000 9001

# Run command
CMD ["sh", "-c", "rustfs $RUSTFS_VOLUMES"]
