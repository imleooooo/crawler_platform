use crate::config::{CrawlResult, CrawlerRunConfig};
use crate::errors::CrawlError;
use reqwest::Client;

pub struct HttpCrawler {
    client: Client,
}

impl Default for HttpCrawler {
    fn default() -> Self {
        Self::new()
    }
}

/// Maximum time to wait for a TCP connection to be established.
const CONNECT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);
/// Maximum end-to-end time for a single HTTP request (connect + send + response headers + body).
const REQUEST_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(60);

impl HttpCrawler {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                // Default UA to look less suspicious, though headers can be overridden
                .user_agent(
                    "Mozilla/5.0 (compatible; LabCrawl/1.0; +https://github.com/example/lab-crawl)",
                )
                .connect_timeout(CONNECT_TIMEOUT)
                .timeout(REQUEST_TIMEOUT)
                .build()
                .expect("Failed to build HTTP client"),
        }
    }

    pub async fn crawl(&self, config: CrawlerRunConfig) -> Result<CrawlResult, CrawlError> {
        let resp = self
            .client
            .get(&config.url)
            .send()
            .await
            .map_err(|e| CrawlError::Other(format!("HTTP Request failed: {}", e)))?;

        let status = resp.status().as_u16();
        let url = resp.url().as_str().to_string();

        // Handling non-200 might be policy dependent, but for now we return what we get unless it's a network error (already handled above)
        // However, if we want to mimic browser behavior which might "fail" on 404/500 depending on definition,
        // usually browser just shows the page. We will do the same: get text.

        let html = resp
            .text()
            .await
            .map_err(|e| CrawlError::Other(format!("Failed to get text: {}", e)))?;

        Ok(CrawlResult {
            url,
            html,
            markdown: None,
            screenshot: None, // No screenshot in http mode
            status_code: status,
            success: (200..400).contains(&status), // Basic success definition
            error_message: if status >= 400 {
                Some(format!("HTTP Error {}", status))
            } else {
                None
            },
        })
    }
}
