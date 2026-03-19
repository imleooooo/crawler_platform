use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserConfig {
    pub headless: bool,
    pub user_agent: Option<String>,
    pub viewport_width: u32,
    pub viewport_height: u32,
    pub verbose: bool,
    pub disable_images: bool,
    pub disable_css: bool,
    pub rotate_user_agent: bool,
    pub semaphore_size: Option<usize>,
}

impl Default for BrowserConfig {
    fn default() -> Self {
        Self {
            headless: true,
            user_agent: Some("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36".to_string()),
            viewport_width: 1080,
            viewport_height: 600,
            verbose: true,
            disable_images: false,
            disable_css: false,
            rotate_user_agent: false,
            semaphore_size: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CrawlerRunConfig {
    pub url: String,
    pub screenshot: bool,
    pub wait_for: Option<String>,
    pub css_selector: Option<String>,
    pub js_code: Option<String>,
    pub word_count_threshold: Option<usize>,
    pub cache_mode: bool, // Simplified for now
    pub magic_markdown: bool,
    pub run_mode: Option<String>,
    pub api_key: Option<String>, // For Agent
    pub model: Option<String>,   // For Agent
    pub prompt: Option<String>,  // For Agent
    pub ignore_links: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrawlResult {
    pub url: String,
    pub html: String,
    pub markdown: Option<String>,
    pub screenshot: Option<String>, // Base64 encoded
    pub status_code: u16,
    pub success: bool,
    pub error_message: Option<String>,
}
