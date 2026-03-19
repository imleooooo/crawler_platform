import asyncio
import time
from lab_crawl import Crawl4AiRs

async def main():
    url = "https://www.reddit.com/r/rust/"
    
    print("--- Test 1: Normal Headless (Default) ---")
    start = time.time()
    crawler_normal = Crawl4AiRs(True) # headless=True
    res_normal = crawler_normal.crawl(url)
    print(f"Normal crawl took: {time.time() - start:.2f}s")
    print(f"Success: {res_normal['success']}")
    crawler_normal.close() # Close first browser

    print("\n--- Test 2: Optimization (No Images/CSS) ---")
    start = time.time()
    # verify signature: headless=True, user_agent=None, disable_images=True, disable_css=True
    crawler_opt = Crawl4AiRs(True, None, True, True)
    res_opt = crawler_opt.crawl(url)
    print(f"Optimized crawl took: {time.time() - start:.2f}s")
    print(f"Success: {res_opt['success']}")
    crawler_opt.close()
    
    # We expect optimized to be faster, or at least successful.

if __name__ == "__main__":
    asyncio.run(main())
