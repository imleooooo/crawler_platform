import sys
import os

# We need to make sure we can import the built module.
# Typically maturin builds into target/wheels or installs into virtualenv.
# For this example, users would run `maturin develop` which installs into current venv.

try:
    import lab_crawl
except ImportError:
    print("Could not import lab_crawl. Did you run 'maturin develop'?")
    sys.exit(1)

def main():
    print("Initializing Crawler (Rust) with rotation...")
    crawler = lab_crawl.Crawl4AiRs(headless=True, rotate_user_agent=True)
    
    url = "https://spark.apache.org/docs/latest/rdd-programming-guide.html"
    print(f"Crawling {url}...")
    
    try:
        result = crawler.crawl(url)
        print("Crawl Success!")
        print(f"URL: {result['url']}")
        print(f"HTML Length: {len(result['html'])}")
        print(f"Success: {result['success']}")
        
        if result.get('markdown'):
            print("Markdown Snippet:")
            print(result['markdown'])
            
    except Exception as e:
        print(f"Error during crawl: {e}")

if __name__ == "__main__":
    main()
