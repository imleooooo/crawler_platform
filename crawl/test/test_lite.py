import sys
import lab_crawl

def main():
    print("Initializing Crawler...")
    # Initialize normally
    crawler = lab_crawl.Crawl4AiRs(headless=True)
    
    url = "https://www.google.com"
    print(f"Crawling {url} with lite mode...")
    
    try:
        # Test new run_mode parameter
        result = crawler.crawl(url, run_mode="lite")
        print(f"Lite Crawl Success: {result['success']}")
        print(f"HTML Length: {len(result['html'])}")
        
        if result['success']:
            # Verify it's actually HTML (standard)
            if "<html" in result['html'].lower() or "<!doctype" in result['html'].lower():
                 print("Content looks like HTML.")
            else:
                 print("Warning: Content might not be HTML.")
            
            if result.get('markdown'):
                 print("Markdown was generated (standard behavior).")
        else:
            print(f"Error Message: {result.get('error_message')}")

    except TypeError as e:
        print(f"TypeError caught: {e}")
        print("This likely means the 'run_mode' argument is not yet recognized by the installed lab_crawl package.")
        print("Please re-compile with 'maturin develop' --release.")
    except Exception as e:
        print(f"Error during lite crawl: {e}")

if __name__ == "__main__":
    main()
