#![allow(clippy::collapsible_if)]
use aws_config::BehaviorVersion;
use axum::{
    http::HeaderValue,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

mod api;
mod config;
mod middleware;
mod services;
mod state;
mod worker;

use crate::config::AppConfig;
use crate::services::queue::QueueService;
use deadpool_redis::{Config, Runtime};
use state::{AppState, MetricsState};

#[tokio::main]
async fn main() {
    dotenv().ok();

    // Structured JSON logging, level controlled by RUST_LOG (default: info)
    tracing_subscriber::fmt()
        .json()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .init();

    // Validate all required environment variables at startup — fail fast if any are missing.
    let cfg = AppConfig::from_env().unwrap_or_else(|e| {
        eprintln!("FATAL: {}", e);
        std::process::exit(1);
    });

    // Initialize S3 Client
    let region = aws_config::Region::new("us-east-1");
    let credentials = aws_sdk_s3::config::Credentials::new(
        &cfg.rustfs_access_key,
        &cfg.rustfs_secret_key,
        None,
        None,
        "env",
    );

    // Load default SDK config (includes HTTP connector)
    let sdk_config = aws_config::load_defaults(BehaviorVersion::latest()).await;

    // Build S3-specific config overriding endpoint and credentials
    let s3_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .region(region.clone())
        .endpoint_url(&cfg.rustfs_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    let s3_public_config = aws_sdk_s3::config::Builder::from(&sdk_config)
        .region(region.clone())
        .endpoint_url(&cfg.rustfs_public_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client_public = aws_sdk_s3::Client::from_conf(s3_public_config);

    // Initialize Redis Pool
    let redis_cfg = Config::from_url(&cfg.redis_url);
    let redis_pool = redis_cfg
        .create_pool(Some(Runtime::Tokio1))
        .unwrap_or_else(|e| {
            eprintln!("FATAL: Failed to create Redis pool: {}", e);
            std::process::exit(1);
        });
    let queue_service = QueueService::new(redis_pool);

    let state = AppState {
        s3_client,
        s3_client_public,
        metrics: Arc::new(Mutex::new(MetricsState::new())),
        queue_service,
        google_api_key: cfg.google_api_key,
        google_cx: cfg.google_cx,
        api_key: cfg.api_key,
        openai_api_key: cfg.openai_api_key,
    };

    // Spawn Worker with a shutdown channel so main can coordinate clean exit
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let worker_state = state.clone();
    let worker_handle = tokio::spawn(async move {
        worker::run_worker(worker_state, shutdown_rx).await;
    });

    // CORS: restrict to explicitly configured origins only
    let allowed_origins: Vec<HeaderValue> = cfg
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse::<HeaderValue>().ok())
        .collect();

    let cors = CorsLayer::new()
        .allow_origin(AllowOrigin::list(allowed_origins))
        .allow_methods([
            axum::http::Method::GET,
            axum::http::Method::POST,
            axum::http::Method::OPTIONS,
        ])
        .allow_headers([
            axum::http::header::CONTENT_TYPE,
            axum::http::header::AUTHORIZATION,
        ]);

    let protected = Router::new()
        .route("/api/metrics", get(api::general::get_metrics))
        .route("/api/search-aggregate", post(api::search::search_aggregate))
        .route("/api/arxiv-search", post(api::arxiv::arxiv_search))
        .route("/api/podcast-search", post(api::podcast::podcast_search))
        .route("/api/ai-exploration", post(api::exploration::ai_exploration))
        .route("/api/agent-crawl", post(api::crawl::agent_crawl))
        .route("/api/batch-crawl", post(api::crawl::batch_crawl))
        .route("/api/storage-stats", get(api::storage::storage_stats))
        .route("/api/storage/delete", post(api::storage::delete_task_data))
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            middleware::auth::require_api_key,
        ));

    let app = Router::new()
        .route("/", get(api::general::root))
        .route("/healthz", get(api::general::healthz))
        .merge(protected)
        .layer(cors)
        .layer(TraceLayer::new_for_http())
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state);

    let port: u16 = match std::env::var("PORT") {
        Ok(v) => v.trim().parse().unwrap_or_else(|_| {
            eprintln!("FATAL: PORT env var '{}' is not a valid port number", v);
            std::process::exit(1);
        }),
        Err(_) => 8000,
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        eprintln!("FATAL: Failed to bind {}: {}", addr, e);
        std::process::exit(1);
    });
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap_or_else(|e| {
            eprintln!("FATAL: Server error: {}", e);
            std::process::exit(1);
        });

    // HTTP connections are fully drained — now signal the worker to stop
    // and wait for any in-progress task to complete before exiting.
    let _ = shutdown_tx.send(true);
    if let Err(e) = worker_handle.await {
        tracing::error!("Worker task panicked: {:?}", e);
    }
}

async fn shutdown_signal() {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    tracing::info!("shutdown signal received, draining in-flight requests");
}
