#![allow(clippy::collapsible_if)]
pub mod config;
pub mod crawler;
pub mod errors;
pub mod strategies;
pub mod user_agents;
pub mod utils;

pub use config::{BrowserConfig, CrawlerRunConfig};
pub use crawler::AsyncWebCrawler;

pub mod python_binding;
