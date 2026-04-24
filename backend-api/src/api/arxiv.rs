use axum::{extract::State, Json};
use backon::{ExponentialBuilder, Retryable};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::{Duration, SystemTime};
use uuid::Uuid;

use crate::services::{s3, sanitize_bucket_name};
use crate::state::{lock_metrics, AppState};

#[derive(Deserialize)]
pub struct ArxivRequest {
    pub keywords: String,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub year: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub output_dir: Option<String>,
    pub job_id: Option<String>,
}

fn default_limit() -> i32 {
    5
}

#[derive(Clone, Copy)]
struct ArxivDateRange {
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

fn arxiv_date_range(request: &ArxivRequest) -> Result<ArxivDateRange, String> {
    let start = parse_optional_date(request.start_date.as_ref(), "start_date")?;
    let end = parse_optional_date(request.end_date.as_ref(), "end_date")?;

    if let (Some(start), Some(end)) = (start, end) {
        if start > end {
            return Err("start_date must be earlier than or equal to end_date".to_string());
        }
    }

    if start.is_some() || end.is_some() {
        return Ok(ArxivDateRange { start, end });
    }

    let Some(year) = request
        .year
        .as_ref()
        .map(|y| y.trim())
        .filter(|y| !y.is_empty())
    else {
        return Ok(ArxivDateRange {
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

    Ok(ArxivDateRange {
        start: Some(start),
        end: Some(end),
    })
}

fn arxiv_submitted_date_query(range: ArxivDateRange) -> Option<String> {
    if range.start.is_none() && range.end.is_none() {
        return None;
    }

    let start = range
        .start
        .map(|date| date.format("%Y%m%d").to_string())
        .unwrap_or_else(|| "00010101".to_string());
    let end = range
        .end
        .map(|date| date.format("%Y%m%d").to_string())
        .unwrap_or_else(|| "99991231".to_string());

    Some(format!("submittedDate:[{}0000 TO {}2359]", start, end))
}

#[derive(Serialize)]
struct ArxivResult {
    title: String,
    authors: Vec<String>,
    published: String,
    pdf_url: String,
    local_path: Option<String>,
    s3_path: Option<String>,
    s3_bucket: String,
    error: Option<String>,
}

fn normalize_query_input(keywords: &str) -> String {
    keywords.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn escape_arxiv_phrase(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn build_arxiv_query(keywords: &str, date_range: ArxivDateRange, title_only: bool) -> String {
    let normalized_keywords = normalize_query_input(keywords);
    let escaped_keywords = escape_arxiv_phrase(&normalized_keywords);
    let search_field = if title_only { "ti" } else { "all" };
    let mut query = format!(r#"{search_field}:"{escaped_keywords}""#);

    if let Some(date_query) = arxiv_submitted_date_query(date_range) {
        query.push_str(" AND ");
        query.push_str(&date_query);
    }

    query
}

pub async fn arxiv_search(
    State(state): State<AppState>,
    Json(request): Json<ArxivRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    let date_range = arxiv_date_range(&request).map_err(|e| {
        tracing::warn!("Invalid arxiv date range: {}", e);
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

    let url = "http://export.arxiv.org/api/query";
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    let limit_str = request.limit.to_string();
    let fetch_feed = |query: String| {
        let client = client.clone();
        let limit_str = limit_str.clone();
        async move {
            let resp = (|| async {
                client
                    .get(url)
                    .query(&[
                        ("search_query", query.as_str()),
                        ("start", "0"),
                        ("max_results", limit_str.as_str()),
                        ("sortBy", "submittedDate"),
                        ("sortOrder", "descending"),
                    ])
                    .send()
                    .await
                    .map_err(|e| format!("ArXiv API failed: {}", e))
                    .and_then(|r| {
                        if r.status().is_server_error() {
                            Err(format!("ArXiv server error: {}", r.status()))
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
            .await?;

            if !resp.status().is_success() {
                return Err(format!("ArXiv API returned {}", resp.status()));
            }

            let xml_content = resp
                .bytes()
                .await
                .map_err(|e| format!("Failed to read XML: {}", e))?;

            feed_rs::parser::parse(xml_content.as_ref())
                .map_err(|e| format!("Failed to parse Atom: {}", e))
        }
    };

    let title_query = build_arxiv_query(&request.keywords, date_range, true);
    let mut feed = fetch_feed(title_query.clone())
        .await
        .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;

    if feed.entries.is_empty() {
        let fallback_query = build_arxiv_query(&request.keywords, date_range, false);
        tracing::info!(
            "ArXiv title query returned no results; falling back to all-fields query: {}",
            fallback_query
        );
        feed = fetch_feed(fallback_query)
            .await
            .map_err(|e| (axum::http::StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    } else {
        tracing::info!("ArXiv title query matched: {}", title_query);
    }

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let bucket_name = if let Some(job_id) = &request.job_id {
        sanitize_bucket_name(job_id)
    } else {
        format!(
            "arxiv-{}-{}",
            timestamp,
            Uuid::new_v4()
                .to_string()
                .chars()
                .take(8)
                .collect::<String>()
        )
    };

    let mut results = Vec::new();

    // Temp dir usage
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

    let output_path_str = if let Some(ref d) = request.output_dir {
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

    if let Err(e) = tokio::fs::create_dir_all(&output_path_str).await {
        tracing::warn!(
            "Failed to create arxiv output directory {}: {}",
            output_path_str,
            e
        );
    }

    for entry in feed.entries {
        let title = entry.title.map(|t| t.content).unwrap_or_default();
        let authors: Vec<String> = entry.authors.into_iter().map(|p| p.name).collect();
        let published = entry.published.map(|t| t.to_rfc3339()).unwrap_or_default();

        // Find PDF link
        // ArXiv atom links: rel="alternate" type="text/html", rel="related" type="application/pdf" ... checking Logic
        // Usually find link with type="application/pdf"
        // feed-rs links
        let pdf_link = entry
            .links
            .iter()
            .find(|l| {
                l.media_type.as_deref() == Some("application/pdf")
                    || l.title.as_deref() == Some("pdf")
            })
            .map(|l| l.href.clone());

        // Fallback: ArXiv sometimes puts PDF link in id or we construct it.
        // Let's assume we find it or skip.

        if let Some(pdf_url) = pdf_link {
            // Basic ID from URL?
            let id_part = pdf_url.split('/').next_back().unwrap_or("unknown");
            let filename = format!("{}.pdf", id_part);
            let filepath = Path::new(&output_path_str).join(&filename);

            let mut result_entry = ArxivResult {
                title,
                authors,
                published,
                pdf_url: pdf_url.clone(),
                local_path: None,
                s3_path: None,
                s3_bucket: bucket_name.clone(),
                error: None,
            };

            // Download PDF
            match client.get(&pdf_url).send().await {
                Ok(pdf_resp) => {
                    if pdf_resp.status().is_success() {
                        if let Ok(bytes) = pdf_resp.bytes().await {
                            if tokio::fs::write(&filepath, &bytes).await.is_ok() {
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
                            }
                        }
                    } else {
                        result_entry.error =
                            Some(format!("PDF download status {}", pdf_resp.status()));
                    }
                }
                Err(e) => {
                    result_entry.error = Some(format!("PDF download error: {}", e));
                }
            }
            results.push(result_entry);
        }
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

    // Temp dir is dropped automatically here if it exists, implementing cleanup.

    // Save search_results.json to RustFS (source: ArXiv API)
    let title = format!("ArXiv: {}", request.keywords);
    let created_at = Utc::now().to_rfc3339();

    let result_json_content = serde_json::to_string_pretty(&json!({
        "job_type": "arxiv",
        "source": "ArXiv API",
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
            "Failed to save arxiv results to S3 bucket {}: {}",
            bucket_name,
            e
        );
    }

    Ok(Json(json!({"data": results})))
}
