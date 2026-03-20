use axum::{extract::State, Json};
use backon::{ExponentialBuilder, Retryable};
use chrono::Utc;
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::services::{s3, sanitize_bucket_name};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct PodcastRequest {
    pub keywords: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub year: Option<String>,
    pub output_dir: Option<String>,
    pub job_id: Option<String>,
}

fn default_limit() -> usize {
    5
}

#[derive(Serialize)]
struct PodcastResult {
    title: String,
    podcast: String,
    published: String,
    audio_url: String,
    local_path: Option<String>,
    s3_path: Option<String>,
    s3_bucket: String,
    error: Option<String>,
}

pub async fn podcast_search(
    State(state): State<AppState>,
    Json(request): Json<PodcastRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Metrics
    {
        if let Ok(mut metrics) = state.metrics.lock() {
            metrics.queue_size += 1;
            metrics.active_workers += 1;
        }
    }

    tracing::info!(
        "Podcast Search Request: keywords='{}', limit={}",
        request.keywords,
        request.limit
    );

    // metadata_client: bounded timeout for iTunes and feed lookups
    let client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("HTTP client init failed: {}", e),
            )
        })?;

    // download_client: connect_timeout only — no total transfer deadline.
    // reqwest's timeout() is an end-to-end cap, not an idle timeout, so it
    // would abort healthy large episodes that simply take a long time.
    // Stalled origins (connection accepted, then silent) are detected per-chunk
    // in the stream loop below via tokio::time::timeout on each stream.next().
    let download_client = reqwest::Client::builder()
        .user_agent("Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .connect_timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("HTTP download client init failed: {}", e),
            )
        })?;

    // 1. iTunes Search
    let itunes_url = "https://itunes.apple.com/search";
    tracing::info!("Querying iTunes: {}", itunes_url);

    let resp = (|| async {
        client
            .get(itunes_url)
            .query(&[
                ("media", "podcast"),
                ("term", request.keywords.as_str()),
                ("limit", "1"),
            ])
            .send()
            .await
            .map_err(|e| format!("iTunes API failed: {}", e))
            .and_then(|r| {
                if r.status().is_server_error() {
                    Err(format!("iTunes server error: {}", r.status()))
                } else {
                    Ok(r)
                }
            })
    })
    .retry(
        ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(300))
            .with_max_delay(Duration::from_secs(5))
            .with_max_times(3),
    )
    .await
    .map_err(|e| {
        tracing::error!("{}", e);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e)
    })?;

    let status = resp.status();
    tracing::info!("iTunes Response Status: {}", status);

    let body_text = resp.text().await.map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Body read error: {}", e),
        )
    })?;
    tracing::info!("iTunes Response Body: {}", body_text);

    let itunes_data: Value = serde_json::from_str(&body_text).map_err(|e| {
        let err_msg = format!("iTunes JSON parsing: {}", e);
        tracing::error!("{}", err_msg);
        (axum::http::StatusCode::INTERNAL_SERVER_ERROR, err_msg)
    })?;

    let results_array = itunes_data["results"].as_array();
    let results_array = match results_array {
        Some(arr) if !arr.is_empty() => arr,
        _ => {
            tracing::warn!("iTunes returned 0 results");
            // Metrics cleanup
            {
                if let Ok(mut metrics) = state.metrics.lock() {
                    if metrics.queue_size > 0 {
                        metrics.queue_size -= 1;
                    }
                    if metrics.active_workers > 0 {
                        metrics.active_workers -= 1;
                    }
                }
            }
            return Ok(Json(json!({"data": [], "message": "No podcasts found"})));
        }
    };

    let first_result = &results_array[0];
    let feed_url = first_result["feedUrl"].as_str().ok_or((
        axum::http::StatusCode::INTERNAL_SERVER_ERROR,
        "No feedUrl".to_string(),
    ))?;
    let collection_name = first_result["collectionName"]
        .as_str()
        .unwrap_or("Unknown Podcast");

    // 2. Parse Feed
    let feed_resp = (|| async {
        client
            .get(feed_url)
            .send()
            .await
            .map_err(|e| format!("Feed fetch failed: {}", e))
            .and_then(|r| {
                if r.status().is_server_error() {
                    Err(format!("Feed server error: {}", r.status()))
                } else {
                    Ok(r)
                }
            })
    })
    .retry(
        ExponentialBuilder::default()
            .with_min_delay(Duration::from_millis(300))
            .with_max_delay(Duration::from_secs(5))
            .with_max_times(3),
    )
    .await
    .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e))?;

    let feed_content = feed_resp.bytes().await.map_err(|_| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            "Feed bytes error".to_string(),
        )
    })?;

    let feed = feed_rs::parser::parse(feed_content.as_ref()).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Feed parse failed: {}", e),
        )
    })?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "podcast-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    // Temp dir setup
    let temp_dir_obj = if request.output_dir.is_none() {
        Some(tempfile::tempdir().map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Temp dir fail: {}", e),
            )
        })?)
    } else {
        None
    };

    let base_output_path = if let Some(ref d) = request.output_dir {
        d.clone()
    } else {
        temp_dir_obj
            .as_ref()
            .ok_or_else(|| {
                (
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    "Failed to obtain temp directory path".to_string(),
                )
            })?
            .path()
            .to_string_lossy()
            .to_string()
    };

    // Sanitize collection name for folder
    let safe_collection_name: String = collection_name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ')
        .collect();
    let podcast_dir = Path::new(&base_output_path).join(safe_collection_name.trim());
    if let Err(e) = tokio::fs::create_dir_all(&podcast_dir).await {
        tracing::warn!("Failed to create podcast directory {:?}: {}", podcast_dir, e);
    }

    let mut results = Vec::new();
    let mut downloaded_count = 0;

    for entry in feed.entries {
        if downloaded_count >= request.limit {
            break;
        }

        let published_str = entry.published.map(|t| t.to_rfc3339()).unwrap_or_default();

        if let Some(req_year) = &request.year {
            if !published_str.starts_with(req_year) {
                continue;
            }
        }

        // Find Audio URL
        let mut audio_url_opt = None;

        // 1. Check Links (Atom/RSS with explicit type)
        if let Some(link) = entry.links.iter().find(|l| {
            l.media_type
                .as_deref()
                .map(|m| m.starts_with("audio"))
                .unwrap_or(false)
        }) {
            audio_url_opt = Some(link.href.clone());
        }

        // 2. Check Media/Enclosures (Standard RSS)
        if audio_url_opt.is_none() {
            for media in &entry.media {
                for content in &media.content {
                    if let Some(mime) = &content.content_type {
                        if mime.to_string().starts_with("audio") {
                            if let Some(url) = &content.url {
                                audio_url_opt = Some(url.to_string());
                                break;
                            }
                        }
                    }
                }
                if audio_url_opt.is_some() {
                    break;
                }
            }
        }

        if let Some(audio_url) = audio_url_opt {
            let title = entry
                .title
                .map(|t| t.content)
                .unwrap_or("Untitled".to_string());
            let safe_title: String = title
                .chars()
                .filter(|c| c.is_alphanumeric() || " -_".contains(*c))
                .collect();
            let filename = format!(
                "{}.mp3",
                safe_title.trim().get(0..50).unwrap_or(&safe_title)
            ); // Trim len
            let filepath = podcast_dir.join(&filename);

            let mut result_entry = PodcastResult {
                title: title.clone(),
                podcast: collection_name.to_string(),
                published: published_str,
                audio_url: audio_url.clone(),
                local_path: None,
                s3_path: None,
                s3_bucket: bucket_name.clone(),
                error: None,
            };

            // Download Audio Stream
            // connect_timeout covers TCP setup; this 30s timeout covers the
            // header read — origins that accept the TCP connection but never
            // return response headers would otherwise hang indefinitely.
            const HEADER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
            let send_result =
                tokio::time::timeout(HEADER_TIMEOUT, download_client.get(&audio_url).send()).await;
            let send_result = match send_result {
                Err(_) => {
                    result_entry.error = Some("Audio download timed out waiting for headers (30s)".to_string());
                    results.push(result_entry);
                    downloaded_count += 1;
                    continue;
                }
                Ok(r) => r,
            };
            match send_result {
                Ok(res) => {
                    if res.status().is_success() {
                        // Stream to file
                        let file = match tokio::fs::File::create(&filepath).await {
                            Ok(f) => Some(f),
                            Err(e) => {
                                tracing::warn!("Failed to create file {:?}: {}", filepath, e);
                                None
                            }
                        };
                        if let Some(mut f) = file {
                            let mut stream = res.bytes_stream();
                            let mut success = true;
                            // Per-chunk idle timeout: fires only when the origin
                            // stops sending data, not during normal slow transfers.
                            // This releases the concurrency slot from stalled
                            // origins without cutting off healthy large episodes.
                            const CHUNK_IDLE_TIMEOUT: std::time::Duration =
                                std::time::Duration::from_secs(30);
                            loop {
                                match tokio::time::timeout(CHUNK_IDLE_TIMEOUT, stream.next()).await
                                {
                                    Err(_) => {
                                        // No chunk arrived within 30s — origin stalled
                                        result_entry.error =
                                            Some("Audio download stalled (30s idle)".to_string());
                                        success = false;
                                        break;
                                    }
                                    Ok(None) => break, // stream finished normally
                                    Ok(Some(Ok(chunk))) => {
                                        if tokio::io::AsyncWriteExt::write_all(&mut f, &chunk)
                                            .await
                                            .is_err()
                                        {
                                            success = false;
                                            break;
                                        }
                                    }
                                    Ok(Some(Err(_))) => {
                                        success = false;
                                        break;
                                    }
                                }
                            }

                            if success {
                                // Upload to S3
                                if let Ok(s3_path) = s3::save_to_rustfs_file(
                                    &state.s3_client,
                                    &bucket_name,
                                    &filename,
                                    &filepath,
                                )
                                .await
                                {
                                    result_entry.s3_path = Some(s3_path);
                                }
                                if request.output_dir.is_some() {
                                    result_entry.local_path =
                                        Some(filepath.to_string_lossy().to_string());
                                }
                            } else if result_entry.error.is_none() {
                                // Only set a generic error if a more specific one
                                // (e.g. idle timeout) hasn't already been recorded.
                                result_entry.error = Some("Stream write failed".to_string());
                            }
                        } else {
                            result_entry.error = Some("File creation failed".to_string());
                        }
                    } else {
                        result_entry.error = Some(format!("Status {}", res.status()));
                    }
                }
                Err(e) => {
                    result_entry.error = Some(e.to_string());
                }
            }

            results.push(result_entry);
            downloaded_count += 1;
        }
    }

    // Metrics cleanup
    {
        if let Ok(mut metrics) = state.metrics.lock() {
            if metrics.queue_size > 0 {
                metrics.queue_size -= 1;
            }
            if metrics.active_workers > 0 {
                metrics.active_workers -= 1;
            }
        }
    }

    // Save search_results.json to RustFS (source: iTunes Podcast API)
    let title = format!("Podcast: {}", request.keywords);
    let created_at = Utc::now().to_rfc3339();

    let result_json_content = serde_json::to_string_pretty(&json!({
        "job_type": "podcast",
        "source": "iTunes Podcast API",
        "title": title,
        "created_at": created_at,
        "keywords": request.keywords,
        "results": results
    }))
    .unwrap_or_default();

    if let Err(e) = s3::save_to_rustfs_content(
        &state.s3_client,
        &bucket_name,
        "search_results.json",
        &result_json_content,
    )
    .await
    {
        tracing::warn!("Failed to save podcast results to S3 bucket {}: {}", bucket_name, e);
    }

    Ok(Json(json!({"data": results})))
}
