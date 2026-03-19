use feed_rs::parser;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Test Temp Dir
    println!("Testing temp dir...");
    let temp_dir = tempfile::tempdir()?;
    println!("Temp dir created at {:?}", temp_dir.path());

    // 2. Test Complex Query
    let keyword = "electron";
    let year = "2023";
    let query = format!(
        "all:{} AND submittedDate:[{}01010000 TO {}12312359]",
        keyword, year, year
    );
    println!("Query: {}", query);

    let url = "http://export.arxiv.org/api/query";
    let client = reqwest::Client::new();

    let resp = client
        .get(url)
        .query(&[
            ("search_query", &query),
            ("start", &"0".to_string()),
            ("max_results", &"5".to_string()),
            ("sortBy", &"submittedDate".to_string()),
            ("sortOrder", &"descending".to_string()),
        ])
        .send()
        .await?;

    println!("Status: {}", resp.status());
    if !resp.status().is_success() {
        println!("Error status returned");
        return Ok(());
    }

    let bytes = resp.bytes().await?;
    println!("Got {} bytes", bytes.len());

    let feed = parser::parse(bytes.as_ref())?;
    println!("Parsed feed with {} entries", feed.entries.len());

    Ok(())
}
