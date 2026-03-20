use aws_sdk_s3::primitives::ByteStream;
use aws_sdk_s3::Client;
use std::path::Path;

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
    create_bucket_if_not_exists(client, bucket_name).await?;

    let body = ByteStream::from(content.as_bytes().to_vec());
    client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed to upload content: {}", e))?;

    Ok(format!("s3://{}/{}", bucket_name, key))
}

pub async fn save_to_rustfs_file(
    client: &Client,
    bucket_name: &str,
    key: &str,
    filepath: &Path,
) -> Result<String, String> {
    create_bucket_if_not_exists(client, bucket_name).await?;

    let body = ByteStream::from_path(filepath)
        .await
        .map_err(|e| format!("File error: {}", e))?;

    client
        .put_object()
        .bucket(bucket_name)
        .key(key)
        .body(body)
        .send()
        .await
        .map_err(|e| format!("Failed to upload file: {}", e))?;

    Ok(format!("s3://{}/{}", bucket_name, key))
}

async fn create_bucket_if_not_exists(client: &Client, bucket_name: &str) -> Result<(), String> {
    if let Err(e) = client.create_bucket().bucket(bucket_name).send().await {
        let err_str = e.to_string();
        if !err_str.contains("BucketAlreadyExists") && !err_str.contains("BucketAlreadyOwnedByYou") {
            return Err(format!("Failed to create bucket {}: {}", bucket_name, e));
        }
    }
    Ok(())
}

