import sys
import time
import lab_crawl

def main():
    print("Initializing Crawler with semaphore_size=2...")
    # Initialize with small semaphore to force concurrency limit
    crawler = lab_crawl.Crawl4AiRs(headless=True, semaphore_size=2)
    
    urls = [
        "https://www.google.com",
        "https://www.bing.com",
        "https://www.yahoo.com",
        "https://duckduckgo.com",
        "https://www.wikipedia.org"
    ]
    
    print(f"Crawling {len(urls)} URLs with limit 2...")
    start_time = time.time()
    
    try:
        results = crawler.crawl_many(urls)
        end_time = time.time()
        
        print(f"Completed in {end_time - start_time:.2f} seconds")
        print(f"Success/Total: {len([r for r in results if r['success']])}/{len(results)}")
        
        for r in results:
            print(f"URL: {r['url']}, Success: {r['success']}, HTML Len: {len(r['html'])}")
            if not r['success'] and r.get('error_message'):
                print(f"  Error: {r['error_message']}")
            
    except Exception as e:
        print(f"Error during crawl: {e}")
    finally:
        crawler.close()

if __name__ == "__main__":
    main()
