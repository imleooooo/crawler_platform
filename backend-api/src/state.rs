use crate::services::queue::QueueService;
use aws_sdk_s3::Client as S3Client;
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

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
