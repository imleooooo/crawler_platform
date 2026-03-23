use crate::services::queue::QueueService;
use aws_sdk_s3::Client as S3Client;
use std::collections::{HashMap, VecDeque};
use std::sync::{atomic::AtomicBool, Arc, Mutex, MutexGuard};
use std::time::Instant;

/// Acquire the metrics lock regardless of poison state.
///
/// A poisoned mutex means a thread panicked while holding it. The data inside
/// (`MetricsState`) is plain counters with no invariants that a panic could
/// break, so recovering the guard is safe. Silently skipping the update (the
/// previous `if let Ok` pattern) would freeze metrics permanently instead.
pub fn lock_metrics(m: &Mutex<MetricsState>) -> MutexGuard<'_, MetricsState> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[derive(Clone)]
pub struct AppState {
    pub s3_client: S3Client,
    pub s3_client_public: S3Client,
    pub metrics: Arc<Mutex<MetricsState>>,
    pub queue_service: QueueService,
    pub google_api_key: String,
    pub google_cx: String,
    pub api_key: String,
    pub openai_api_key: String,
    /// Guards the check-then-enqueue path against the shutdown race.
    /// true = open (enqueuing allowed); false = closed (return 503).
    /// Shutdown stores false (Release) before signalling the worker. Any
    /// request that already loaded true before the store may still enqueue
    /// one task; the worst-case leak is bounded by the concurrency limiter.
    pub enqueue_gate: Arc<AtomicBool>,
    /// Per-domain politeness throttle.  Maps hostname → earliest time the next
    /// request to that domain may be sent (1 s between requests).
    pub domain_throttle: Arc<Mutex<HashMap<String, Instant>>>,
}

pub struct MetricsState {
    pub active_workers: usize,
    pub queue_size: usize,
    pub request_history: VecDeque<f64>,
}

impl MetricsState {
    pub fn new() -> Self {
        Self {
            active_workers: 0,
            queue_size: 0,
            request_history: VecDeque::with_capacity(50),
        }
    }
}
