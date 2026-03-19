use lab_crawl::utils::init_logging;
use lab_crawl::{AsyncWebCrawler, BrowserConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    init_logging();

    let config = BrowserConfig::default();
    let crawler = AsyncWebCrawler::new(config).await?;

    let url = "https://example.com";
    tracing::info!("Crawling {}...", url);
    let result = crawler.arun(url, None).await?;

    tracing::info!("HTML Length: {}", result.html.len());
    if let Some(md) = result.markdown {
        tracing::info!(
            "Markdown Content:\n----------------\n{}\n----------------",
            md
        );
    }

    if let Some(screenshot) = result.screenshot {
        tracing::info!("Screenshot captured ({} bytes)", screenshot.len());
    }

    crawler.close().await;

    Ok(())
}
