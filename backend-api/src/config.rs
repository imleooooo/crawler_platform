pub struct AppConfig {
    pub rustfs_endpoint: String,
    pub rustfs_access_key: String,
    pub rustfs_secret_key: String,
    pub rustfs_public_endpoint: String,
    pub redis_url: String,
    pub google_api_key: String,
    pub google_cx: String,
    /// Comma-separated list of allowed CORS origins, e.g. "https://app.example.com,https://admin.example.com"
    pub allowed_origins: Vec<String>,
}

impl AppConfig {
    pub fn from_env() -> Result<Self, String> {
        let mut errors = Vec::new();

        macro_rules! require_env {
            ($var:expr) => {
                std::env::var($var).map_err(|_| errors.push(format!("Missing required env var: {}", $var))).ok()
            };
        }

        let rustfs_endpoint = require_env!("RUSTFS_ENDPOINT");
        let rustfs_access_key = require_env!("RUSTFS_ACCESS_KEY");
        let rustfs_secret_key = require_env!("RUSTFS_SECRET_KEY");
        let rustfs_public_endpoint = require_env!("RUSTFS_PUBLIC_ENDPOINT");
        let redis_url = require_env!("REDIS_URL");
        let google_api_key = require_env!("GOOGLE_API_KEY");
        let google_cx = require_env!("GOOGLE_CX");

        let allowed_origins_raw = std::env::var("ALLOWED_ORIGINS").unwrap_or_default();
        let allowed_origins: Vec<String> = allowed_origins_raw
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect();

        if !errors.is_empty() {
            return Err(format!("Configuration errors:\n  - {}", errors.join("\n  - ")));
        }

        if allowed_origins.is_empty() {
            return Err("ALLOWED_ORIGINS must be set to a comma-separated list of allowed origins (e.g. https://app.example.com)".to_string());
        }

        Ok(Self {
            rustfs_endpoint: rustfs_endpoint.unwrap(),
            rustfs_access_key: rustfs_access_key.unwrap(),
            rustfs_secret_key: rustfs_secret_key.unwrap(),
            rustfs_public_endpoint: rustfs_public_endpoint.unwrap(),
            redis_url: redis_url.unwrap(),
            google_api_key: google_api_key.unwrap(),
            google_cx: google_cx.unwrap(),
            allowed_origins,
        })
    }
}
