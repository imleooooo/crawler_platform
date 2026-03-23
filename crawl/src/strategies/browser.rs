use crate::config::{BrowserConfig, CrawlResult, CrawlerRunConfig};
use crate::errors::CrawlError;

/// Hard deadline for page.goto() — covers TCP connect + TLS + server response start.
const NAVIGATION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
/// Hard deadline for wait_for_navigation() / find_element() after the initial load.
const WAIT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);
use crate::strategies::stealth::{apply_stealth, StealthConfig};
use chromiumoxide::browser::BrowserConfig as CBrowserConfig;
use chromiumoxide::handler::viewport::Viewport;
use chromiumoxide::{Browser, Page};
use futures::StreamExt;

use base64::{engine::general_purpose, Engine as _};
use chromiumoxide::cdp::browser_protocol::network::SetUserAgentOverrideParams;
use rand::seq::SliceRandom;
use rand::thread_rng;
use std::sync::Arc;
use tokio::sync::{Mutex, OwnedSemaphorePermit, Semaphore};

pub struct BrowserManager {
    browser: Browser,
    handle: tokio::task::JoinHandle<()>,
    _permit: Option<OwnedSemaphorePermit>,
    config: BrowserConfig,
}

impl BrowserManager {
    pub async fn new(config: &BrowserConfig) -> Result<Self, CrawlError> {
        let mut builder = CBrowserConfig::builder()
            .viewport(Viewport {
                width: config.viewport_width,
                height: config.viewport_height,
                ..Default::default()
            })
            .arg("--disable-blink-features=AutomationControlled")
            .arg("--disable-infobars")
            .arg("--exclude-switches=enable-automation")
            .arg("--no-sandbox")
            .arg("--disable-setuid-sandbox")
            .arg("--disable-dev-shm-usage")
            .arg("--disable-accelerated-2d-canvas")
            .arg("--disable-gpu");

        // Set unique user data directory for each instance to avoid singleton lock conflicts
        let user_data_dir =
            std::env::temp_dir().join(format!("crawl4ai_profile_{}", uuid::Uuid::new_v4()));
        builder = builder.user_data_dir(user_data_dir);

        if config.disable_images {
            builder = builder.arg("--blink-settings=imagesEnabled=false");
        }

        if config.headless {
            // Use new headless mode for better stealth
            builder = builder
                .with_head() // Disable default old headless
                .arg("--headless=new");
        } else {
            builder = builder.with_head();
        }

        if let Some(ua) = &config.user_agent {
            builder = builder.args(vec![format!("--user-agent={}", ua)]);
        }

        let build_config = builder
            .build()
            .map_err(|e| CrawlError::BrowserLaunchError(e.to_string()))?;

        let (browser, mut handler) = Browser::launch(build_config)
            .await
            .map_err(|e| CrawlError::BrowserLaunchError(e.to_string()))?;

        // Spawn the handler thread
        let handle = tokio::spawn(async move { while (handler.next().await).is_some() {} });

        Ok(Self {
            browser,
            handle,
            _permit: None,
            config: config.clone(),
        })
    }

    pub async fn create_page(&self, _config: &BrowserConfig) -> Result<Page, CrawlError> {
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| CrawlError::BrowserLaunchError(e.to_string()))?;

        // Apply stealth scripts to new page
        apply_stealth(&page, &StealthConfig::default()).await?;

        let _ = self.apply_resource_blocking(&page, _config).await;

        Ok(page)
    }

    async fn apply_resource_blocking(
        &self,
        page: &Page,
        config: &BrowserConfig,
    ) -> Result<(), CrawlError> {
        if config.disable_css || config.disable_images {
            let mut methods = vec![];
            if config.disable_images {
                methods.push("*.png".to_string());
                methods.push("*.jpg".to_string());
                methods.push("*.jpeg".to_string());
                methods.push("*.gif".to_string());
                methods.push("*.svg".to_string());
                methods.push("*.webp".to_string());
            }
            if config.disable_css {
                methods.push("*.css".to_string());
            }

            // Use CDP to block URLs
            if !methods.is_empty() {
                page.execute(
                    chromiumoxide::cdp::browser_protocol::network::SetBlockedUrLsParams {
                        urls: methods,
                    },
                )
                .await
                .map_err(|e| CrawlError::Other(e.to_string()))?;
            }
        }
        Ok(())
    }

    pub async fn get_current_page(&self) -> Result<Page, CrawlError> {
        // Return a handle to a new blank page or the existing one?
        // Ideally we should reuse the page created during crawl() but crawl() consumes the manager logic.
        // For Agent, we acquire Manager, but haven't called crawl() yet.
        // So we need to create a new page if one doesn't exist, or just create one now.
        // Since we are likely in a fresh Manager from acquire(), let's create a fresh page.
        self.create_page(&self.config).await
    }

    pub async fn crawl(&self, run_config: CrawlerRunConfig) -> Result<CrawlResult, CrawlError> {
        let page = self
            .browser
            .new_page("about:blank")
            .await
            .map_err(|e| CrawlError::NavigationError(e.to_string()))?;

        // All navigation logic runs inside an async block so that page.close() below
        // is guaranteed to execute even when a timeout fires and `?` returns early.
        let result: Result<CrawlResult, CrawlError> = async {
            // Apply stealth
            apply_stealth(&page, &StealthConfig::default()).await?;

            // Apply resource blocking and UA rotation
            self.apply_resource_blocking(&page, &self.config).await?;

            if self.config.rotate_user_agent {
                let mut rng = thread_rng();
                if let Some(ua) = crate::user_agents::USER_AGENTS.choose(&mut rng) {
                    page.execute(SetUserAgentOverrideParams::new(ua.to_string()))
                        .await
                        .map_err(|e| CrawlError::Other(e.to_string()))?;
                }
            }

            tokio::time::timeout(NAVIGATION_TIMEOUT, page.goto(&run_config.url))
                .await
                .map_err(|_| CrawlError::NavigationError("goto timed out (30s)".to_string()))?
                .map_err(|e| CrawlError::NavigationError(e.to_string()))?;

            // Wait for element if specified
            if let Some(wait_for) = &run_config.wait_for {
                tokio::time::timeout(WAIT_TIMEOUT, page.find_element(wait_for.as_str()))
                    .await
                    .map_err(|_| {
                        CrawlError::ElementNotFound("find_element timed out (30s)".to_string())
                    })?
                    .map_err(|e| CrawlError::ElementNotFound(e.to_string()))?;
            } else {
                // Default wait (network idle or load)
                tokio::time::timeout(WAIT_TIMEOUT, page.wait_for_navigation())
                    .await
                    .map_err(|_| {
                        CrawlError::NavigationError(
                            "wait_for_navigation timed out (30s)".to_string(),
                        )
                    })?
                    .map_err(|e| CrawlError::NavigationError(e.to_string()))?;
            }

            // Execute JS if provided
            if let Some(js) = &run_config.js_code {
                let _ = page
                    .evaluate(js.as_str())
                    .await
                    .map_err(|e| CrawlError::JsError(e.to_string()))?;
            }

            let content = page
                .content()
                .await
                .map_err(|e| CrawlError::Other(e.to_string()))?;

            let mut screenshot_data = None;
            if run_config.screenshot {
                // 10 MB cap on raw PNG bytes — a full-page screenshot of a very
                // tall page can easily exceed this and exhaust memory or S3 quota.
                const MAX_SCREENSHOT_BYTES: usize = 10 * 1024 * 1024;

                let screenshot_bytes = page
                    .screenshot(
                        chromiumoxide::page::ScreenshotParams::builder()
                            .format(
                                chromiumoxide::cdp::browser_protocol::page::CaptureScreenshotFormat::Png,
                            )
                            .full_page(true)
                            .build(),
                    )
                    .await
                    .map_err(|e| CrawlError::ScreenshotError(e.to_string()))?;

                if screenshot_bytes.len() > MAX_SCREENSHOT_BYTES {
                    return Err(CrawlError::ScreenshotError(format!(
                        "Screenshot too large ({} bytes, limit {} MB)",
                        screenshot_bytes.len(),
                        MAX_SCREENSHOT_BYTES / 1024 / 1024
                    )));
                }

                screenshot_data = Some(general_purpose::STANDARD.encode(screenshot_bytes));
            }

            Ok(CrawlResult {
                url: run_config.url,
                html: content,
                markdown: None, // To be filled later
                screenshot: screenshot_data,
                status_code: 200, // Hard to get exact status code with CDP sometimes, default to 200 if successful
                success: true,
                error_message: None,
            })
        }
        .await;

        // Always close the tab — prevents orphaned navigations accumulating in the
        // pooled browser when a timeout fired and the inner block returned Err early.
        //
        // On close failure after a successful crawl, wrap the result in
        // CloseFailedWithResult so the caller can both use the data and discard
        // this browser instance instead of returning it to the idle pool.
        match (result, page.close().await) {
            (Ok(crawl_result), Ok(())) => Ok(crawl_result),
            (Ok(crawl_result), Err(e)) => Err(CrawlError::CloseFailedWithResult {
                result: Box::new(crawl_result),
                close_error: e.to_string(),
            }),
            (Err(crawl_err), _) => Err(crawl_err),
        }
    }

    pub async fn close(mut self) {
        let _ = self.browser.close().await;
        let _ = self.handle.await;
    }
}

#[derive(Clone)]
pub struct BrowserPool {
    idle_browsers: Arc<Mutex<Vec<BrowserManager>>>,
    config: BrowserConfig,
    semaphore: Arc<Semaphore>,
}

impl BrowserPool {
    pub fn new(config: BrowserConfig) -> Self {
        // Default to a reasonable limit if not set, e.g., 10
        let limit = config.semaphore_size.unwrap_or(10);
        Self {
            idle_browsers: Arc::new(Mutex::new(Vec::new())),
            config,
            semaphore: Arc::new(Semaphore::new(limit)),
        }
    }

    pub async fn acquire(&self) -> Result<BrowserManager, CrawlError> {
        let permit = self
            .semaphore
            .clone()
            .acquire_owned()
            .await
            .map_err(|e| CrawlError::Other(format!("Semaphore acquire error: {}", e)))?;

        let mut idle = self.idle_browsers.lock().await;
        if let Some(mut browser) = idle.pop() {
            // Re-attach permit to reusable browser
            browser._permit = Some(permit);
            Ok(browser)
        } else {
            let mut browser = BrowserManager::new(&self.config).await?;
            browser._permit = Some(permit);
            Ok(browser)
        }
    }

    pub async fn release(&self, mut browser: BrowserManager) {
        // Drop the permit so other tasks can use it while this browser is idle
        browser._permit = None;
        let mut idle = self.idle_browsers.lock().await;
        idle.push(browser);
    }

    pub async fn close(&self) {
        let mut idle = self.idle_browsers.lock().await;
        for browser in idle.drain(..) {
            browser.close().await;
        }
    }

    pub fn get_concurrency_limit(&self) -> usize {
        self.config.semaphore_size.unwrap_or(10)
    }
}
