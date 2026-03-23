#![allow(clippy::collapsible_if)]
use axum::{
    http::HeaderValue,
    routing::{get, post},
    Router,
};
use dotenvy::dotenv;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tower::limit::ConcurrencyLimitLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::{DefaultOnRequest, DefaultOnResponse, TraceLayer};
use tracing::Level;

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
    // Build the config directly without aws_config::load_defaults to avoid:
    //   1. IMDS probing (3-second delay on non-EC2 hosts)
    //   2. IMDS-based credential/region probing incorrectly influencing the client
    let region = aws_sdk_s3::config::Region::new("us-east-1");
    let credentials = aws_sdk_s3::config::Credentials::new(
        &cfg.rustfs_access_key,
        &cfg.rustfs_secret_key,
        None,
        None,
        "env",
    );

    let s3_config = aws_sdk_s3::config::Builder::new()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(region.clone())
        .endpoint_url(&cfg.rustfs_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client = aws_sdk_s3::Client::from_conf(s3_config);

    let s3_public_config = aws_sdk_s3::config::Builder::new()
        .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
        .region(region.clone())
        .endpoint_url(&cfg.rustfs_public_endpoint)
        .credentials_provider(aws_sdk_s3::config::SharedCredentialsProvider::new(
            credentials.clone(),
        ))
        .force_path_style(true)
        .build();

    let s3_client_public = aws_sdk_s3::Client::from_conf(s3_public_config);

    // Initialize Redis Pool with explicit sizing and timeouts
    let mut redis_cfg = Config::from_url(&cfg.redis_url);
    redis_cfg.pool = Some(deadpool_redis::PoolConfig {
        max_size: 20,
        timeouts: deadpool_redis::Timeouts {
            wait: Some(std::time::Duration::from_secs(5)),
            create: Some(std::time::Duration::from_secs(5)),
            recycle: Some(std::time::Duration::from_secs(5)),
        },
        queue_mode: Default::default(),
    });
    let redis_pool = redis_cfg
        .create_pool(Some(Runtime::Tokio1))
        .unwrap_or_else(|e| {
            eprintln!("FATAL: Failed to create Redis pool: {}", e);
            std::process::exit(1);
        });
    let queue_service = QueueService::new(redis_pool);

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    // enqueue_gate: true = open. Shutdown stores false (Release ordering)
    // before signalling the worker. Non-blocking; no mutex needed.
    let enqueue_gate = Arc::new(AtomicBool::new(true));

    let state = AppState {
        s3_client,
        s3_client_public,
        metrics: Arc::new(Mutex::new(MetricsState::new())),
        queue_service,
        google_api_key: cfg.google_api_key,
        google_cx: cfg.google_cx,
        api_key: cfg.api_key,
        openai_api_key: cfg.openai_api_key,
        enqueue_gate: enqueue_gate.clone(),
        domain_throttle: Arc::new(Mutex::new(std::collections::HashMap::new())),
    };

    // false = idle (initial), true = actively processing a crawl job.
    // main() watches this to avoid force-killing an in-progress task during shutdown.
    let (worker_busy_tx, worker_busy_rx) = tokio::sync::watch::channel(false);

    let worker_state = state.clone();
    let worker_handle = tokio::spawn(async move {
        worker::run_worker(worker_state, shutdown_rx, worker_busy_tx).await;
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
            axum::http::HeaderName::from_static("x-request-id"),
        ])
        .expose_headers([axum::http::HeaderName::from_static("x-request-id")]);

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
        ))
        // Concurrency cap on authenticated routes only: excess requests get 503 immediately
        // without queuing. /healthz and / are excluded so health checks never stall.
        .layer(ConcurrencyLimitLayer::new(200));

    let app = Router::new()
        .route("/", get(api::general::root))
        .route("/healthz", get(api::general::healthz))
        .merge(protected)
        .layer(cors)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(|request: &axum::http::Request<axum::body::Body>| {
                    let request_id = request
                        .headers()
                        .get("x-request-id")
                        .and_then(|v| v.to_str().ok())
                        .unwrap_or("-");
                    tracing::span!(
                        Level::INFO,
                        "request",
                        request_id = request_id,
                        method = %request.method(),
                        uri = %request.uri(),
                    )
                })
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        )
        .layer(PropagateRequestIdLayer::x_request_id())
        .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid))
        .with_state(state);

    let port: u16 = match std::env::var("PORT") {
        Ok(v) => v.trim().parse().unwrap_or_else(|_| {
            eprintln!("FATAL: PORT env var '{}' is not a valid port number", v);
            std::process::exit(1);
        }),
        Err(std::env::VarError::NotPresent) => 8000,
        Err(std::env::VarError::NotUnicode(_)) => {
            eprintln!("FATAL: PORT env var contains invalid Unicode");
            std::process::exit(1);
        }
    };
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    tracing::info!("listening on {}", addr);
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap_or_else(|e| {
        eprintln!("FATAL: Failed to bind {}: {}", addr, e);
        std::process::exit(1);
    });
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(shutdown_tx, enqueue_gate))
        .await
        .unwrap_or_else(|e| {
            eprintln!("FATAL: Server error: {}", e);
            std::process::exit(1);
        });

    // Two-phase worker drain:
    //
    // Phase 1 — wait for any in-progress crawl to finish (no hard cap).
    //   The job was already dequeued from Redis so aborting it here would
    //   silently lose it. Crawl-level timeouts (HTTP 60s, browser 30s) bound
    //   how long this can take in practice.
    //
    // Phase 2 — once the worker is idle it will pick up the shutdown signal on
    //   its next loop iteration (within one 5s dequeue poll). Give a short fixed
    //   window before giving up and letting the runtime drop the task.
    let mut busy_rx = worker_busy_rx;
    if *busy_rx.borrow() {
        tracing::info!("Waiting for in-progress crawl job to complete before shutdown");
        loop {
            if busy_rx.changed().await.is_err() {
                break; // sender dropped — worker exited or panicked
            }
            if !*busy_rx.borrow() {
                break; // worker became idle
            }
        }
    }

    const IDLE_EXIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
    match tokio::time::timeout(IDLE_EXIT_TIMEOUT, worker_handle).await {
        Ok(Ok(())) => {}
        Ok(Err(e)) => tracing::error!("Worker task panicked: {:?}", e),
        Err(_) => tracing::error!(
            "Worker did not exit within {}s after becoming idle; forcing shutdown",
            IDLE_EXIT_TIMEOUT.as_secs()
        ),
    }
}

async fn shutdown_signal(
    shutdown_tx: tokio::sync::watch::Sender<bool>,
    enqueue_gate: Arc<AtomicBool>,
) {
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

    // Close the gate (non-blocking) then signal the worker. Requests that
    // already loaded true before this store may still enqueue one task each;
    // worst-case leakage is bounded by the concurrency limiter (200).
    enqueue_gate.store(false, Ordering::Release);
    let _ = shutdown_tx.send(true);
    tracing::info!("shutdown signal received, draining in-flight requests");
}
