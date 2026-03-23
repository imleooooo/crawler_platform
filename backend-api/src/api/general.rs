use crate::state::{lock_metrics, AppState};
use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

pub async fn root() -> Json<Value> {
    Json(json!({"Hello": "World from Backend API (Rust)"}))
}

pub async fn healthz(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let timeout = std::time::Duration::from_secs(3);

    let redis_result = tokio::time::timeout(timeout, state.queue_service.ping()).await;
    let redis_ok = matches!(redis_result, Ok(Ok(())));

    let s3_result =
        tokio::time::timeout(timeout, state.s3_client.list_buckets().send()).await;
    let s3_ok = matches!(s3_result, Ok(Ok(_)));

    let all_ok = redis_ok && s3_ok;
    let http_status = if all_ok {
        StatusCode::OK
    } else {
        StatusCode::SERVICE_UNAVAILABLE
    };

    (
        http_status,
        Json(json!({
            "status": if all_ok { "ok" } else { "degraded" },
            "dependencies": {
                "redis": if redis_ok { "ok" } else { "error" },
                "s3": if s3_ok { "ok" } else { "error" },
            }
        })),
    )
}

pub async fn get_metrics(State(state): State<AppState>) -> Json<Value> {
    let metrics = lock_metrics(&state.metrics);
    let history = &metrics.request_history;
    let avg_latency = if history.is_empty() {
        0.0
    } else {
        history.iter().sum::<f64>() / history.len() as f64
    };

    Json(json!({
        "queue_size": metrics.queue_size,
        "active_workers": metrics.active_workers,
        "avg_latency": (avg_latency * 100.0).round() / 100.0
    }))
}
