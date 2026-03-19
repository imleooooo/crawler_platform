use axum::{extract::State, Json};
use chrono::Utc;
use regex::Regex;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::path::Path;
use std::time::SystemTime;
use url::Url;
use uuid::Uuid;

use crate::services::{crawler, s3, sanitize_bucket_name};
use crate::state::AppState;

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
    // Metrics
    {
        let mut metrics = state.metrics.lock().unwrap();
        metrics.queue_size += 1;
        metrics.active_workers += 1;
    }

    let mut current_url = request.url.clone();
    let limit = request.limit;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
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
    let link_regex = Regex::new(r"\[([^\]]+)\]\(([^)]+)\)").unwrap();

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

        let crawl_res = crawler::call_crawler_service(&crawl_req).await;
        if let Err(e) = crawl_res {
            tracing::error!("Failed to crawl page {}: {}", current_url, e);
            break;
        }

        let crawl_data = crawl_res.unwrap();
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
        let articles_to_crawl: Vec<String> = article_links.into_iter().take(5).collect();
        for (i, link) in articles_to_crawl.iter().enumerate() {
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

            if let Ok(art_resp) = crawler::call_crawler_service(&art_req).await {
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
                            let _ = tokio::fs::create_dir_all(dir).await;
                            let filepath = Path::new(dir).join(format!(
                                "page{}_article{}.md",
                                page_num + 1,
                                i + 1
                            ));
                            let _ = tokio::fs::write(filepath, &content_with_header).await;
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

        // Pagination
        if let Some(next) = next_page_url {
            current_url = next;
        } else {
            break;
        }
    }

    // Metrics cleanup
    {
        let mut metrics = state.metrics.lock().unwrap();
        metrics.queue_size -= 1;
        metrics.active_workers -= 1;
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
    .unwrap();

    let _ = s3::save_to_rustfs_content(
        &state.s3_client,
        &bucket_name,
        "search_results.json",
        &result_json_content,
    )
    .await;

    Ok(Json(json!({"data": results})))
}
