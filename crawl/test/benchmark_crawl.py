import time
import asyncio
import argparse
import sys

# Try imports
try:
    import aiohttp
except ImportError:
    aiohttp = None

try:
    from playwright.async_api import async_playwright
except ImportError:
    async_playwright = None

try:
    import lab_crawl
except ImportError:
    print("Error: lab_crawl not installed. Run 'maturin develop --release' in crawl/ directory.")
    sys.exit(1)

try:
    from bs4 import BeautifulSoup
except ImportError:
    BeautifulSoup = None

try:
    from crawl4ai import AsyncWebCrawler
try:
    from crawl4ai import AsyncWebCrawler
except ImportError:
    AsyncWebCrawler = None

# Configuration
URLS = [
    "https://www.ithome.com.tw/news/172911", 
]

async def benchmark_crawl4ai(urls):
    if not AsyncWebCrawler:
        print("Skipping crawl4ai: not installed")
        return

    print(f"--- Benchmarking crawl4ai ({len(urls)} urls) ---")
    start = time.time()
    
    # crawl4ai manages its own browser session
    async with AsyncWebCrawler(verbose=False) as crawler:
        tasks = []
        for url in urls:
            tasks.append(crawler.arun(url=url))
        
        results = await asyncio.gather(*tasks)
        
    duration = time.time() - start
    print(f"crawl4ai finished in {duration:.4f}s")
    return duration


async def benchmark_bs4(urls):
    if not aiohttp or not BeautifulSoup:
        print("Skipping BS4: aiohttp or beautifulsoup4 not installed")
        return

    print(f"--- Benchmarking BS4 + aiohttp ({len(urls)} urls) ---")
    start = time.time()
    async with aiohttp.ClientSession() as session:
        tasks = []
        for url in urls:
             tasks.append(fetch_parse_bs4(session, url))
        results = await asyncio.gather(*tasks)
    
    duration = time.time() - start
    print(f"BS4 finished in {duration:.4f}s")
    return duration

async def fetch_parse_bs4(session, url):
    try:
        async with session.get(url) as response:
            html = await response.text()
            soup = BeautifulSoup(html, 'html.parser')
            # Simulate some work
            title = soup.title.string if soup.title else "No Title"
            return True
    except Exception as e:
        print(f"BS4 error {url}: {e}")
        return False


async def benchmark_aiohttp(urls):
    if not aiohttp:
        print("Skipping aiohttp: not installed")
        return
    
    print(f"--- Benchmarking aiohttp ({len(urls)} urls) ---")
    start = time.time()
    async with aiohttp.ClientSession() as session:
        tasks = []
        for url in urls:
            tasks.append(fetch_aiohttp(session, url))
        results = await asyncio.gather(*tasks)
    
    duration = time.time() - start
    print(f"aiohttp finished in {duration:.4f}s")
    return duration

async def fetch_aiohttp(session, url):
    try:
        async with session.get(url) as response:
            await response.text()
            return True
    except Exception as e:
        print(f"aiohttp error {url}: {e}")
        return False

async def benchmark_playwright(urls):
    if not async_playwright:
        print("Skipping playwright: not installed")
        return

    print(f"--- Benchmarking playwright ({len(urls)} urls) ---")
    start = time.time()
    
    async with async_playwright() as p:
        browser = await p.chromium.launch(headless=True)
        # Playwright usually processes page by page or needs multiple contexts for true parallelism
        # Here we do sequential to be fair to single-browser instance usage, 
        # OR we can launch multiple contexts. 
        # Typically lab_crawl's crawl_many is parallel.
        # Let's try to mimic basic usage: one context, multiple pages?
        # Or just sequential for simplicity in "browser automation"
        
        # Parallel implementation for fairness
        tasks = []
        for url in urls:
             tasks.append(fetch_playwright(browser, url))
        
        await asyncio.gather(*tasks)
        
        await browser.close()

    duration = time.time() - start
    print(f"playwright finished in {duration:.4f}s")
    return duration

async def fetch_playwright(browser, url):
    try:
        page = await browser.new_page()
        await page.goto(url)
        content = await page.content()
        await page.close()
        return True
    except Exception as e:
        print(f"playwright error {url}: {e}")
        return False

def benchmark_lab_crawl(urls):
    print(f"--- Benchmarking lab_crawl ({len(urls)} urls) ---")
    
    # Init
    start_init = time.time()
    crawler = lab_crawl.Crawl4AiRs(headless=True)
    print(f"lab_crawl init time: {time.time() - start_init:.4f}s")

    start = time.time()
    try:
        # lab_crawl.crawl_many is blocking/sync call in Python but parallel in Rust
        results = crawler.crawl_many(urls)
        # Just to ensure we got results
        success_count = sum(1 for r in results if r['success'])
        print(f"lab_crawl success: {success_count}/{len(urls)}")
    finally:
        crawler.close()

    duration = time.time() - start
    print(f"lab_crawl finished in {duration:.4f}s")
    return duration

async def main():
    print(f"Comparison on {len(URLS)} URLs:\n{URLS}\n")
    
    # 1. aiohttp (Baseline HTTP)
    await benchmark_aiohttp(URLS)
    print("-" * 30)

    # 2. BS4 + aiohttp (Static Scraping)
    await benchmark_bs4(URLS)
    print("-" * 30)

    # 3. lab_crawl (Rust + CDP)
    # Run in thread executor because it's blocking? Or just call it directly since it's a script.
    # It communicates via channel to Rust, but Python side blocks? 
    # Based on docs, it returns results, so it blocks.
    benchmark_lab_crawl(URLS)
    print("-" * 30)

    # 4. Playwright (Python + CDP)
    await benchmark_playwright(URLS)
    print("-" * 30)

    # 5. crawl4ai (Public Package)
    await benchmark_crawl4ai(URLS)

if __name__ == "__main__":
    asyncio.run(main())
