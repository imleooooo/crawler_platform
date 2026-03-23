use crate::config::{BrowserConfig, CrawlResult, CrawlerRunConfig};

use crate::errors::CrawlError;
use crate::strategies::browser::BrowserPool;
use crate::strategies::http::HttpCrawler;
use crate::strategies::markdown::MarkdownGenerator;
use futures::stream::{self, StreamExt};

use crate::strategies::agent::Agent;

pub struct AsyncWebCrawler {
    browser_pool: BrowserPool,
    markdown_generator: MarkdownGenerator,
    http_crawler: HttpCrawler,
    agent: Agent,
}

impl AsyncWebCrawler {
    pub async fn new(config: BrowserConfig) -> Result<Self, CrawlError> {
        let browser_pool = BrowserPool::new(config);
        let markdown_generator = MarkdownGenerator::new();
        let http_crawler = HttpCrawler::new();
        let agent = Agent::new();
        Ok(Self {
            browser_pool,
            markdown_generator,
            http_crawler,
            agent,
        })
    }

    pub async fn arun(
        &self,
        url: &str,
        run_config: Option<CrawlerRunConfig>,
    ) -> Result<CrawlResult, CrawlError> {
        let mut config = run_config.unwrap_or_default();
        config.url = url.to_string();

        let magic_markdown = config.magic_markdown;
        let use_lite = config.run_mode.as_deref() == Some("lite");
        let use_agent = config.run_mode.as_deref() == Some("agent");

        let result = if use_lite {
            self.http_crawler.crawl(config.clone()).await
        } else if use_agent {
            // Agent Flow
            let manager = self.browser_pool.acquire().await?;
            let page = manager.get_current_page().await?;

            // Navigate first
            page.goto(&config.url)
                .await
                .map_err(|e| CrawlError::NavigationError(format!("Initial Nav failed: {}", e)))?;
            page.wait_for_navigation()
                .await
                .map_err(|e| CrawlError::NavigationError(e.to_string()))?;

            let agent_text = self.agent.run(&page, &config).await?;
            self.browser_pool.release(manager).await;

            Ok(CrawlResult {
                url: config.url.clone(),
                html: String::new(),
                markdown: Some(agent_text),
                screenshot: None,
                status_code: 200,
                success: true,
                error_message: None,
            })
        } else {
            let manager = self.browser_pool.acquire().await?;
            let res = manager.crawl(config.clone()).await;
            if res.is_ok() {
                // Healthy crawl — return browser to idle pool for reuse.
                self.browser_pool.release(manager).await;
            } else {
                // Crawl failed (navigation error, timeout, close failure, etc.).
                // Discard the instance rather than returning it to the pool;
                // close() drops the semaphore permit so the slot is released.
                manager.close().await;
            }
            res
        };

        let mut result = result?;

        // Generate markdown only if it's NOT agent/lite result that already handled it
        if !use_agent && !use_lite && result.markdown.is_none() {
            let md = self.markdown_generator.generate(
                &result.html,
                Some(&result.url),
                magic_markdown,
            )?;

            // Apply ignore_links if requested
            let md = if config.ignore_links {
                crate::utils::clean_markdown_links(&md)
            } else {
                md
            };

            result.markdown = Some(md);
        }

        Ok(result)
    }

    pub async fn arun_many(
        &self,
        urls: Vec<String>,
        run_config: Option<CrawlerRunConfig>,
    ) -> Vec<CrawlResult> {
        let config = run_config.unwrap_or_default();

        stream::iter(urls)
            .map(|url| {
                let mut c = config.clone();
                c.url = url.clone();
                let url_str = c.url.clone();
                async move {
                    match self.arun(&url_str, Some(c)).await {
                        Ok(res) => res,
                        Err(e) => {
                            // Convert error to CrawlResult here so we have the URL
                            CrawlResult {
                                url: url_str,
                                html: "".to_string(),
                                markdown: None,
                                screenshot: None,
                                status_code: 0,
                                success: false,
                                error_message: Some(e.to_string()),
                            }
                        }
                    }
                }
            })
            .buffer_unordered(self.browser_pool.get_concurrency_limit()) // Concurrency
            .collect::<Vec<_>>()
            .await
    }

    pub async fn close(&self) {
        self.browser_pool.close().await;
    }
}
