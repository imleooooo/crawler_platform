#![allow(clippy::collapsible_if)]
use aws_config::BehaviorVersion;
use axum::{
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::{Any, CorsLayer};
use tower_http::trace::TraceLayer;

mod api;
mod services;
mod state;
mod worker;

use crate::services::queue::QueueService;
use deadpool_redis::{Config, Runtime};
use state::{AppState, MetricsState};

#[tokio::main]
async fn main() {
    dotenv().ok();
    tracing_subscriber::fmt::init();

    // Initialize S3 Client
    let rustfs_endpoint =
        std::env::var("RUSTFS_ENDPOINT").unwrap_or_else(|_| "http://localhost:9000".to_string());
    let access_key =
        std::env::var("RUSTFS_ACCESS_KEY").unwrap_or_else(|_| "minioadmin".to_string());
    let secret_key =
        std::env::var("RUSTFS_SECRET_KEY").unwrap_or_else(|_| "minioadmin".to_string());

    let region = aws_config::Region::new("us-east-1");
    let credentials =
        aws_sdk_s3::config::Credentials::new(access_key, secret_key, None, None, "env");

    // Load default SDK config (includes HTTP connector)
    let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    // Build S3-specific config overriding endpoint and credentials
    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .region(region.clone())
        .endpoint_url(rustfs_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    let rustfs_public_endpoint = std::env::var("RUSTFS_PUBLIC_ENDPOINT")
        .unwrap_or_else(|_| "http://localhost:9000".to_string());

    let s3_public_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .region(region.clone())
        .endpoint_url(rustfs_public_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client_public = aws_sdk_s3::Client::from_conf(s3_public_config);

    // Initialize Redis Pool
    let redis_url =
        std::env::var("REDIS_URL").unwrap_or_else(|_| "redis://127.0.0.1:6379".to_string());
    let cfg = Config::from_url(redis_url);
    let redis_pool = cfg
        .create_pool(Some(Runtime::Tokio1))
        .expect("Failed to create Redis pool");
    let queue_service = QueueService::new(redis_pool);

    let state = AppState {
        s3_client,
        s3_client_public,
        metrics: Arc::new(Mutex::new(MetricsState::new())),
        queue_service,
    };

    // Spawn Worker
    let worker_state = state.clone();
    tokio::spawn(async move {
        worker::run_worker(worker_state).await;
    });
    let cors = CorsLayer::new()
        .allow_origin(Any) // For dev only. In prod specific origins.
        .allow_methods(Any)
        .allow_headers(Any);

    let app = Router::new()
        .route("/", get(api::general::root))
        .route("/api/metrics", get(api::general::get_metrics))
        .route("/api/search-aggregate", post(api::search::search_aggregate))
        .route("/api/arxiv-search", post(api::arxiv::arxiv_search))
        .route("/api/podcast-search", post(api::podcast::podcast_search))
        .route(
            "/api/ai-exploration",
            post(api::exploration::ai_exploration),
        )
        .route("/api/agent-crawl", post(api::crawl::agent_crawl))
        .route("/api/batch-crawl", post(api::crawl::batch_crawl))
        .route("/api/storage-stats", get(api::storage::storage_stats))
        .route("/api/storage/delete", post(api::storage::delete_task_data))
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr = SocketAddr::from(([0, 0, 0, 0], 8000));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
