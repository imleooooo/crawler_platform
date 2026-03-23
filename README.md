# Lab Crawler Platform

> 基於 Rust 建構的智慧型網頁爬取與內容聚合平台

---

## 目錄

- [專案動機](#專案動機)
- [系統架構](#系統架構)
- [技術選型](#技術選型)
- [元件說明](#元件說明)
- [使用方案](#使用方案)
- [API 規格](#api-規格)
- [部署與執行](#部署與執行)
- [預計結果](#預計結果)
- [已知限制與未來規劃](#已知限制與未來規劃)

---

## 專案動機

### 背景

在 AI 應用快速發展的今天，高品質的結構化資料是訓練模型、研究分析、知識管理的核心需求。然而，網頁內容的獲取面臨幾個主要挑戰：

- **反爬蟲機制**：現代網站廣泛部署 Bot 偵測、Cloudflare 等防護措施
- **JavaScript 渲染**：大量網站依賴 SPA 動態渲染，純 HTTP 請求無法取得完整內容
- **格式不一致**：不同來源（網頁、論文、Podcast）資料格式各異，難以統一處理
- **規模化問題**：批次處理數百至數千個 URL 時，效能與穩定性難以兼顧
- **AI 整合需求**：需要 LLM 直接理解並操作網頁，執行複雜的資料擷取任務

### 解決方案

Lab Crawler Platform 設計為一個**統一的內容聚合中台**，以 Rust 為核心，提供：

1. **多策略爬取**：根據目標網站特性自動或手動選擇最適爬取方式
2. **AI 驅動操作**：讓 LLM 以自然語言指令操作瀏覽器，應對複雜互動場景
3. **非同步任務佇列**：大量任務不阻塞前端，後台持續消化處理
4. **標準化輸出**：所有來源的內容統一轉換為 Markdown，適合 LLM 後續處理
5. **多來源聚合**：一個平台整合 Google Search、ArXiv、iTunes Podcast 及自訂 URL

---

## 系統架構

```
┌─────────────────────────────────────────────────────────────┐
│                        Frontend (React)                      │
│  ┌─────────┐ ┌──────────┐ ┌──────────┐ ┌───────────────┐   │
│  │TaskForm │ │MetricsPanel│ │DataStorage│ │SearchAggregator│  │
│  └────┬────┘ └────┬─────┘ └────┬─────┘ └───────┬───────┘   │
└───────┼───────────┼────────────┼────────────────┼───────────┘
        │           │            │                │
        ▼           ▼            ▼                ▼
┌─────────────────────────────────────────────────────────────┐
│                    Backend API (Rust/Axum)                   │
│                                                              │
│  POST /api/batch-crawl      POST /api/agent-crawl            │
│  POST /api/search-aggregate POST /api/arxiv-search           │
│  POST /api/podcast-search   POST /api/ai-exploration         │
│  GET  /api/metrics          GET  /api/storage-stats          │
│                                                              │
│  ┌──────────────┐    ┌──────────────┐    ┌───────────────┐  │
│  │ CrawlerService│    │  QueueService│    │   S3 Service  │  │
│  └──────┬───────┘    └──────┬───────┘    └──────┬────────┘  │
│         │                  │                    │            │
│  ┌──────▼────────────────────────────────────── ▼ ────────┐ │
│  │              Background Worker (Tokio)                  │ │
│  └─────────────────────────────────────────────────────────┘ │
└────────┬────────────────────┬────────────────────────────────┘
         │                    │
         ▼                    ▼
┌────────────────┐   ┌──────────────────┐   ┌────────────────┐
│  Crawl Library │   │  Redis (Queue)   │   │ RustFS (S3)    │
│  (Rust + PyO3) │   │                  │   │                │
│                │   └──────────────────┘   │  Crawled Data  │
│  ┌───────────┐ │                          │  (Markdown,    │
│  │ Browser   │ │                          │   JSON, etc.)  │
│  │ Strategy  │ │                          └────────────────┘
│  ├───────────┤ │
│  │ HTTP      │ │
│  │ Strategy  │ │
│  ├───────────┤ │
│  │ Agent     │ │
│  │ Strategy  │ │
│  └───────────┘ │
└────────────────┘
```

### 資料流

#### 同步模式（立即返回結果）
```
使用者填表 → POST /api/agent-crawl
           → CrawlerService 執行爬取
           → 儲存至 S3
           → 回傳結果 JSON → 前端顯示
```

#### 非同步模式（大批量任務）
```
使用者填表 → POST /api/batch-crawl (sync=false)
           → 推入 Redis Queue
           → 回傳 Job ID（立即）

Background Worker（持續運行）
           → 從 Redis 取出任務
           → 並發爬取（最多 10 個 URL 同時）
           → 儲存至 S3
           → 任務完成
```

---

## 技術選型

| 層次 | 技術 | 選擇原因 |
|------|------|---------|
| 後端語言 | **Rust** | 記憶體安全、高效能、零成本抽象，適合長時間運行的爬蟲服務 |
| Web 框架 | **Axum** | 基於 Tokio 的非同步框架，與 Rust 生態整合良好 |
| 非同步執行 | **Tokio** | Rust 最成熟的非同步執行環境，支援高並發 I/O |
| 瀏覽器自動化 | **Chromiumoxide** | Chrome DevTools Protocol 原生實現，無需 WebDriver |
| HTTP 客戶端 | **Reqwest** | 功能完整、支援非同步、TLS，為輕量爬取首選 |
| 任務佇列 | **Redis** | 低延遲、持久化、支援 FIFO 佇列語義 |
| 物件儲存 | **RustFS** | S3 相容、自托管，開發階段替代 AWS S3 |
| Python 綁定 | **PyO3** | 讓 Rust 爬蟲庫可作為 Python 套件使用 |
| 前端框架 | **React 19** | 現代 React 特性（并發模式），搭配 Vite 快速開發 |
| 前端樣式 | **Tailwind CSS** | Utility-first，快速建構響應式介面 |
| 容器化 | **Docker Compose** | 一鍵啟動完整服務棧（API、Redis、Storage、前端） |

---

## 元件說明

### 1. Backend API (`backend-api/`)

Rust/Axum REST API 伺服器，系統核心協調層。

**職責**：
- 接收前端請求，分派至對應服務
- 管理 Redis 任務佇列
- 協調 S3 儲存操作
- 背景 Worker 持續消化非同步任務
- 即時指標回報（佇列大小、Worker 數量、平均延遲）

**關鍵模組**：

| 檔案 | 職責 |
|------|------|
| `src/main.rs` | 伺服器初始化、路由設定、環境變數載入 |
| `src/worker.rs` | 無限迴圈 Worker，從 Redis 取任務並執行 |
| `src/state.rs` | 共享應用狀態（S3 Client、指標、Queue） |
| `src/services/crawler.rs` | 核心爬取邏輯、OpenAI API 整合 |
| `src/services/queue.rs` | Redis FIFO 佇列封裝 |
| `src/services/s3.rs` | S3 上傳、預簽 URL、Bucket 管理 |
| `src/api/crawl.rs` | Agent Crawl、Batch Crawl 端點 |
| `src/api/search.rs` | Google Custom Search 聚合端點 |
| `src/api/arxiv.rs` | ArXiv 學術論文搜尋端點 |
| `src/api/podcast.rs` | iTunes Podcast 搜尋端點 |
| `src/api/exploration.rs` | 遞迴 URL 探索端點 |
| `src/api/storage.rs` | S3 統計與刪除端點 |

---

### 2. Crawl Library (`crawl/`)

高效能爬取函式庫，同時支援 Rust 原生呼叫與 Python 套件形式。

**爬取策略**：

| 策略 | 技術 | 適用場景 |
|------|------|---------|
| **HTTP (lite)** | Reqwest | 靜態 HTML 頁面，速度優先 |
| **Browser (default)** | Chromiumoxide (CDP) | 需要 JavaScript 渲染的 SPA |
| **Agent** | Browser + LLM | 需要點擊、填表等複雜互動的頁面 |

**反偵測機制** (`strategies/stealth.rs`)：
- 移除 `navigator.webdriver` 特徵
- 模擬 Chrome Runtime API
- 偽造瀏覽器 Plugin 資訊
- User Agent 輪換

**Browser Pool**：
- 管理多個 Chromium 實例
- 基於 Semaphore 的並發控制
- 實例生命週期管理

**Python 綁定**：
- PyO3 實現 FFI 介面
- 主要類別：`Crawl4AiRs`
- 方法：`new()`、`crawl()`、`crawl_many()`、`close()`
- 支援透過 Maturin 打包為跨平台 Python Wheel

---

### 3. Frontend (`frontend/`)

React 19 + TypeScript SPA，透過 Vite 建構，Tailwind CSS 樣式。

**主要頁面元件**：

| 元件 | 功能 |
|------|------|
| `TaskForm` | 任務建立表單，支援 6 種爬取模式切換 |
| `MetricsPanel` | 即時顯示 Worker 數、佇列長度、平均延遲 |
| `DashboardTabs` | 分頁式儀表板（結果 / 任務歷史 / 儲存） |
| `DataStorage` | S3 Bucket 瀏覽器，支援下載與刪除 |
| `SearchAggregator` | Google Search 聚合介面 |
| `StrategyModal` | 爬取策略選擇器 |

**特性**：
- LocalStorage 持久化任務歷史
- 即時指標輪詢更新
- 預簽 URL 支援安全下載
- 響應式佈局（手機 1 欄 / 桌面 3 欄）

---

### 4. RustFS (`rustfs/`)

S3 相容的物件儲存伺服器，開發環境用來取代 AWS S3 / MinIO。

- 以目錄模擬 Bucket 結構
- 實作 S3 API 子集
- Ports：9000（API）、9001（管理介面）

---

## 使用方案

### 方案一：Google Custom Search 聚合爬取

**場景**：研究特定主題，自動搜集並爬取相關網頁內容。

**流程**：
1. 前端選擇「Search」模式
2. 輸入關鍵字（如 `"AI agents", "LLM"`）、結果數量、時間範圍
3. 可選擇限定特定網站（`site:github.com`）
4. 後端呼叫 Google Custom Search API 取得 URL 清單
5. 逐一爬取並轉換為 Markdown
6. 結果儲存至 S3，前端顯示可下載連結

```json
POST /api/search-aggregate
{
  "keywords": ["AI agents"],
  "num_results": 10,
  "time_limit": "1y",
  "site": "github.com"
}
```

---

### 方案二：批次 URL 爬取（非同步）

**場景**：擁有大量 URL 清單，需要批次爬取並長期保存。

**流程**：
1. 前端選擇「Batch」模式，上傳或貼入 URL 清單
2. 選擇爬取模式（lite / default / agent）
3. 設定 `sync=false` 非同步執行
4. 後端將任務推入 Redis 佇列，立即回傳 Job ID
5. Background Worker 並發處理（最多 10 個 URL 同時）
6. 所有結果上傳至 S3 的專屬 Bucket
7. 前端可透過 Storage 頁面瀏覽下載

```json
POST /api/batch-crawl
{
  "urls": ["https://url1.com", "https://url2.com", "..."],
  "run_mode": "lite",
  "sync": false,
  "job_id": "batch-20240127"
}
```

---

### 方案三：AI Agent 爬取

**場景**：目標頁面需要登入、點擊、填表等複雜互動才能取得資料。

**流程**：
1. 前端選擇「Agent」模式
2. 輸入目標 URL 與自然語言指令（如 `"提取所有產品的價格與名稱"`）
3. 提供 OpenAI API Key 與模型選擇
4. 後端啟動 Browser，LLM 逐步推理並執行操作（最多 10 步）
5. 操作序列：`goto`、`click`、`type`、`scroll`、`done`、`fail`
6. 最終提取頁面內容轉為 Markdown

```json
POST /api/agent-crawl
{
  "url": "https://example.com/pricing",
  "prompt": "Extract all pricing tiers and their features",
  "api_key": "sk-...",
  "model": "gpt-4o"
}
```

---

### 方案四：ArXiv 學術論文搜尋

**場景**：研究人員需要自動收集特定領域的最新論文。

**流程**：
1. 前端選擇「ArXiv」模式
2. 輸入搜尋關鍵字、年份、數量限制
3. 後端呼叫 ArXiv API 取得論文元資料
4. 自動下載 PDF 連結並解析內容
5. 結果包含：標題、作者、發表日期、PDF URL

```json
POST /api/arxiv-search
{
  "keywords": "federated learning privacy",
  "year": "2024",
  "limit": 20
}
```

---

### 方案五：AI 探索模式

**場景**：從一個入口 URL 開始，自動探索並爬取相關連結。

**流程**：
1. 輸入起始 URL 與探索深度限制
2. 爬取初始頁面，提取所有連結
3. 選擇下一個最相關連結繼續爬取
4. 重複至達到深度限制
5. 回傳探索路徑上所有頁面的內容

```json
POST /api/ai-exploration
{
  "url": "https://docs.example.com/",
  "limit": 5
}
```

---

### 方案六：Python 套件整合

**場景**：在現有 Python 資料管線中使用高效能 Rust 爬蟲。

```python
from lab_crawl import Crawl4AiRs
import asyncio

async def main():
    crawler = Crawl4AiRs()

    # 單一 URL 爬取
    result = await crawler.crawl("https://example.com")
    print(result.markdown)

    # 批次爬取
    results = await crawler.crawl_many([
        "https://url1.com",
        "https://url2.com"
    ])

    await crawler.close()

asyncio.run(main())
```

---

## API 規格

### 端點總覽

| 方法 | 路徑 | 說明 |
|------|------|------|
| `GET` | `/` | 健康檢查 |
| `GET` | `/api/metrics` | 系統即時指標 |
| `POST` | `/api/search-aggregate` | Google Custom Search 聚合 |
| `POST` | `/api/arxiv-search` | ArXiv 論文搜尋 |
| `POST` | `/api/podcast-search` | iTunes Podcast 搜尋 |
| `POST` | `/api/ai-exploration` | 遞迴 URL 探索 |
| `POST` | `/api/agent-crawl` | AI Agent 單頁爬取 |
| `POST` | `/api/batch-crawl` | 批次 URL 爬取 |
| `GET` | `/api/storage-stats` | S3 儲存統計 |
| `POST` | `/api/storage/delete` | 刪除任務資料 |

### 指標回應範例

```json
GET /api/metrics
{
  "queue_size": 5,
  "active_workers": 2,
  "avg_latency": 2.45
}
```

### 儲存統計回應範例

```json
GET /api/storage-stats
{
  "total_files": 150,
  "total_size_bytes": 10240000,
  "total_size_display": "9.8 MB",
  "recent_files": [
    {
      "name": "crawl_0_abc123.md",
      "bucket": "agent-job-20240127",
      "size": 52400,
      "type": "MD",
      "url": "http://localhost:9000/agent-.../crawl_0_abc123.md",
      "last_modified": "2024-01-27T10:00:00Z"
    }
  ]
}
```

---

## 部署與執行

### 環境需求

- Docker & Docker Compose
- （可選）Rust toolchain（本地開發）
- （可選）Python 3.11 + Maturin（Python Wheel 建構）

### 環境變數設定（`.env`）

在專案根目錄（與 `docker-compose.yml` 同層）建立 `.env`：

```env
# API 存取金鑰（自訂任意字串；前端透過 nginx/Vite proxy 傳遞，不會暴露於 bundle）
API_KEY=your-secret-key

# OpenAI（Agent 模式必填）
OPENAI_API_KEY=sk-...

# Google Custom Search
GOOGLE_API_KEY=AIzaSy...
GOOGLE_CX=378c5563679a...

# RustFS（S3 相容儲存）
RUSTFS_ACCESS_KEY=rustfsadmin
RUSTFS_SECRET_KEY=rustfsadmin
RUSTFS_ENDPOINT=http://rustfs:9000
RUSTFS_PUBLIC_ENDPOINT=http://localhost:9000

# CORS（前端 origin，多個以逗號分隔）
ALLOWED_ORIGINS=http://localhost:5173
```

### 啟動服務

```bash
# 1. 建構所有 Docker images
docker compose build

# 2. 啟動所有服務
docker compose up -d

# 僅啟動後端（不含前端）
docker compose up -d backend-api redis rustfs
```

### 服務端口

| 服務 | 端口 |
|------|------|
| Frontend (nginx) | `http://localhost:5173` |
| Backend API | `http://localhost:8000` |
| RustFS API | `http://localhost:9000` |
| RustFS Console | `http://localhost:9001` |
| Redis | `localhost:6379` |

### Docker Compose 服務結構

```yaml
services:
  frontend:    # React + nginx（反向代理 /api/*，注入 Bearer token）
  backend-api: # Rust/Axum REST API
  redis:       # Redis Alpine（任務佇列）
  rustfs:      # S3 相容物件儲存
```

### 認證架構

所有 `/api/*` 路由均需 `Authorization: Bearer <API_KEY>`。Bearer token 由基礎設施層注入，**不會出現在前端 JS bundle**：

```
瀏覽器 → nginx（注入 Authorization header）→ backend-api
```

**本地開發（`npm run dev`）**：Vite `server.proxy` 讀取 `.env` 中的 `API_KEY` 並在 Node.js 側注入，行為與 nginx 相同。

---

## 預計結果

### 功能面

| 功能 | 狀態 |
|------|------|
| REST API 伺服器（8+ 端點） | 完成 |
| Google Search 聚合 | 完成 |
| ArXiv 論文搜尋 | 完成 |
| Podcast 搜尋 | 完成 |
| HTTP 爬取策略 | 完成 |
| Browser 爬取策略 | 完成 |
| Agent 爬取策略 | 完成 |
| 反偵測機制 | 完成 |
| Redis 非同步佇列 | 完成 |
| Background Worker | 完成 |
| S3/RustFS 儲存整合 | 完成 |
| Python 套件綁定 | 完成 |
| 前端儀表板 | 完成 |
| 即時指標監控 | 完成 |
| PostgreSQL 資料庫整合 | 設計完成，待實作 |
| API Bearer Token 身份驗證 | 完成 |

### 效能目標

| 指標 | 目標值 |
|------|--------|
| HTTP 爬取（單頁） | < 1 秒 |
| Browser 爬取（單頁，含 JS） | < 5 秒 |
| 並發爬取（batch） | 10 個 URL 同時 |
| 任務佇列延遲 | < 100ms（Redis FIFO） |
| API 響應時間（健康檢查） | < 50ms |

### 輸出格式

所有爬取結果統一輸出為：

```
s3://bucket-name/
  ├── crawl_0_{hash}.md       # 頁面 Markdown 內容
  ├── crawl_1_{hash}.md
  └── job_summary.json        # 任務摘要（URL、狀態、時間）
```

**Markdown 特性**：
- HTML 標籤完全清理
- 保留標題層級、列表、連結結構
- 適合直接餵入 LLM 進行後續分析
- 移除廣告、導覽列等雜訊內容

---

## 已知限制與未來規劃

### 當前限制

1. **資料庫持久化**：任務歷史目前靠前端 LocalStorage 儲存，後端無持久化；PostgreSQL schema 已設計完成，但尚未整合至 API
3. **Agent 模式穩定性**：複雜互動場景下 LLM 推理準確率有待提升
4. **Worker 錯誤恢復**：Worker 崩潰後任務可能遺失，缺乏 Dead Letter Queue
5. **資源限制**：瀏覽器實例數量固定，尚無動態擴縮容機制

### 未來規劃

| 優先級 | 功能 |
|--------|------|
| 高 | PostgreSQL 整合，實現完整任務生命週期管理 |
| 中 | JWT 身份驗證（多用戶支援） |
| 中 | Dead Letter Queue 與失敗任務重試機制 |
| 中 | Webhook 通知（任務完成時推送） |
| 中 | 動態 Browser Pool 擴縮容 |
| 低 | 任務排程（定時爬取） |
| 低 | 多租戶支援 |
| 低 | Prometheus 指標匯出 |

---

## 資料庫設計（規劃中）

```sql
-- 任務表
CREATE TABLE jobs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    type VARCHAR(50) NOT NULL,  -- search, arxiv, podcast, agent, batch, exploration
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    params JSONB NOT NULL,
    s3_bucket VARCHAR(255),
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW(),
    completed_at TIMESTAMP WITH TIME ZONE
);

-- 結果表
CREATE TABLE results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    job_id UUID NOT NULL REFERENCES jobs(id) ON DELETE CASCADE,
    url TEXT NOT NULL,
    title TEXT,
    s3_bucket VARCHAR(255),
    s3_key VARCHAR(500),
    metadata JSONB,
    created_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);

-- 系統指標歷史
CREATE TABLE system_metrics (
    id BIGSERIAL PRIMARY KEY,
    queue_size INT,
    active_workers INT,
    avg_latency FLOAT,
    recorded_at TIMESTAMP WITH TIME ZONE DEFAULT NOW()
);
```

---

*Lab Crawler Platform — 以 Rust 的效能與安全性，建構 AI 時代的內容聚合基礎設施*
