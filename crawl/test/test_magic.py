import asyncio
import sys
try:
    import lab_crawl
except ImportError:
    print("Could not import lab_crawl. Run 'maturin develop'.")
    sys.exit(1)

def main():
    crawler = lab_crawl.Crawl4AiRs(headless=True)
    # Homepage - should be filtered if magic=True, full if magic=False
    url = "https://www.ithome.com.tw/" 

    print(f"--- Crawling {url} ---")

    # 1. Default (magic_markdown=False)
    print("\n[1] Default (Full):")
    res1 = crawler.crawl(url, magic_markdown=False)
    if res1['success']:
        print(f"Length: {len(res1['html'])}")
        snippet = res1['markdown'][:500].replace('\n', ' ')
        print(f"Snippet: {snippet}...")
    else:
        print("Failed")

    # 2. Magic (magic_markdown=True)
    print("\n[2] Magic (Smart):")
    res2 = crawler.crawl(url, magic_markdown=True)
    if res2['success']:
        print(f"Length: {len(res2['html'])}") # HTML length is same, but Markdown should be different
        # Note: HTML in result is raw HTML. Markdown is generated from it.
        snippet = res2['markdown'][:500].replace('\n', ' ')
        print(f"Snippet: {snippet}...")
    else:
        print("Failed")

if __name__ == "__main__":
    main()
