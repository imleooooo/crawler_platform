# lab-crawl 使用指南

`lab-crawl` 是一個高效的 Python 爬蟲套件，核心使用 Rust 語言編寫，基於 Chromium (CDP) 進行網頁自動化。它專為需要高效能、支援 JavaScript 渲染以及 LLM 友善 (Markdown 輸出) 的場景設計。

## 1. 安裝 (Installation)

由於本套件包含 Rust 擴充，建議使用 `maturin` 進行安裝。

### 前置需求

- Python 3.8+
- Rust 編譯環境 (建議透過 `rustup` 安裝)

### 安裝步驟

```bash
# 1. 建立並啟動虛擬環境 (建議)
python3 -m venv .venv
source .venv/bin/activate  # macOS/Linux
# .venv\Scripts\activate   # Windows

# 2. 安裝 maturin
pip install maturin

# 3. 編譯並安裝套件 (在專案根目錄執行)
maturin develop --release
```

## 2. 快速開始 (Quick Start)

以下是一個最基本的爬蟲範例：

```python
import lab_crawl

def main():
    # 初始化爬蟲 (預設啟用 headless 模式)
    crawler = lab_crawl.Crawl4AiRs(headless=True)
    
    url = "https://www.example.com"
    print(f"正在爬取: {url}")
    
    try:
        # 執行爬取
        result = crawler.crawl(url)
        
        if result['success']:
            print("爬取成功！")
            # 取得 Markdown 內容 (適合 LLM 使用)
            print("--- Markdown Content ---")
            print(result['markdown'][:500]) # 只顯示前 500 字
        else:
            print(f"爬取失敗: {result.get('error_message')}")
            
    finally:
        # 關閉爬蟲資源
        crawler.close()

if __name__ == "__main__":
    main()
```

## 3. 進階功能 (Advanced Features)

### 3.1 初始化設定 (Configuration)

`Crawl4AiRs` 建構函式支援多種設定以優化爬蟲行為：

```python
crawler = lab_crawl.Crawl4AiRs(
    headless=True,          # 是否使用無頭模式 (預設: True)
    user_agent="MyBot/1.0", # 自定義 User-Agent (預設: None)
    rotate_user_agent=True, # 是否自動輪替常見 User-Agent (預設: False)
    disable_images=True,    # 停用圖片載入以加速 (預設: False)
    disable_css=True,       # 停用 CSS 以加速 (預設: False)
    semaphore_size=10       # 全域並發數量限制 (預設: None，無限制)
)
```

- **User Agent 輪替**: 開啟 `rotate_user_agent=True` 可以自動為每個請求隨機切換 User-Agent，減少被網站阻擋的風險。
- **資源優化**: 開啟 `disable_images` 和 `disable_css` 可以顯著提升載入速度並節省頻寬。
- **並發控制**: `semaphore_size` 用於限制同時執行的瀏覽器頁面數量，避免系統資源耗盡。

### 3.2 智能 Markdown 轉換 (Magic Markdown)

`lab-crawl` 提供兩種 HTML 轉 Markdown 的模式：

1. **標準模式**: 保留大部分內容結構。
2. **Magic 模式**: 使用 `readability` 演算法去除雜訊 (如導航列、廣告、頁尾)，只保留主要文章內容。

```python
# 開啟 Magic Markdown
result = crawler.crawl("https://news.ycombinator.com", magic_markdown=True)
```

### 3.3 輕量化爬取 (Lightweight Crawling)

若不需要執行 JavaScript，可使用 `run_mode="lite"` 進行純 HTTP 請求爬取，速度更快且資源消耗更低。

```python
# 使用輕量模式 (純 HTTP GET)
result = crawler.crawl("https://www.example.com", run_mode="lite")
```

### 3.4 批次爬取 (Batch Crawling)

對於大量網址，使用 `crawl_many` 可以並行處理，效率更高。

```python
urls = [
    "https://www.google.com",
    "https://www.bing.com",
    "https://duckduckgo.com"
]

# 並行爬取所有網址
results = crawler.crawl_many(urls, magic_markdown=True)

for res in results:
    print(f"URL: {res['url']}, Success: {res['success']}")
```

### 3.5 錯誤處理 (Error Handling)

`lab_crawl` 提供了具體的異常類型，方便進行錯誤分類處理。

```python
import lab_crawl

try:
    crawler.crawl("https://invalid-url")
except lab_crawl.TimeoutError:
    print("請求超時")
except lab_crawl.NavigationError:
    print("導航失敗")
except lab_crawl.BrowserLaunchError:
    print("瀏覽器啟動失敗")
except lab_crawl.CrawlError as e:
    print(f"其他爬取錯誤: {e}")
```

所有的異常都繼承自 `lab_crawl.CrawlError`。

## 4. 返回資料結構

`crawl` 和 `crawl_many` 方法回傳的結果為字典 (Dictionary)，包含以下欄位：

| 欄位名稱 | 類型 | 說明 |
| :--- | :--- | :--- |
| `url` | str | 請求的網址 |
| `success` | bool | 是否爬取成功 |
| `html` | str | 原始 HTML 內容 |
| `markdown` | str | 轉換後的 Markdown 內容 |
| `error_message` | str (Optional) | 錯誤訊息 (若 success 為 False) |

---

**注意**: 請確保在使用完畢後呼叫 `crawler.close()` 以釋放系統資源。
