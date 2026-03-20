use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use std::path::Path;
use std::time::Duration;

// Tight timeout for small JSON/markdown content uploads.
const S3_CONTENT_TIMEOUT: Duration = Duration::from_secs(15);
// Generous timeout for large binary files (PDFs, audio).
const S3_FILE_TIMEOUT: Duration = Duration::from_secs(300);
// Bucket creation is a lightweight control-plane call.
const S3_BUCKET_TIMEOUT: Duration = Duration::from_secs(5);

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
    create_bucket_if_not_exists(client, bucket_name).await;

    let body = ByteStream::from(content.as_bytes().to_vec());
    tokio::time::timeout(
        S3_CONTENT_TIMEOUT,
        client.put_object().bucket(bucket_name).key(key).body(body).send(),
    )
    .await
    .map_err(|_| format!("S3 upload timed out after {}s", S3_CONTENT_TIMEOUT.as_secs()))?
    .map_err(|e| format!("Failed to upload content: {}", e))?;

    Ok(format!("s3://{}/{}", bucket_name, key))
}

pub async fn save_to_rustfs_file(
    client: &Client,
    bucket_name: &str,
    key: &str,
    filepath: &Path,
) -> Result<String, String> {
    create_bucket_if_not_exists(client, bucket_name).await;

    let body = ByteStream::from_path(filepath)
        .await
        .map_err(|e| format!("File error: {}", e))?;

    tokio::time::timeout(
        S3_FILE_TIMEOUT,
        client.put_object().bucket(bucket_name).key(key).body(body).send(),
    )
    .await
    .map_err(|_| format!("S3 file upload timed out after {}s", S3_FILE_TIMEOUT.as_secs()))?
    .map_err(|e| format!("Failed to upload file: {}", e))?;

    Ok(format!("s3://{}/{}", bucket_name, key))
}

async fn create_bucket_if_not_exists(client: &Client, bucket_name: &str) {
    match tokio::time::timeout(
        S3_BUCKET_TIMEOUT,
        client.create_bucket().bucket(bucket_name).send(),
    )
    .await
    {
        Err(_) => tracing::warn!(
            "Timed out creating bucket {} after {}s",
            bucket_name,
            S3_BUCKET_TIMEOUT.as_secs()
        ),
        Ok(Err(e)) => {
            let err_str = e.to_string();
            if !err_str.contains("BucketAlreadyExists")
                && !err_str.contains("BucketAlreadyOwnedByYou")
            {
                tracing::warn!("Unexpected error creating bucket {}: {}", bucket_name, e);
            }
        }
        Ok(Ok(_)) => {}
    }
}

