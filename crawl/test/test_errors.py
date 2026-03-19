
try:
    import lab_crawl as crawl4ai_rs
except ImportError:
    import crawl4ai_rs

class TestErrors:
    def test_import_errors(self):
        print(f"Module: {crawl4ai_rs}")
        assert hasattr(crawl4ai_rs, "CrawlError")
        assert hasattr(crawl4ai_rs, "BrowserLaunchError")
        assert hasattr(crawl4ai_rs, "NavigationError")
        assert hasattr(crawl4ai_rs, "ElementNotFound")
        assert hasattr(crawl4ai_rs, "TimeoutError")
        assert hasattr(crawl4ai_rs, "JsError")
        assert hasattr(crawl4ai_rs, "ScreenshotError")

    def test_hierarchy(self):
        CrawlError = crawl4ai_rs.CrawlError
        BrowserLaunchError = crawl4ai_rs.BrowserLaunchError
        NavigationError = crawl4ai_rs.NavigationError
        
        # Verify inheritance
        assert issubclass(BrowserLaunchError, CrawlError)
        assert issubclass(NavigationError, CrawlError)
        assert issubclass(CrawlError, Exception)

if __name__ == "__main__":
    t = TestErrors()
    t.test_import_errors()
    t.test_hierarchy()
    print("All tests passed!")
