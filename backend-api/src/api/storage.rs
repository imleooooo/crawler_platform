use crate::state::AppState;
use axum::{extract::State, Json};
use serde_json::{json, Value};

pub async fn storage_stats(
    State(state): State<AppState>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let s3 = &state.s3_client;

    // List all buckets
    let buckets_resp = s3.list_buckets().send().await.map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to list buckets: {}", e),
        )
    })?;

    let buckets = buckets_resp.buckets.unwrap_or_default();

    let mut total_files = 0;
    let mut total_size = 0;
    let mut recent_files: Vec<Value> = Vec::new();

    for bucket in buckets {
        let name = bucket.name.unwrap_or_default();

        let objects_resp = s3.list_objects_v2().bucket(&name).send().await;

        if let Ok(output) = objects_resp {
            if let Some(contents) = output.contents {
                for obj in contents {
                    total_files += 1;
                    let size = obj.size.unwrap_or(0);
                    total_size += size;

                    let key = obj.key.clone().unwrap_or_default();
                    let extension = std::path::Path::new(&key)
                        .extension()
                        .and_then(std::ffi::OsStr::to_str)
                        .unwrap_or("FILE")
                        .to_uppercase();

                    // Generate presigned URL using the public client (for frontend access)
                    // PresigningConfig::expires_in only fails for zero/negative durations, so this is safe.
                    let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(
                        std::time::Duration::from_secs(3600),
                    )
                    .expect("presigning config with 3600s duration is always valid");

                    let presigned_req = state
                        .s3_client_public
                        .get_object()
                        .bucket(&name)
                        .key(&key)
                        .presigned(presigning_config)
                        .await;

                    let url = match presigned_req {
                        Ok(req) => req.uri().to_string(),
                        Err(e) => {
                            tracing::error!("Failed to presign URL for {}/{}: {}", name, key, e);
                            continue; // Skip this file rather than returning a broken localhost URL
                        }
                    };

                    recent_files.push(json!({
                        "filename": key.clone(), // specific to backend, keep for ref
                        "name": key.clone(),     // Frontend expects 'name'
                        "bucket": name.clone(),
                        "size": size,
                        "type": extension,       // Frontend expects 'type'
                        "url": url,              // Frontend expects 'url'
                        "last_modified": obj.last_modified.map(|d| d.to_string()).unwrap_or_default()
                    }));
                }
            }
        }
    }

    // Sort recent_files by last_modified (descending)
    recent_files.sort_by(|a, b| {
        let date_a = a["last_modified"].as_str().unwrap_or("");
        let date_b = b["last_modified"].as_str().unwrap_or("");
        date_b.cmp(date_a)
    });
    recent_files.truncate(50);

    let total_size_display = format_size(total_size);

    Ok(Json(json!({
        "total_files": total_files,
        "total_size_bytes": total_size,
        "total_size_display": total_size_display, // Frontend expects this
        "recent_files": recent_files
    })))
}

fn format_size(size: i64) -> String {
    const KB: i64 = 1024;
    const MB: i64 = KB * 1024;
    const GB: i64 = MB * 1024;

    if size >= GB {
        format!("{:.2} GB", size as f64 / GB as f64)
    } else if size >= MB {
        format!("{:.2} MB", size as f64 / MB as f64)
    } else if size >= KB {
        format!("{:.2} KB", size as f64 / KB as f64)
    } else {
        format!("{} B", size)
    }
}

use serde::Deserialize;

#[derive(Deserialize)]
pub struct DeleteRequest {
    pub bucket_names: Vec<String>,
}

pub async fn delete_task_data(
    State(state): State<AppState>,
    Json(request): Json<DeleteRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let s3 = &state.s3_client;
    let mut deleted_buckets = Vec::new();

    for bucket_name in request.bucket_names {
        // 1. List all objects in the bucket
        let objects_resp = s3.list_objects_v2().bucket(&bucket_name).send().await;

        if let Ok(output) = objects_resp {
            if let Some(contents) = output.contents {
                for obj in contents {
                    if let Some(key) = obj.key {
                        // 2. Delete object
                        let _ = s3
                            .delete_object()
                            .bucket(&bucket_name)
                            .key(&key)
                            .send()
                            .await
                            .map_err(|e| {
                                tracing::error!(
                                    "Failed to delete object {} in {}: {}",
                                    key,
                                    bucket_name,
                                    e
                                );
                            });
                    }
                }
            }
        }

        // 3. Delete the bucket itself
        let delete_bucket_resp = s3.delete_bucket().bucket(&bucket_name).send().await;

        match delete_bucket_resp {
            Ok(_) => {
                deleted_buckets.push(bucket_name);
            }
            Err(e) => {
                tracing::error!("Failed to delete bucket {}: {}", bucket_name, e);
                // We continue to try subsequent buckets even if one fails
            }
        }
    }

    Ok(Json(json!({
        "success": true,
        "deleted_buckets": deleted_buckets
    })))
}
