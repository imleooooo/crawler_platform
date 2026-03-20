use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use std::path::Path;
use std::time::Duration;

// Generous timeout for large binary files (PDFs, audio).
const S3_FILE_TIMEOUT: Duration = Duration::from_secs(300);

/// Sanitize a string to be a valid S3 bucket name.
/// S3 bucket names must:
/// - Be 3-63 characters long
/// - Only contain lowercase letters, numbers, and hyphens
/// - Not start or end with a hyphen
pub fn sanitize_bucket_name(name: &str) -> String {
    // Convert to lowercase and replace invalid chars with hyphens
    let mut sanitized: String = name
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect();

    // Collapse multiple consecutive hyphens into one
    while sanitized.contains("--") {
        sanitized = sanitized.replace("--", "-");
    }

    // Trim leading and trailing hyphens
    sanitized = sanitized.trim_matches('-').to_string();

    // Ensure minimum length of 3
    if sanitized.len() < 3 {
        sanitized = format!("{}-job", sanitized);
    }

    // Truncate to max 63 chars and remove trailing hyphen if present
    if sanitized.len() > 63 {
        sanitized = sanitized[..63].trim_end_matches('-').to_string();
    }

    sanitized
}

pub async fn save_to_rustfs_content(
    client: &Client,
    bucket_name: &str,
    key: &str,
    content: &str,
) -> Result<String, String> {
    // Size-proportional timeout: 30s baseline + 1s per 50 KB, capped at the
    // large-file ceiling. Prevents hard failures on slow-but-healthy backends
    // for larger markdown/JSON payloads while still bounding hung uploads.
    let timeout = (Duration::from_secs(30)
        + Duration::from_secs((content.len() / (50 * 1024)) as u64))
    .min(S3_FILE_TIMEOUT);

    // Optimistic path: attempt put_object before create_bucket so that
    // pre-existing buckets (including pre-provisioned PutObject-only buckets)
    // never pay the control-plane round-trip.
    let first = put_content(client, bucket_name, key, content, timeout).await;

    match first {
        Ok(()) => {}
        Err(ref e) if e.contains("NoSuchBucket") => {
            // Bucket is confirmed absent — create it, then retry.
            // No timeout on create_bucket here: we know the bucket does not
            // exist, so we must wait for creation to complete; cancelling the
            // future mid-flight with tokio::time::timeout would leave put_object
            // racing a partially-created bucket. Truly unresponsive backends are
            // already bounded by the put_object timeout on the retry below.
            ensure_bucket(client, bucket_name).await;
            put_content(client, bucket_name, key, content, timeout).await?;
        }
        Err(e) => return Err(e),
    }

    Ok(format!("s3://{}/{}", bucket_name, key))
}

pub async fn save_to_rustfs_file(
    client: &Client,
    bucket_name: &str,
    key: &str,
    filepath: &Path,
) -> Result<String, String> {
    // Optimistic path — see save_to_rustfs_content for the full rationale.
    let body = ByteStream::from_path(filepath)
        .await
        .map_err(|e| format!("File error: {}", e))?;

    let first = put_body(client, bucket_name, key, body, S3_FILE_TIMEOUT).await;

    match first {
        Ok(()) => {}
        Err(ref e) if e.contains("NoSuchBucket") => {
            ensure_bucket(client, bucket_name).await;
            let body = ByteStream::from_path(filepath)
                .await
                .map_err(|e| format!("File error on retry: {}", e))?;
            put_body(client, bucket_name, key, body, S3_FILE_TIMEOUT).await?;
        }
        Err(e) => return Err(e),
    }

    Ok(format!("s3://{}/{}", bucket_name, key))
}

async fn put_content(
    client: &Client,
    bucket_name: &str,
    key: &str,
    content: &str,
    timeout: Duration,
) -> Result<(), String> {
    let body = ByteStream::from(content.as_bytes().to_vec());
    put_body(client, bucket_name, key, body, timeout).await
}

async fn put_body(
    client: &Client,
    bucket_name: &str,
    key: &str,
    body: ByteStream,
    timeout: Duration,
) -> Result<(), String> {
    tokio::time::timeout(
        timeout,
        client.put_object().bucket(bucket_name).key(key).body(body).send(),
    )
    .await
    .map_err(|_| format!("S3 upload timed out after {}s", timeout.as_secs()))?
    .map_err(|e| format!("S3 upload failed: {}", e))?;
    Ok(())
}

/// Create the bucket, waiting for completion. Called only after put_object has
/// confirmed the bucket is absent, so we must not cancel mid-flight.
/// Unexpected errors (other than AlreadyExists) are logged; the subsequent
/// put_object will surface a definitive failure if the bucket still does not exist.
async fn ensure_bucket(client: &Client, bucket_name: &str) {
    if let Err(e) = client.create_bucket().bucket(bucket_name).send().await {
        let err_str = e.to_string();
        if !err_str.contains("BucketAlreadyExists") && !err_str.contains("BucketAlreadyOwnedByYou")
        {
            tracing::warn!("create_bucket failed for {}: {}", bucket_name, e);
        }
    }
}
