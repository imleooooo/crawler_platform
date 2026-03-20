#![allow(clippy::collapsible_if)]

use serde_json::Value;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let keyword = "The Daily"; // Popular podcast
    let year = Some("".to_string()); // Test empty year

    // 1. iTunes Search
    let client = reqwest::Client::new();
    let itunes_url = "https://itunes.apple.com/search";
    println!("Searching iTunes for '{}'...", keyword);

    let resp = client
        .get(itunes_url)
        .query(&[("media", "podcast"), ("term", keyword), ("limit", "1")])
        .send()
        .await?;

    let text = resp.text().await?;
    let data: Value = serde_json::from_str(&text)?;

    let results = data["results"]
        .as_array()
        .ok_or("Missing 'results' array in iTunes response")?;
    if results.is_empty() {
        println!("No podcasts found on iTunes.");
        return Ok(());
    }

    let feed_url = results[0]["feedUrl"]
        .as_str()
        .ok_or("Missing 'feedUrl' in first iTunes result")?;
    println!("Found Feed URL: {}", feed_url);

    // 2. Parse Feed
    println!("Fetching feed...");
    let feed_resp = client.get(feed_url).send().await?;
    let feed_bytes = feed_resp.bytes().await?;

    let feed = feed_rs::parser::parse(feed_bytes.as_ref())?;
    println!("Feed parsed. extracted {} entries.", feed.entries.len());

    let mut matched_count = 0;
    for entry in feed.entries.iter().take(10) {
        let title = entry
            .title
            .as_ref()
            .map(|t| t.content.clone())
            .unwrap_or_default();
        let published = entry.published.map(|t| t.to_rfc3339()).unwrap_or_default();

        println!("Entry: {} ({})", title, published);

        // Check Year
        if let Some(y) = &year {
            if !y.is_empty() && !published.starts_with(y) {
                println!("  -> Skipped (Year mismatch)");
                continue;
            }
        }

        // Debug print
        // println!("  -> Links: {:?}", entry.links);
        // println!("  -> Media: {:?}", entry.media);

        // Check Audio URL (Fix Logic)
        let mut audio_url_opt = None;

        // 1. Check Links (Atom/RSS with explicit type)
        if let Some(link) = entry.links.iter().find(|l| {
            l.media_type
                .as_deref()
                .map(|m| m.starts_with("audio"))
                .unwrap_or(false)
        }) {
            audio_url_opt = Some(link.href.clone());
        }

        // 2. Check Media/Enclosures (Standard RSS)
        if audio_url_opt.is_none() {
            for media in &entry.media {
                for content in &media.content {
                    if let Some(mime) = &content.content_type {
                        if mime.to_string().starts_with("audio") {
                            if let Some(url) = &content.url {
                                audio_url_opt = Some(url.to_string());
                                break;
                            }
                        }
                    }
                }
                if audio_url_opt.is_some() {
                    break;
                }
            }
        }

        if let Some(l) = audio_url_opt {
            println!("  -> Found Audio: {}", l);
            matched_count += 1;
        } else {
            println!("  -> No audio link found.");
        }
    }

    println!("Total matched entries: {}", matched_count);

    Ok(())
}
