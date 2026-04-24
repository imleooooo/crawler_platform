use axum::{extract::State, Json};
use backon::{ExponentialBuilder, Retryable};
use chrono::{NaiveDate, Utc};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::services::{s3, sanitize_bucket_name};
use crate::state::{lock_metrics, AppState};

#[derive(Deserialize)]
pub struct PodcastRequest {
    pub keywords: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub year: Option<String>,
    pub output_dir: Option<String>,
    pub job_id: Option<String>,
}

fn default_limit() -> usize {
    5
}

#[derive(Clone, Copy)]
struct PodcastDateRange {
    start: Option<NaiveDate>,
    end: Option<NaiveDate>,
}

fn parse_optional_date(
    value: Option<&String>,
    field_name: &str,
) -> Result<Option<NaiveDate>, String> {
    let Some(raw) = value else {
        return Ok(None);
    };

    let raw = raw.trim();
    if raw.is_empty() {
        return Ok(None);
    }

    NaiveDate::parse_from_str(raw, "%Y-%m-%d")
        .map(Some)
        .map_err(|_| format!("{} must use YYYY-MM-DD format", field_name))
}

fn podcast_date_range(request: &PodcastRequest) -> Result<PodcastDateRange, String> {
    let start = parse_optional_date(request.start_date.as_ref(), "start_date")?;
    let end = parse_optional_date(request.end_date.as_ref(), "end_date")?;

    if let (Some(start), Some(end)) = (start, end) {
        if start > end {
            return Err("start_date must be earlier than or equal to end_date".to_string());
        }
    }

    if start.is_some() || end.is_some() {
        return Ok(PodcastDateRange { start, end });
    }

    let Some(year) = request
        .year
        .as_ref()
        .map(|y| y.trim())
        .filter(|y| !y.is_empty())
    else {
        return Ok(PodcastDateRange {
            start: None,
            end: None,
        });
    };

    let parsed_year = year
        .parse::<i32>()
        .map_err(|_| "year must use YYYY format".to_string())?;
    let start = NaiveDate::from_ymd_opt(parsed_year, 1, 1)
        .ok_or_else(|| "year is out of range".to_string())?;
    let end = NaiveDate::from_ymd_opt(parsed_year, 12, 31)
        .ok_or_else(|| "year is out of range".to_string())?;

    Ok(PodcastDateRange {
        start: Some(start),
        end: Some(end),
    })
}

fn podcast_entry_matches_date_range(
    published: Option<chrono::DateTime<Utc>>,
    range: PodcastDateRange,
) -> (bool, String) {
    let published_str = published.map(|t| {
        let published_date = t.date_naive();
        let matches_start = range
            .start
            .map(|start| published_date >= start)
            .unwrap_or(true);
        let matches_end = range.end.map(|end| published_date <= end).unwrap_or(true);
        (matches_start && matches_end, t.to_rfc3339())
    });

    match published_str {
        Some((matches, published_str)) => (matches, published_str),
        None if range.start.is_some() || range.end.is_some() => (false, String::new()),
        None => (true, String::new()),
    }
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

struct PodcastCandidate {
    title: String,
    podcast: String,
    published: String,
    audio_url: String,
    score: usize,
    podcast_rank: usize,
    episode_rank: usize,
}

fn normalize_search_text(value: &str) -> String {
    value
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn keyword_terms(keywords: &str) -> Vec<String> {
    normalize_search_text(keywords)
        .split(' ')
        .filter(|term| !term.is_empty())
        .map(ToString::to_string)
        .collect()
}

fn title_match_score(title: &str, keywords: &str) -> Option<usize> {
    let normalized_title = normalize_search_text(title);
    let normalized_keywords = normalize_search_text(keywords);
    let terms = keyword_terms(keywords);

    if normalized_title.is_empty() || terms.is_empty() {
        return None;
    }

    if normalized_title == normalized_keywords {
        return Some(1_000);
    }

    if normalized_title.contains(&normalized_keywords) {
        return Some(900);
    }

    if terms.iter().all(|term| normalized_title.contains(term)) {
        return Some(700 + terms.len());
    }

    None
}

fn audio_url_from_entry(entry: &feed_rs::model::Entry) -> Option<String> {
    if let Some(link) = entry.links.iter().find(|l| {
        l.media_type
            .as_deref()
            .map(|m| m.starts_with("audio"))
            .unwrap_or(false)
    }) {
        return Some(link.href.clone());
    }

    for media in &entry.media {
        for content in &media.content {
            if let Some(mime) = &content.content_type {
                if mime.to_string().starts_with("audio") {
                    if let Some(url) = &content.url {
                        return Some(url.to_string());
                    }
                }
            }
        }
    }

    None
}

fn safe_path_component(value: &str) -> String {
    let safe: String = value
        .chars()
        .filter(|c| c.is_alphanumeric() || " -_".contains(*c))
        .collect();
    let safe = safe.trim();

    if safe.is_empty() {
        "untitled".to_string()
    } else {
        safe.chars().take(50).collect()
    }
}

pub async fn podcast_search(
    State(state): State<AppState>,
    Json(request): Json<PodcastRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let date_range = podcast_date_range(&request).map_err(|e| {
        tracing::warn!("Invalid podcast date range: {}", e);
        (axum::http::StatusCode::BAD_REQUEST, e)
    })?;

    // Metrics
    {
        {
            let mut metrics = lock_metrics(&state.metrics);
            metrics.queue_size += 1;
            metrics.active_workers += 1;
        }
    }

    tracing::info!(
        "Podcast Search Request: keywords='{}', limit={}, start_date={:?}, end_date={:?}, year={:?}",
        request.keywords,
        request.limit,
        request.start_date,
        request.end_date,
        request.year
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
                ("limit", "10"),
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
                {
                    let mut metrics = lock_metrics(&state.metrics);
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

    let mut candidates = Vec::new();

    for (podcast_rank, itunes_result) in results_array.iter().enumerate() {
        let Some(feed_url) = itunes_result["feedUrl"].as_str() else {
            continue;
        };
        let collection_name = itunes_result["collectionName"]
            .as_str()
            .unwrap_or("Unknown Podcast");

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
        .await;

        let feed_resp = match feed_resp {
            Ok(resp) => resp,
            Err(e) => {
                tracing::warn!("Feed fetch failed for {}: {}", collection_name, e);
                continue;
            }
        };

        let feed_content = match feed_resp.bytes().await {
            Ok(bytes) => bytes,
            Err(e) => {
                tracing::warn!("Feed bytes error for {}: {}", collection_name, e);
                continue;
            }
        };

        let feed = match feed_rs::parser::parse(feed_content.as_ref()) {
            Ok(feed) => feed,
            Err(e) => {
                tracing::warn!("Feed parse failed for {}: {}", collection_name, e);
                continue;
            }
        };

        for (episode_rank, entry) in feed.entries.into_iter().enumerate() {
            let (matches_date_range, published_str) =
                podcast_entry_matches_date_range(entry.published, date_range);
            if !matches_date_range {
                continue;
            }

            let title = entry
                .title
                .as_ref()
                .map(|t| t.content.clone())
                .unwrap_or_else(|| "Untitled".to_string());
            let Some(score) = title_match_score(&title, &request.keywords) else {
                continue;
            };
            let Some(audio_url) = audio_url_from_entry(&entry) else {
                continue;
            };

            candidates.push(PodcastCandidate {
                title,
                podcast: collection_name.to_string(),
                published: published_str,
                audio_url,
                score,
                podcast_rank,
                episode_rank,
            });
        }
    }

    candidates.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.podcast_rank.cmp(&b.podcast_rank))
            .then_with(|| a.episode_rank.cmp(&b.episode_rank))
    });

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

    let mut results = Vec::new();
    let mut downloaded_count = 0;

    for candidate in candidates.into_iter().take(request.limit) {
        if downloaded_count >= request.limit {
            break;
        }

        let podcast_dir =
            Path::new(&base_output_path).join(safe_path_component(&candidate.podcast));
        if let Err(e) = tokio::fs::create_dir_all(&podcast_dir).await {
            tracing::warn!(
                "Failed to create podcast directory {:?}: {}",
                podcast_dir,
                e
            );
        }

        let filename = format!("{}.mp3", safe_path_component(&candidate.title));
        let filepath = podcast_dir.join(&filename);

        let mut result_entry = PodcastResult {
            title: candidate.title.clone(),
            podcast: candidate.podcast.clone(),
            published: candidate.published.clone(),
            audio_url: candidate.audio_url.clone(),
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
        let send_result = tokio::time::timeout(
            HEADER_TIMEOUT,
            download_client.get(&candidate.audio_url).send(),
        )
        .await;
        let send_result = match send_result {
            Err(_) => {
                result_entry.error =
                    Some("Audio download timed out waiting for headers (30s)".to_string());
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
                    if let Some(f) = file {
                        // Stream loop runs inside a block so `f` is dropped
                        // before remove_file — on Windows an open handle
                        // prevents deletion, which would leave partial files
                        // on disk despite the cleanup below.
                        let success = {
                            let mut f = f;
                            let mut stream = res.bytes_stream();
                            let mut success = true;
                            // Per-chunk idle timeout: fires only when the origin
                            // stops sending data, not during normal slow transfers.
                            // This releases the concurrency slot from stalled
                            // origins without cutting off healthy large episodes.
                            const CHUNK_IDLE_TIMEOUT: std::time::Duration =
                                std::time::Duration::from_secs(30);
                            // Hard cap per episode to prevent disk/memory exhaustion
                            // from unexpectedly large files or malicious origins.
                            const MAX_AUDIO_BYTES: u64 = 200 * 1024 * 1024; // 200 MB
                            let mut bytes_written: u64 = 0;
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
                                        bytes_written += chunk.len() as u64;
                                        if bytes_written > MAX_AUDIO_BYTES {
                                            result_entry.error = Some(format!(
                                                "Audio file too large (exceeded {} MB limit)",
                                                MAX_AUDIO_BYTES / 1024 / 1024
                                            ));
                                            success = false;
                                            break;
                                        }
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
                            success
                        }; // `f` is dropped here — handle closed before remove_file

                        if !success {
                            // Remove the partial file so repeated failed attempts
                            // don't accumulate ~200 MB of incomplete data on disk.
                            let _ = tokio::fs::remove_file(&filepath).await;
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

    // Metrics cleanup
    {
        {
            let mut metrics = lock_metrics(&state.metrics);
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
        "start_date": request.start_date,
        "end_date": request.end_date,
        "year": request.year,
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
        tracing::warn!(
            "Failed to save podcast results to S3 bucket {}: {}",
            bucket_name,
            e
        );
    }

    Ok(Json(json!({"data": results})))
}
