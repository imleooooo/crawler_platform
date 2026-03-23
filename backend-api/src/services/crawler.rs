use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Shared per-domain politeness throttle.  Maps hostname → earliest time the
/// next outbound request to that domain is allowed.
pub type DomainThrottle = Arc<Mutex<HashMap<String, Instant>>>;

#[derive(Serialize, Deserialize, Clone)]
pub struct CrawlerRequest {
    pub urls: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run_mode: Option<String>,
    /// OpenAI API key used only for in-process agent calls.
    /// Skipped in (de)serialization so it is never written to Redis or S3.
    #[serde(skip)]
    pub api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub prompt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_dir: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub bucket_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ignore_links: Option<bool>,
}

impl std::fmt::Debug for CrawlerRequest {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CrawlerRequest")
            .field("urls", &self.urls)
            .field("run_mode", &self.run_mode)
            .field("api_key", &self.api_key.as_ref().map(|_| "[REDACTED]"))
            .field("prompt", &self.prompt)
            .field("model", &self.model)
            .field("output_dir", &self.output_dir)
            .field("bucket_name", &self.bucket_name)
            .field("ignore_links", &self.ignore_links)
            .finish()
    }
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct CrawlResult {
    pub url: String,
    pub title: Option<String>,
    pub published_at: Option<String>,
    pub success: bool,
    pub markdown: Option<String>,
    pub error: Option<String>,
}

#[derive(Deserialize, Debug)]
pub struct CrawlerResponse {
    pub results: Vec<CrawlResult>,
}

/// Native implementation of crawling without external service
use futures::stream::{self, StreamExt};

/// Native implementation of crawling without external service
pub async fn call_crawler_service(
    req: &CrawlerRequest,
    throttle: DomainThrottle,
) -> Result<CrawlerResponse, String> {
    let client = reqwest::Client::builder()
        .user_agent("Agentic-Crawler/1.0")
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|e| e.to_string())?;

    let urls = req.urls.clone();
    let concurrency_limit = 10; // Process 10 URLs concurrently

    let results = stream::iter(urls)
        .map(|url| {
            let client = client.clone();
            let req = req.clone();
            let throttle = throttle.clone();
            async move {
                match fetch_and_parse(&client, &url, req.ignore_links.unwrap_or(false), &throttle).await {
                    Ok((title, published_at, markdown)) => {
                        let final_markdown = if req.run_mode.as_deref() == Some("agent") {
                            if let (Some(prompt), Some(api_key)) = (&req.prompt, &req.api_key) {
                                match call_openai_api(
                                    &client,
                                    api_key,
                                    req.model.as_deref().unwrap_or("gpt-4o"),
                                    prompt,
                                    &markdown,
                                )
                                .await
                                {
                                    Ok(llm_response) => llm_response,
                                    Err(e) => {
                                        tracing::error!("OpenAI API failed: {}", e);
                                        format!(
                                            "Error calling OpenAI: {}\n\nOriginal Content:\n{}",
                                            e, markdown
                                        )
                                    }
                                }
                            } else {
                                markdown
                            }
                        } else {
                            markdown
                        };

                        CrawlResult {
                            url,
                            title,
                            published_at,
                            success: true,
                            markdown: Some(final_markdown),
                            error: None,
                        }
                    }
                    Err(e) => CrawlResult {
                        url,
                        title: None,
                        published_at: None,
                        success: false,
                        markdown: None,
                        error: Some(e.to_string()),
                    },
                }
            }
        })
        .buffer_unordered(concurrency_limit)
        .collect::<Vec<CrawlResult>>()
        .await;

    Ok(CrawlerResponse { results })
}

async fn call_openai_api(
    client: &reqwest::Client,
    api_key: &str,
    model: &str,
    user_prompt: &str,
    context_markdown: &str,
) -> Result<String, String> {
    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": "You are a helpful assistant. You will be provided with the content of a web page in Markdown format. Your task is to process this content according to the user's instructions."
            },
            {
                "role": "user",
                "content": format!("Context:\n{}\n\nTask:\n{}", context_markdown, user_prompt)
            }
        ]
    });

    let resp = client
        .post("https://api.openai.com/v1/chat/completions")
        .header("Authorization", format!("Bearer {}", api_key))
        .json(&payload)
        .send()
        .await
        .map_err(|e| e.to_string())?;

    if !resp.status().is_success() {
        let error_text = resp.text().await.map_err(|e| e.to_string())?;
        return Err(format!("OpenAI API Error: {}", error_text));
    }

    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    let content = body["choices"]
        .as_array()
        .and_then(|choices| choices.first())
        .and_then(|choice| choice["message"]["content"].as_str())
        .ok_or("Failed to parse OpenAI response content: choices array is empty or missing")?
        .to_string();

    Ok(content)
}

/// Extract title from HTML using regex
fn extract_title(html: &str) -> Option<String> {
    // Try to find <title>...</title> tag
    let title_regex = regex::Regex::new(r"(?i)<title[^>]*>([^<]+)</title>").ok()?;
    if let Some(caps) = title_regex.captures(html) {
        let title = caps.get(1)?.as_str().trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }

    // Fallback: try og:title meta tag
    let og_title_regex = regex::Regex::new(
        r#"(?i)<meta[^>]*property=["']og:title["'][^>]*content=["']([^"']+)["']"#,
    )
    .ok()?;
    if let Some(caps) = og_title_regex.captures(html) {
        let title = caps.get(1)?.as_str().trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }

    // Try alternative og:title format
    let og_title_alt_regex = regex::Regex::new(
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*property=["']og:title["']"#,
    )
    .ok()?;
    if let Some(caps) = og_title_alt_regex.captures(html) {
        let title = caps.get(1)?.as_str().trim().to_string();
        if !title.is_empty() {
            return Some(title);
        }
    }

    None
}

/// Extract published date from HTML meta tags
fn extract_published_date(html: &str) -> Option<String> {
    // Common meta tags for publication date
    let patterns = [
        // article:published_time (Open Graph)
        r#"(?i)<meta[^>]*property=["']article:published_time["'][^>]*content=["']([^"']+)["']"#,
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*property=["']article:published_time["']"#,
        // datePublished (Schema.org)
        r#"(?i)"datePublished"\s*:\s*"([^"]+)""#,
        // pubdate
        r#"(?i)<meta[^>]*name=["']pubdate["'][^>]*content=["']([^"']+)["']"#,
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*name=["']pubdate["']"#,
        // date
        r#"(?i)<meta[^>]*name=["']date["'][^>]*content=["']([^"']+)["']"#,
        r#"(?i)<meta[^>]*content=["']([^"']+)["'][^>]*name=["']date["']"#,
        // DC.date
        r#"(?i)<meta[^>]*name=["']DC\.date["'][^>]*content=["']([^"']+)["']"#,
        // time tag with datetime attribute
        r#"(?i)<time[^>]*datetime=["']([^"']+)["']"#,
    ];

    for pattern in patterns {
        if let Ok(regex) = regex::Regex::new(pattern) {
            if let Some(caps) = regex.captures(html) {
                if let Some(date) = caps.get(1) {
                    let date_str = date.as_str().trim().to_string();
                    if !date_str.is_empty() {
                        return Some(date_str);
                    }
                }
            }
        }
    }

    None
}

async fn fetch_and_parse(
    client: &reqwest::Client,
    url: &str,
    ignore_links: bool,
    throttle: &DomainThrottle,
) -> Result<(Option<String>, Option<String>, String), String> {
    // Per-domain politeness delay: at least 1 s between successive requests to
    // the same host.  We reserve the next slot under a brief lock so concurrent
    // requests to the same domain queue up correctly without racing.
    let domain = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_default();

    if !domain.is_empty() {
        let sleep_dur = {
            let mut map = throttle.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let next_avail = map.get(&domain).copied().unwrap_or(now);
            let sleep = next_avail.saturating_duration_since(now);
            // Advance the slot by 1 s regardless of whether we sleep, so the
            // next concurrent request to this domain queues 1 s behind us.
            map.insert(domain, now.max(next_avail) + Duration::from_secs(1));
            sleep
        };
        if sleep_dur > Duration::ZERO {
            tokio::time::sleep(sleep_dur).await;
        }
    }

    let resp = client.get(url).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    if !status.is_success() {
        return Err(format!("HTTP {}", status));
    }

    // Read body as text
    let html = resp.text().await.map_err(|e| e.to_string())?;

    // Extract title and published date from HTML before converting to markdown
    let title = extract_title(&html);
    let published_at = extract_published_date(&html);

    let mut markdown = html2text::from_read(html.as_bytes(), 80)
        .map_err(|e| format!("HTML parse error: {}", e))?;

    if ignore_links {
        markdown = clean_markdown_links(&markdown);
    }

    Ok((title, published_at, markdown))
}

static CRAWLER_REF_LINE_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static CRAWLER_MARKER_REGEX: OnceLock<regex::Regex> = OnceLock::new();
static CRAWLER_WHITESPACE_REGEX: OnceLock<regex::Regex> = OnceLock::new();

fn clean_markdown_links(markdown: &str) -> String {
    // 1. Remove reference lines at the bottom.
    let ref_line_regex = CRAWLER_REF_LINE_REGEX
        .get_or_init(|| regex::Regex::new(r"(?m)^\[\d+\]:.*(?:\n[^\[\r\n].*)*").expect("valid regex"));
    let cleaned_text = ref_line_regex.replace_all(markdown, "");

    // 2. Remove [n] markers from text, e.g. "some text [1]" -> "some text"
    let marker_regex =
        CRAWLER_MARKER_REGEX.get_or_init(|| regex::Regex::new(r"\[\d+\]").expect("valid regex"));
    let cleaned_text = marker_regex.replace_all(&cleaned_text, "");

    // 3. Remove excess newlines that might be left behind
    let whitespace_regex = CRAWLER_WHITESPACE_REGEX
        .get_or_init(|| regex::Regex::new(r"\n{3,}").expect("valid regex"));
    let final_text = whitespace_regex.replace_all(&cleaned_text, "\n\n");

    final_text.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_markdown_links() {
        let input = r#"
This is a paragraph with a link [1] and another [2].

[1]: https://www.typescriptlang.org/docs/handbook/2/basic-types.html
[2]: http://example.com
"#;
        let expected = r#"
This is a paragraph with a link  and another .
"#;
        assert_eq!(clean_markdown_links(input), expected.trim());

        let input2 = "Hello [1] World.\n\n[1]: http://test.com";
        let output2 = clean_markdown_links(input2);

        assert_eq!(output2, "Hello  World.");
    }

    #[test]
    fn test_clean_markdown_links_wrapped() {
        let input = r#"
Some text with [3] link.

[3]: https://events.linuxfoundation.org/kubecon-cloudnativecon-europe/?utm_sourc
e=cncf&utm_medium=subpage&utm_campaign=18269725-KubeCon-EU-2026&utm_content=hell
o-bar
[4]: /
"#;
        let cleaned = clean_markdown_links(input);
        println!("Cleaned output:\n{}", cleaned);

        assert!(!cleaned.contains("utm_sourc"));
        assert!(!cleaned.contains("e=cncf"));
        assert!(!cleaned.contains("o-bar"));
        assert_eq!(cleaned.trim(), "Some text with  link.");
    }
}
