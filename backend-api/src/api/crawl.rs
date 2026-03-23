use axum::{extract::State, Json};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;
use std::sync::OnceLock;
use std::time::SystemTime;
use uuid::Uuid;

use crate::services::{crawler, s3, sanitize_bucket_name, validate_url};
use crate::state::AppState;

static URL_REGEX: OnceLock<regex::Regex> = OnceLock::new();

#[derive(Deserialize)]
pub struct AgentCrawlRequest {
    pub url: String,
    pub prompt: String,
    #[serde(default = "default_model")]
    pub model: String,
    pub job_id: Option<String>,
    #[serde(default)]
    pub ignore_links: Option<bool>,
}

fn default_model() -> String {
    "gpt-4o".to_string()
}

#[derive(Deserialize)]
pub struct BatchCrawlRequest {
    pub urls: Vec<String>,
    #[serde(default = "default_run_mode")]
    pub run_mode: String,
    pub output_dir: Option<String>,
    #[serde(default)]
    pub sync: Option<bool>,
    pub job_id: Option<String>,
    #[serde(default)]
    pub ignore_links: Option<bool>,
}

fn default_run_mode() -> String {
    "lite".to_string()
}

pub async fn agent_crawl(
    State(state): State<AppState>,
    Json(request): Json<AgentCrawlRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Extract URL from prompt if present, to override default/hardcoded URL
    let mut target_url = request.url.clone();

    // Frontend sends "https://google.com" by default. If we find a specific URL in prompt, use it.
    let url_regex = URL_REGEX.get_or_init(|| regex::Regex::new(r"https?://[^\s,]+").expect("valid regex"));
    if let Some(mat) = url_regex.find(&request.prompt) {
        target_url = mat.as_str().to_string();
        tracing::info!("Extracted URL from prompt: {}", target_url);
    }

    // Save prompt for later use in JSON (before it's moved)
    let prompt_for_json = request.prompt.clone();

    // Validate before touching metrics so an invalid URL never inflates counters.
    if let Err(e) = validate_url(&target_url).await {
        return Err((axum::http::StatusCode::UNPROCESSABLE_ENTITY, e));
    }

    // Metrics — incremented only after validation passes
    {
        if let Ok(mut metrics) = state.metrics.lock() {
            metrics.queue_size += 1;
            metrics.active_workers += 1;
        }
    }

    // Call Crawler with agent mode
    let crawl_req = crawler::CrawlerRequest {
        urls: vec![target_url],
        run_mode: Some("agent".to_string()),
        api_key: Some(state.openai_api_key.clone()),
        prompt: Some(request.prompt),
        model: Some(request.model),
        output_dir: None,
        bucket_name: None,
        ignore_links: request.ignore_links,
    };

    let crawl_res = crawler::call_crawler_service(&crawl_req).await;

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

    let results = match crawl_res {
        Ok(res) => res.results,
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Agent service failed: {}", e),
            ));
        }
    };

    // Save
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "agent-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    let mut processed_results = Vec::new();

    for (i, item) in results.iter().enumerate() {
        let mut item_val = match serde_json::to_value(item) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to serialize agent crawl result for {}: {}", item.url, e);
                continue;
            }
        };
        if let Some(obj) = item_val.as_object_mut() {
            obj.insert("s3_bucket".to_string(), json!(bucket_name));

            if item.success {
                if let Some(md) = &item.markdown {
                    let url_hash = format!("{:x}", md5::compute(item.url.as_bytes()));
                    let s3_key = format!("agent_{}_{}.md", i, &url_hash[..8]);

                    if let Ok(path) =
                        s3::save_to_rustfs_content(&state.s3_client, &bucket_name, &s3_key, md)
                            .await
                    {
                        obj.insert("s3_path".to_string(), json!(path));
                    }
                }
            }
        }
        processed_results.push(item_val);
    }

    // Save search_results.json to RustFS (source: Crawl4AI Agent)
    let title = format!(
        "Agent Crawl: {}",
        &prompt_for_json.chars().take(50).collect::<String>()
    );
    let created_at = Utc::now().to_rfc3339();

    let result_json_content = serde_json::to_string_pretty(&json!({
        "job_type": "agent",
        "source": "Crawl4AI Agent",
        "title": title,
        "created_at": created_at,
        "prompt": prompt_for_json,
        "results": processed_results
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
        tracing::warn!("Failed to save agent results to S3 bucket {}: {}", bucket_name, e);
    }

    Ok(Json(json!({"data": processed_results})))
}

pub async fn batch_crawl(
    State(state): State<AppState>,
    Json(request): Json<BatchCrawlRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Validate all URLs before any processing so we don't enqueue or partially
    // execute a request that contains a disallowed target.
    let mut invalid = Vec::new();
    for u in &request.urls {
        if let Err(e) = validate_url(u).await {
            invalid.push(e);
        }
    }
    if !invalid.is_empty() {
        return Err((
            axum::http::StatusCode::UNPROCESSABLE_ENTITY,
            format!("Invalid URL(s): {}", invalid.join("; ")),
        ));
    }

    // Generate bucket name upfront for tracking and deletion
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "async-crawl-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    let crawl_req = crawler::CrawlerRequest {
        urls: request.urls.clone(),
        run_mode: Some(request.run_mode.clone()),
        api_key: None,
        prompt: None,
        model: None,
        output_dir: request.output_dir.clone(),
        bucket_name: Some(bucket_name.clone()),
        ignore_links: request.ignore_links,
    };

    // If sync is true, process immediately and wait for results (similar to agent_crawl logic)
    if let Some(true) = request.sync {
        // Metrics update
        {
            if let Ok(mut metrics) = state.metrics.lock() {
                metrics.queue_size += 1;
                metrics.active_workers += 1;
            }
        }

        let crawl_res = crawler::call_crawler_service(&crawl_req).await;

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

        let results = match crawl_res {
            Ok(res) => res.results,
            Err(e) => {
                return Err((
                    axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Crawl service failed: {}", e),
                ));
            }
        };

        // Save Results to S3
        let mut processed_results = Vec::new();
        for (i, item) in results.iter().enumerate() {
            let mut item_val = match serde_json::to_value(item) {
                Ok(v) => v,
                Err(e) => {
                    tracing::error!("Failed to serialize batch crawl result for {}: {}", item.url, e);
                    continue;
                }
            };
            if let Some(obj) = item_val.as_object_mut() {
                obj.insert("s3_bucket".to_string(), json!(bucket_name));

                if item.success {
                    if let Some(md) = &item.markdown {
                        let url_hash = format!("{:x}", md5::compute(item.url.as_bytes()));
                        let s3_key = format!("crawl_{}_{}.md", i, &url_hash[..8]);

                        if let Ok(path) =
                            s3::save_to_rustfs_content(&state.s3_client, &bucket_name, &s3_key, md)
                                .await
                        {
                            obj.insert("s3_path".to_string(), json!(path));
                        }
                    }
                }
            }
            processed_results.push(item_val);
        }

        // Save search_results.json to RustFS (source: Crawl4AI Batch)
        let title = format!("Batch Crawl: {} URLs", request.urls.len());
        let created_at = Utc::now().to_rfc3339();

        let result_json_content = serde_json::to_string_pretty(&json!({
            "job_type": "crawl",
            "source": "Crawl4AI Batch Crawler",
            "title": title,
            "created_at": created_at,
            "urls": request.urls,
            "results": processed_results
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
            tracing::warn!("Failed to save batch results to S3 bucket {}: {}", bucket_name, e);
        }

        return Ok(Json(json!({
            "success": true,
            "message": "Batch crawl completed synchronously",
            "data": processed_results
        })));
    }

    // Non-blocking gate check. shutdown_signal() stores false (Release) before
    // signalling the worker; we load with Acquire so we see the store.
    // Requests that loaded true before the store may still enqueue; worst-case
    // leakage is bounded by the concurrency limiter (200 slots). Those tasks
    // remain in Redis until the next server start — acceptable vs. a mutex that
    // can block shutdown indefinitely when Redis is slow.
    if !state.enqueue_gate.load(Ordering::Acquire) {
        return Err((
            axum::http::StatusCode::SERVICE_UNAVAILABLE,
            "Server is shutting down, please retry".to_string(),
        ));
    }

    let result = state.queue_service.enqueue(crawl_req).await;

    match result {
        Ok(_) => Ok(Json(json!({
            "success": true,
            "message": "Batch crawl task submitted to queue",
            "data": {
                "urls": request.urls,
                "status": "queued",
                "s3_bucket": bucket_name
            }
        }))),
        Err(e) => {
            tracing::error!("Failed to enqueue batch crawl: {}", e);
            Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Failed to enqueue task: {}", e),
            ))
        }
    }
}
