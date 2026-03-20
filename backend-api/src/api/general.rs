use crate::state::AppState;
use axum::{extract::State, http::StatusCode, Json};
use serde_json::{json, Value};

pub async fn root() -> Json<Value> {
    Json(json!({"Hello": "World from Backend API (Rust)"}))
}

pub async fn healthz() -> (StatusCode, Json<Value>) {
    (StatusCode::OK, Json(json!({"status": "ok"})))
}

pub async fn get_metrics(State(state): State<AppState>) -> Json<Value> {
    if let Ok(metrics) = state.metrics.lock() {
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
    } else {
        tracing::error!("Metrics lock poisoned");
        Json(json!({
            "queue_size": 0,
            "active_workers": 0,
            "avg_latency": 0.0
        }))
    }
}
