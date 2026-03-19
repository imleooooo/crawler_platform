use axum::{extract::State, Json};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::time::SystemTime;
use uuid::Uuid;

use crate::services::{crawler, s3, sanitize_bucket_name};
use crate::state::AppState;

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
        if let Ok(mut metrics) = state.metrics.lock() {
            metrics.queue_size += 1;
        }
    }

    let start_time = SystemTime::now();
    let result = search_logic(state.clone(), request, start_time).await;

    // Metric update cleanup
    {
        if let Ok(mut metrics) = state.metrics.lock() {
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
    let mut all_urls = Vec::new();
    let client = reqwest::Client::new();

    for keyword in &request.keywords {
        let mut fetched_count = 0;
        let mut start_index = 1;

        // Loop to fetch results in batches/pages
        loop {
            if fetched_count >= request.num_results {
                break;
            }

            // Determine how many to fetch in this batch (max 10 allowed by Google API)
            let num = (request.num_results - fetched_count).min(10);

            // Safety check for Google API typical limits (optional, but prevents wasted calls)

            // Construct Query
            let mut query = keyword.clone();

            // If target_website is true, append site filters
            if let Some(true) = request.target_website {
                let site_filter = TARGET_SITES
                    .iter()
                    .map(|site| format!("site:{}", site))
                    .collect::<Vec<String>>()
                    .join(" OR ");
                query = format!("({}) AND ({})", query, site_filter);
                tracing::info!("Applied Target Website Filter: {}", query);
            }

            // Also append manual site if provided (legacy)
            if let Some(site) = &request.site {
                query = format!("{} site:{}", query, site);
            }

            let url = "https://www.googleapis.com/customsearch/v1";
            let mut req_builder = client.get(url).query(&[
                ("key", &google_api_key),
                ("cx", &google_cx),
                ("q", &query),
                ("num", &num.to_string()),
                ("start", &start_index.to_string()),
            ]);

            if let Some(ref t) = request.time_limit {
                req_builder = req_builder.query(&[("dateRestrict", t)]);
            }
            if let Some(ref s) = request.site {
                req_builder =
                    req_builder.query(&[("siteSearch", s.as_str()), ("siteSearchFilter", "i")]);
            }

            match req_builder.send().await {
                Ok(resp) => {
                    if resp.status().is_success() {
                        if let Ok(data) = resp.json::<Value>().await {
                            if let Some(items) = data["items"].as_array() {
                                let count = items.len();
                                if count == 0 {
                                    break; // No results returned
                                }

                                for item in items {
                                    if let Some(link) = item["link"].as_str() {
                                        all_urls.push(link.to_string());
                                    }
                                }

                                fetched_count += count as i32;
                                start_index += count as i32;

                                // If we received fewer items than requested for this page, we're likely at the end
                                if count < num as usize {
                                    break;
                                }
                            } else {
                                // "items" key missing implies 0 results
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
                        // If we hit an error (e.g., 400 Bad Request due to deep pagination), we stop this keyword
                        tracing::warn!(
                            "Google Search API terminated early: Status {} for keyword '{}' at start_index {}",
                            resp.status(),
                            keyword,
                            start_index
                        );
                        break;
                    }
                }
                Err(e) => {
                    tracing::error!("Error searching for {}: {}", keyword, e);
                    break;
                }
            }

            // Optional: Sleep briefly to avoid aggressive rate limiting if fetching many pages?
            // tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    }

    // Deduplicate
    let unique_urls: Vec<String> = all_urls
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect();

    if unique_urls.is_empty() {
        return Ok(Json(json!({"data": []})));
    }

    // Update active workers
    {
        if let Ok(mut metrics) = state.metrics.lock() {
            metrics.active_workers += unique_urls.len();
        }
    }

    // 3. Call Crawler Service
    let crawl_res = crawler::call_crawler_service(&crawler::CrawlerRequest {
        urls: unique_urls.clone(),
        run_mode: None,
        api_key: None,
        prompt: None,
        model: None,
        output_dir: request.output_dir.clone(),
        bucket_name: None,
        ignore_links: request.ignore_links,
    })
    .await;

    // Decrement workers
    {
        if let Ok(mut metrics) = state.metrics.lock() {
            if metrics.active_workers >= unique_urls.len() {
                metrics.active_workers -= unique_urls.len();
            } else {
                metrics.active_workers = 0;
            }
        }
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
