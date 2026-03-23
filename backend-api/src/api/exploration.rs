use axum::{extract::State, Json};
use chrono::Utc;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;
use std::time::SystemTime;
use url::Url;
use uuid::Uuid;

use crate::services::{crawler, s3, sanitize_bucket_name, validate_url};
use crate::state::{lock_metrics, AppState};

static LINK_REGEX: OnceLock<regex::Regex> = OnceLock::new();

#[derive(Deserialize)]
pub struct ExplorationRequest {
    pub url: String,
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub output_dir: Option<String>,
    pub job_id: Option<String>,
}

fn default_limit() -> i32 {
    1
}

pub async fn ai_exploration(
    State(state): State<AppState>,
    Json(request): Json<ExplorationRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Validate before touching metrics so an invalid URL never inflates counters.
    if let Err(e) = validate_url(&request.url).await {
        return Err((axum::http::StatusCode::UNPROCESSABLE_ENTITY, e));
    }

    // Metrics — incremented only after validation passes
    {
        { let mut metrics = lock_metrics(&state.metrics);
            metrics.queue_size += 1;
            metrics.active_workers += 1;
        }
    }

    let mut current_url = request.url.clone();
    let limit = request.limit;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "explore-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    let mut results = Vec::new();

    // Regex for markdown links [text](url)
    let link_regex = LINK_REGEX.get_or_init(|| regex::Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").expect("valid regex"));

    for page_num in 0..limit {
        tracing::info!(
            "Exploration: Crawling Page {}: {}",
            page_num + 1,
            current_url
        );

        // 1. Crawl Page
        let crawl_req = crawler::CrawlerRequest {
            urls: vec![current_url.clone()],
            run_mode: None,
            api_key: None,
            prompt: None,
            model: None,
            output_dir: None,
            bucket_name: None,
            ignore_links: None,
        };

        let crawl_res = crawler::call_crawler_service(&crawl_req, state.domain_throttle.clone()).await;
        if let Err(e) = crawl_res {
            tracing::error!("Failed to crawl page {}: {}", current_url, e);
            break;
        }

        let crawl_data = crawl_res.expect("crawl_res was already checked above");
        if crawl_data.results.is_empty() || !crawl_data.results[0].success {
            break;
        }

        let page_markdown = crawl_data.results[0].markdown.clone().unwrap_or_default();

        // 2. Extract Links
        let mut article_links = HashSet::new();
        let mut next_page_url = None;

        for cap in link_regex.captures_iter(&page_markdown) {
            let text = &cap[1];
            let href = &cap[2];

            if let Ok(full_url) = Url::parse(&current_url).and_then(|base| base.join(href)) {
                let full_url_str = full_url.to_string();

                // Heuristics
                if href.len() > 20
                    && full_url_str != current_url
                    && ["/news/", "/articles/", "/tech/", "/story/"]
                        .iter()
                        .any(|s| full_url_str.contains(s))
                {
                    article_links.insert(full_url_str.clone());
                }

                if text.contains("Next") || text.contains("下一頁") || text.trim() == ">" {
                    next_page_url = Some(full_url_str);
                }
            }
        }

        tracing::info!(
            "Found {} potential articles on page {}",
            article_links.len(),
            page_num + 1
        );

        // 3. Crawl Articles (Top 5)
        // Re-validate each discovered link: an attacker-controlled seed page
        // could embed absolute links to internal addresses.
        let articles_to_crawl: Vec<String> = article_links.into_iter().take(5).collect();
        for (i, link) in articles_to_crawl.iter().enumerate() {
            if let Err(e) = validate_url(link).await {
                tracing::warn!("Skipping article link that failed URL validation: {}", e);
                continue;
            }
            let art_req = crawler::CrawlerRequest {
                urls: vec![link.clone()],
                run_mode: None,
                api_key: None,
                prompt: None,
                model: None,
                output_dir: None,
                bucket_name: None,
                ignore_links: None,
            };

            if let Ok(art_resp) = crawler::call_crawler_service(&art_req, state.domain_throttle.clone()).await {
                if !art_resp.results.is_empty() && art_resp.results[0].success {
                    let art_data = &art_resp.results[0];
                    if let Some(md) = &art_data.markdown {
                        let s3_key = format!("page{}_article{}.md", page_num + 1, i + 1);
                        let content_with_header = format!("# Source: {}\n\n{}", link, md);

                        let s3_path = s3::save_to_rustfs_content(
                            &state.s3_client,
                            &bucket_name,
                            &s3_key,
                            &content_with_header,
                        )
                        .await;

                        // Optional: save local
                        if let Some(dir) = &request.output_dir {
                            if let Err(e) = tokio::fs::create_dir_all(dir).await {
                                tracing::warn!("Failed to create output directory {}: {}", dir, e);
                            } else {
                                let filepath = Path::new(dir).join(format!(
                                    "page{}_article{}.md",
                                    page_num + 1,
                                    i + 1
                                ));
                                if let Err(e) = tokio::fs::write(&filepath, &content_with_header).await {
                                    tracing::warn!("Failed to write article to {:?}: {}", filepath, e);
                                }
                            }
                        }

                        results.push(json!({
                            "url": link,
                            "success": true,
                            "markdown": md, // Maybe too large? Python included it.
                            "s3_path": s3_path.ok(),
                            "s3_bucket": bucket_name
                        }));
                    }
                }
            }
        }

        // Pagination — validate before following to prevent SSRF via a next-page link
        // embedded in an attacker-controlled page.
        match next_page_url {
            Some(next) if validate_url(&next).await.is_ok() => {
                current_url = next;
            }
            _ => break,
        }
    }

    // Metrics cleanup
    {
        { let mut metrics = lock_metrics(&state.metrics);
            if metrics.queue_size > 0 {
                metrics.queue_size -= 1;
            }
            if metrics.active_workers > 0 {
                metrics.active_workers -= 1;
            }
        }
    }

    // Save search_results.json to RustFS (source: AI Exploration)
    let title = format!("AI Exploration: {}", &request.url);
    let created_at = Utc::now().to_rfc3339();

    let result_json_content = serde_json::to_string_pretty(&json!({
        "job_type": "exploration",
        "source": "Crawl4AI AI Exploration",
        "title": title,
        "created_at": created_at,
        "url": request.url,
        "limit": request.limit,
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
        tracing::warn!("Failed to save exploration results to S3 bucket {}: {}", bucket_name, e);
    }

    Ok(Json(json!({"data": results})))
}
