use thiserror::Error;

#[derive(Error, Debug)]
pub enum CrawlError {
    #[error("Browser launch error: {0}")]
    BrowserLaunchError(String),
    #[error("Navigation error: {0}")]
    NavigationError(String),
    #[error("Element not found: {0}")]
    ElementNotFound(String),
    #[error("Timeout waiting for: {0}")]
    Timeout(String),
    #[error("Javascript execution error: {0}")]
    JsError(String),
    #[error("Screenshot error: {0}")]
    ScreenshotError(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Other error: {0}")]
    Other(String),
}
