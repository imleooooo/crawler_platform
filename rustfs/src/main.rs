use aws_config::BehaviorVersion;
use aws_sdk_s3::{config::{Credentials, Region}, Client};
use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let access_key = "admin";
    let secret_key = "password";
    let endpoint = "http://127.0.0.1:9000";
    let region = "us-east-1"; // Region is often ignored but required by SDK

    println!("Connecting to RustFS at {}...", endpoint);

    let credentials = Credentials::new(access_key, secret_key, None, None, "rustfs");
    
    let config = aws_config::defaults(BehaviorVersion::latest())
        .credentials_provider(credentials)
        .region(Region::new(region))
        .endpoint_url(endpoint)
        .load()
        .await;

    let client = Client::new(&config);

    let bucket_name = "demo-bucket";

    // 1. Create Bucket
    println!("Creating bucket '{}'...", bucket_name);
    match client.create_bucket().bucket(bucket_name).send().await {
        Ok(_) => println!("Successfully created bucket."),
        Err(e) => {
            // Check if bucket already exists or handles error gracefully
            println!("Note: Bucket might already exist or error occurred: {}", e);
        }
    }

    // 2. Put Object
    let file_name = "hello.txt";
    let content = "Hello, RustFS World!";
    println!("Uploading file '{}' with content: '{}'", file_name, content);
    
    client
        .put_object()
        .bucket(bucket_name)
        .key(file_name)
        .body(content.as_bytes().to_vec().into())
        .send()
        .await?;
    println!("Upload successful.");

    // 3. Get Object
    println!("Downloading file '{}'...", file_name);
    let resp = client
        .get_object()
        .bucket(bucket_name)
        .key(file_name)
        .send()
        .await?;

    let data = resp.body.collect().await?;
    let downloaded_content = String::from_utf8(data.into_bytes().to_vec())?;
    
    println!("Downloaded content: '{}'", downloaded_content);

    if downloaded_content == content {
        println!("SUCCESS: Content matches!");
    } else {
        println!("FAILURE: Content mismatch.");
    }

    // 4. List Objects
    println!("Listing objects in bucket '{}':", bucket_name);
    let resp = client.list_objects_v2().bucket(bucket_name).send().await?;
    for object in resp.contents() {
        println!(" - {}", object.key().unwrap_or("<unknown>"));
    }

    Ok(())
}
