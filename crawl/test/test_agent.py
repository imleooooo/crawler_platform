import asyncio
import os
from lab_crawl import Crawl4AiRs

async def test_agent():
    print("Testing Agent Flow...")
    crawler = Crawl4AiRs(headless=True)
    
    # We can mock this or expect it to fail gracefully if no API key
    api_key = os.getenv("OPENAI_API_KEY", "mock-key")
    
    try:
        # This might fail if the key is invalid, which is expected during test
        # UNLESS we mock the server, but for integration test, we just want to see if arguments pass through
        # and Rust accepts it.
        result = crawler.crawl(
            url="https://example.com",
            run_mode="agent",
            api_key=api_key,
            model="gpt-4o",
            prompt="Extract the main title"
        )
        print("Agent Result:", result)
        
    except Exception as e:
        print(f"Agent verification ended (expected if no valid key): {e}")

if __name__ == "__main__":
    # We need to run this in an event loop if the bindings are async compatible 
    # BUT current bindings block_on internal runtime, so it's sync from Python side.
    # Wait, lab_crawl_adapter runs it in executor. 
    # The direct binding 'crawl' is blocking (PyResult<Py<PyAny>>).
    
    crawler = Crawl4AiRs(headless=True)
    api_key = os.getenv("OPENAI_API_KEY", "sk-mock-key")
    
    print(f"Running agent test with key: {api_key[:4]}...")
    try:
        result = crawler.crawl(
            url="https://example.com", 
            run_mode="agent",
            api_key=api_key,
            model="gpt-4o",
            prompt="Find the h1 tag"
        )
        print("Result:", result)
    except Exception as e:
        print(f"Error (likely valid if no real key): {e}")
