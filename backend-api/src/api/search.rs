use axum::{extract::State, Json};
use backon::{ExponentialBuilder, Retryable};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::services::{crawler, s3, sanitize_bucket_name};
use crate::state::{lock_metrics, AppState};

#[derive(Deserialize)]
pub struct SearchRequest {
    pub keywords: Vec<String>,
    #[serde(default = "default_num_results")]
    pub num_results: i32,
    pub output_dir: Option<String>,
    pub time_limit: Option<String>,
    pub site: Option<String>,
    #[serde(default)]
    pub target_website: Option<bool>,
    pub job_id: Option<String>,
    #[serde(default)]
    pub ignore_links: Option<bool>,
}

fn default_num_results() -> i32 {
    10
}

// List of target websites for filtering
const TARGET_SITES: &[&str] = &[
    "ycombinator.com",
    "techcrunch.com",
    "infoq.com",
    "a16z.com",
    "ithome.com.tw",
    "explainthis.io",
    "theinformation.com",
    "cncf.io",
];

pub async fn search_aggregate(
    State(state): State<AppState>,
    Json(request): Json<SearchRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // 1. Metrics update
    {
        { let mut metrics = lock_metrics(&state.metrics);
            metrics.queue_size += 1;
        }
    }

    let start_time = SystemTime::now();
    let result = search_logic(state.clone(), request, start_time).await;

    // Metric update cleanup
    {
        { let mut metrics = lock_metrics(&state.metrics);
            if metrics.queue_size > 0 {
                metrics.queue_size -= 1;
            }

            // Update history
            if let Ok(elapsed) = start_time.elapsed() {
                metrics.request_history.push_back(elapsed.as_secs_f64());
                if metrics.request_history.len() > 50 {
                    metrics.request_history.pop_front();
                }
            }
        }
    }

    result
}

async fn search_logic(
    state: AppState,
    request: SearchRequest,
    _start_time: SystemTime,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let google_api_key = state.google_api_key.clone();
    let google_cx = state.google_cx.clone();

    // 2. Google Custom Search
    // Use a HashSet to accumulate unique URLs as we go, so that the remaining
    // quota for each keyword reflects unique links collected so far rather than
    // raw hit counts.  Duplicate-heavy keywords won't exhaust the budget and
    // block later keywords that may return genuinely new links.
    let mut seen_urls: std::collections::HashSet<String> = std::collections::HashSet::new();
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    // Google Custom Search API hard limit: start ≤ 91 → max 100 results per query.
    //
    // To exceed 100 results for the same keyword we issue multiple queries, each
    // covering a non-overlapping 6-month date window via `after:` / `before:`
    // operators in the query string.  Up to MAX_DATE_BATCHES windows are used,
    // giving a ceiling of 500 results per keyword.
    //
    // When the caller already supplies a `time_limit`, the date range is already
    // constrained, so we skip window-batching and use a single query instead.
    const MAX_DATE_BATCHES: usize = 5;
    const DATE_WINDOW_DAYS: i64 = 180; // ~6 months per window

    for keyword in &request.keywords {
        let unique_so_far = seen_urls.len() as i32;
        if unique_so_far >= request.num_results {
            break;
        }

        // Give this keyword the full remaining unique-URL quota.
        // Basing this on seen_urls.len() rather than raw hit counts means
        // duplicate-heavy early keywords don't starve later keywords.
        let per_keyword_limit = (request.num_results - unique_so_far)
            .min(100 * MAX_DATE_BATCHES as i32);

        let date_batches_needed: usize = if request.time_limit.is_none() {
            ((per_keyword_limit + 99) / 100).max(1) as usize
        } else {
            1 // caller-supplied time_limit already restricts the date range
        };

        let today = chrono::Utc::now().date_naive();
        let mut keyword_total = 0i32;

        'date_batches: for batch_idx in 0..date_batches_needed {
            // Build the query for this date window.
            let mut base_query = keyword.clone();

            if date_batches_needed > 1 {
                // Window 0 = most recent DATE_WINDOW_DAYS, window 1 = next older, …
                let days_end = batch_idx as i64 * DATE_WINDOW_DAYS;
                let days_start = days_end + DATE_WINDOW_DAYS;
                let before = today - chrono::Duration::days(days_end);
                let after = today - chrono::Duration::days(days_start);
                base_query = format!(
                    "{} after:{} before:{}",
                    base_query,
                    after.format("%Y-%m-%d"),
                    before.format("%Y-%m-%d"),
                );
            }

            // Append static filters on top of the (possibly date-windowed) query.
            if let Some(true) = request.target_website {
                let site_filter = TARGET_SITES
                    .iter()
                    .map(|site| format!("site:{}", site))
                    .collect::<Vec<String>>()
                    .join(" OR ");
                base_query = format!("({}) AND ({})", base_query, site_filter);
                tracing::info!("Applied Target Website Filter: {}", base_query);
            }
            if let Some(site) = &request.site {
                base_query = format!("{} site:{}", base_query, site);
            }

            let batch_limit = (per_keyword_limit - keyword_total).min(100);
            let mut batch_fetched = 0i32;
            let mut start_index = 1i32;

            loop {
                if batch_fetched >= batch_limit {
                    break;
                }

                // Google API: start > 91 returns 400.
                if start_index > 91 {
                    tracing::info!(
                        "Google CSE pagination limit reached for '{}' (window {}): {} total so far",
                        keyword, batch_idx, keyword_total
                    );
                    break;
                }

                let num = (batch_limit - batch_fetched).min(10);

                let url = "https://www.googleapis.com/customsearch/v1";
                let mut params: Vec<(String, String)> = vec![
                    ("key".into(), google_api_key.clone()),
                    ("cx".into(), google_cx.clone()),
                    ("q".into(), base_query.clone()),
                    ("num".into(), num.to_string()),
                    ("start".into(), start_index.to_string()),
                ];
                if let Some(ref t) = request.time_limit {
                    params.push(("dateRestrict".into(), t.clone()));
                }
                if let Some(ref s) = request.site {
                    params.push(("siteSearch".into(), s.clone()));
                    params.push(("siteSearchFilter".into(), "i".into()));
                }

                let google_result = (|| {
                    let p = params.clone();
                    let c = client.clone();
                    async move {
                        c.get(url)
                            .query(&p)
                            .send()
                            .await
                            .map_err(|e| format!("Google Search failed: {}", e))
                            .and_then(|r| {
                                if r.status().is_server_error() {
                                    Err(format!("Google Search server error: {}", r.status()))
                                } else {
                                    Ok(r)
                                }
                            })
                    }
                })
                .retry(
                    ExponentialBuilder::default()
                        .with_min_delay(Duration::from_millis(300))
                        .with_max_delay(Duration::from_secs(5))
                        .with_max_times(3),
                )
                .await;

                match google_result {
                    Err(e) => {
                        tracing::error!("Error searching for {}: {}", keyword, e);
                        break;
                    }
                    Ok(resp) => {
                        if resp.status().is_success() {
                            if let Ok(data) = resp.json::<Value>().await {
                                if let Some(items) = data["items"].as_array() {
                                    let count = items.len();
                                    if count == 0 {
                                        break;
                                    }
                                    for item in items {
                                        if let Some(link) = item["link"].as_str() {
                                            seen_urls.insert(link.to_string());
                                        }
                                    }
                                    batch_fetched += count as i32;
                                    keyword_total += count as i32;
                                    start_index += count as i32;
                                    if count < num as usize {
                                        break;
                                    }
                                } else {
                                    break;
                                }
                            } else {
                                tracing::error!(
                                    "Failed to parse Google Search response JSON for keyword '{}'",
                                    keyword
                                );
                                break;
                            }
                        } else {
                            tracing::warn!(
                                "Google Search API terminated early: Status {} for keyword '{}' at start_index {}",
                                resp.status(),
                                keyword,
                                start_index
                            );
                            break;
                        }
                    }
                }
            } // end inner pagination loop

            if keyword_total >= per_keyword_limit {
                break 'date_batches;
            }
        } // end date_batches

    }

    // seen_urls is already deduplicated; convert to Vec for the crawler.
    let unique_urls: Vec<String> = seen_urls.into_iter().collect();

    if unique_urls.is_empty() {
        return Ok(Json(json!({"data": []})));
    }

    // Update active workers
    {
        let mut metrics = lock_metrics(&state.metrics);
        metrics.active_workers += unique_urls.len();
    }

    // 3. Call Crawler Service
    let crawl_res = crawler::call_crawler_service(
        &crawler::CrawlerRequest {
            urls: unique_urls.clone(),
            run_mode: None,
            api_key: None,
            prompt: None,
            model: None,
            output_dir: request.output_dir.clone(),
            bucket_name: None,
            ignore_links: request.ignore_links,
        },
        state.domain_throttle.clone(),
    )
    .await;

    {
        let mut metrics = lock_metrics(&state.metrics);
        metrics.active_workers = metrics.active_workers.saturating_sub(unique_urls.len());
    }

    let mut aggregated_results = match crawl_res {
        Ok(res) => res.results,
        Err(e) => {
            return Err((
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("Crawler service failed: {}", e),
            ));
        }
    };

    // 4. Save to RustFS
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "search-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    // Save JSON results
    let _json_content = json!({
        "keywords": request.keywords,
        "results": aggregated_results // This is CrawlResult struct, we need to ensure it serializes well or convert to Value
        // Serde default serialization should work
    });

    // We need to re-structure `aggregated_results` slightly to match the Python output which adds `s3_path` etc.
    // Also we need `hashlib`. Usage of `md5` crate.

    let mut final_results = Vec::new();

    // Create bucket once
    // Actually our s3 helper checks/creates bucket on each call, which is fine but inefficient.
    // We can assume first call creates it.

    for (i, item) in aggregated_results.iter_mut().enumerate() {
        let mut item_value = match serde_json::to_value(&*item) {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to serialize crawl result for {}: {}", item.url, e);
                continue;
            }
        };

        // Inject info
        if let Some(obj) = item_value.as_object_mut() {
            obj.insert("s3_bucket".to_string(), json!(bucket_name));
        }

        if item.success {
            if let Some(md) = &item.markdown {
                let url_hash = format!("{:x}", md5::compute(item.url.as_bytes()));
                let url_hash_short = &url_hash[..8];
                let s3_key = format!("result_{}_{}.md", i, url_hash_short);

                let s3_path =
                    s3::save_to_rustfs_content(&state.s3_client, &bucket_name, &s3_key, md).await;

                if let Some(obj) = item_value.as_object_mut() {
                    if let Ok(path) = s3_path {
                        obj.insert("s3_path".to_string(), json!(path));
                    }
                }
            }
        }
        final_results.push(item_value);
    }

    // Generate title from keywords (source: Google Custom Search API)
    let title = format!("Google Search: {}", request.keywords.join(", "));
    let created_at = Utc::now().to_rfc3339();

    let result_json_content = serde_json::to_string_pretty(&json!({
        "job_type": "news",
        "source": "Google Custom Search API",
        "title": title,
        "created_at": created_at,
        "keywords": request.keywords,
        "results": final_results
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
        tracing::warn!("Failed to save search results to S3 bucket {}: {}", bucket_name, e);
    }

    // Optional: Save to local output_dir
    if let Some(dir) = request.output_dir {
        if let Err(e) = tokio::fs::create_dir_all(&dir).await {
            tracing::warn!("Failed to create output directory {}: {}", dir, e);
        } else {
            let filename = format!("search_results_{}.json", timestamp);
            let filepath = std::path::Path::new(&dir).join(filename);
            if let Err(e) = tokio::fs::write(filepath, &result_json_content).await {
                tracing::warn!("Failed to write local results to {}: {}", dir, e);
            }
        }
    }

    Ok(Json(json!({"data": final_results})))
}
