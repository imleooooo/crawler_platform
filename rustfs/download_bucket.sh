#!/bin/bash
#./download_bucket.sh job-1770103569067-805 .

# Check if bucket name is provided
if [ -z "$1" ]; then
    echo "Usage: $0 <bucket_name> [local_dir]"
    exit 1
fi

BUCKET_NAME=$1
PARENT_DIR=${2:-"."}
LOCAL_DIR="$PARENT_DIR/$BUCKET_NAME"

# RustFS Credentials and Config
export AWS_ACCESS_KEY_ID=rustfsadmin
export AWS_SECRET_ACCESS_KEY=rustfsadmin
export AWS_REGION=us-east-1

echo "Downloading bucket '$BUCKET_NAME' from RustFS..."
echo "Destination: $LOCAL_DIR"

# Create local directory if it doesn't exist
mkdir -p "$LOCAL_DIR"

# Run AWS S3 Sync
aws s3 sync "s3://$BUCKET_NAME" "$LOCAL_DIR" --endpoint-url http://localhost:9000

if [ $? -eq 0 ]; then
    echo "Download complete."
else
    echo "Download failed."
    exit 1
fi
