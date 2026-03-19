#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test case: Empty string for year
    let keyword = "electron";
    let year = "   "; // Simulating empty/whitespace input

    // Logic from arxiv.rs (FIXED VERSION)
    let mut query = format!("all:{}", keyword);
    // Mimic the fix: check for empty
    if !year.trim().is_empty() {
        query.push_str(&format!(
            " AND submittedDate:[{}01010000 TO {}12312359]",
            year, year
        ));
    }

    println!("Generated Query: {}", query);

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
    if resp.status().is_success() {
        println!("Success! Fix confirmed.");
    } else {
        println!("Failure! Status: {}", resp.status());
        let text = resp.text().await?;
        println!("Response: {}", text);
    }

    Ok(())
}
