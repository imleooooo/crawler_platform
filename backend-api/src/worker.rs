use crate::services::{crawler, s3};
use crate::state::AppState;
use serde_json::json;
use std::time::SystemTime;
use uuid::Uuid;

pub async fn run_worker(state: AppState) {
    tracing::info!("Worker started");
    loop {
        // Dequeue with 5 second timeout
        match state.queue_service.dequeue(5.0).await {
            Ok(Some(task)) => {
                tracing::info!("Worker received task with {} URLs", task.urls.len());

                // Update metrics (optional / simplified)
                {
                    if let Ok(mut metrics) = state.metrics.lock() {
                        metrics.active_workers += 1;
                    }
                }

                // Execute Crawl
                let result = crawler::call_crawler_service(&task).await;

                // Metrics cleanup
                {
                    if let Ok(mut metrics) = state.metrics.lock() {
                        if metrics.active_workers > 0 {
                            metrics.active_workers -= 1;
                        }
                    }
                }

                match result {
                    Ok(resp) => {
                        // Save Results logic
                        let timestamp = SystemTime::now()
                            .duration_since(SystemTime::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();

                        let bucket_name = if let Some(name) = &task.bucket_name {
                            name.clone()
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
                        tracing::info!("Saving results to bucket: {}", bucket_name);

                        let mut processed_results = Vec::new();

                        for (i, item) in resp.results.iter().enumerate() {
                            let mut item_val = serde_json::to_value(item).unwrap();
                            if let Some(obj) = item_val.as_object_mut() {
                                obj.insert("s3_bucket".to_string(), json!(bucket_name));

                                if item.success {
                                    if let Some(md) = &item.markdown {
                                        let url_hash =
                                            format!("{:x}", md5::compute(item.url.as_bytes()));
                                        let s3_key = format!("crawl_{}_{}.md", i, &url_hash[..8]);

                                        if let Ok(path) = s3::save_to_rustfs_content(
                                            &state.s3_client,
                                            &bucket_name,
                                            &s3_key,
                                            md,
                                        )
                                        .await
                                        {
                                            obj.insert("s3_path".to_string(), json!(path));
                                        }
                                    }
                                }
                            }
                            processed_results.push(item_val);
                        }

                        // Save summary
                        let json_content = serde_json::to_string_pretty(&json!({
                            "original_request": task,
                            "results": processed_results
                        }))
                        .unwrap_or_default();

                        let _ = s3::save_to_rustfs_content(
                            &state.s3_client,
                            &bucket_name,
                            "summary.json",
                            &json_content,
                        )
                        .await;

                        // Local Output (if requested)
                        if let Some(dir) = &task.output_dir {
                            let _ = tokio::fs::create_dir_all(dir).await;
                            let filename = format!("batch_crawl_results_{}.json", timestamp);
                            let filepath = std::path::Path::new(dir).join(filename);
                            let _ = tokio::fs::write(&filepath, &json_content).await;
                            tracing::info!("Saved local copy to {}", filepath.display());
                        }

                        tracing::info!("Task completed and saved.");
                    }
                    Err(e) => {
                        tracing::error!("Task execution failed: {}", e);
                    }
                }
            }
            Ok(None) => {
                // Timeout, nothing to do
            }
            Err(e) => {
                tracing::error!("Worker queue error: {}", e);
                tokio::time::sleep(tokio::time::Duration::from_secs(5)).await;
            }
        }
    }
}
