use axum::{extract::State, Json};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::path::Path;
use std::time::SystemTime;
use uuid::Uuid;

use crate::services::{s3, sanitize_bucket_name};
use crate::state::AppState;

#[derive(Deserialize)]
pub struct ArxivRequest {
    pub keywords: String,
    pub year: Option<String>,
    #[serde(default = "default_limit")]
    pub limit: i32,
    pub output_dir: Option<String>,
    pub job_id: Option<String>,
}

fn default_limit() -> i32 {
    5
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

pub async fn arxiv_search(
    State(state): State<AppState>,
    Json(request): Json<ArxivRequest>,
) -> Result<Json<Value>, (axum::http::StatusCode, String)> {
    // Metrics
    {
        let mut metrics = state.metrics.lock().unwrap();
        metrics.queue_size += 1;
        metrics.active_workers += 1;
    }

    // Logic
    let mut query = format!("all:{}", request.keywords);
    if let Some(year) = &request.year {
        if !year.trim().is_empty() {
            query.push_str(&format!(
                " AND submittedDate:[{}01010000 TO {}12312359]",
                year.trim(),
                year.trim()
            ));
        }
    }

    let url = "http://export.arxiv.org/api/query";
    let client = reqwest::Client::new();

    let resp = client
        .get(url)
        .query(&[
            ("search_query", &query),
            ("start", &"0".to_string()),
            ("max_results", &request.limit.to_string()),
            ("sortBy", &"submittedDate".to_string()),
            ("sortOrder", &"descending".to_string()),
        ])
        .send()
        .await
        .map_err(|e| {
            (
                axum::http::StatusCode::INTERNAL_SERVER_ERROR,
                format!("ArXiv API failed: {}", e),
            )
        })?;

    if !resp.status().is_success() {
        return Err((
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("ArXiv API returned {}", resp.status()),
        ));
    }

    let xml_content = resp.bytes().await.map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to read XML: {}", e),
        )
    })?;

    // Parse using feed-rs
    let feed = feed_rs::parser::parse(xml_content.as_ref()).map_err(|e| {
        (
            axum::http::StatusCode::INTERNAL_SERVER_ERROR,
            format!("Failed to parse Atom: {}", e),
        )
    })?;

    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
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
            .unwrap()
            .path()
            .to_string_lossy()
            .to_string()
    };

    let _ = tokio::fs::create_dir_all(&output_path_str).await;

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
        let mut metrics = state.metrics.lock().unwrap();
        if metrics.queue_size > 0 {
            metrics.queue_size -= 1;
        }
        if metrics.active_workers > 0 {
            metrics.active_workers -= 1;
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
