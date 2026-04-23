# Lab Crawler Platform — Architecture

## 目錄

- [專案概觀](#專案概觀)
- [目錄結構](#目錄結構)
- [服務架構](#服務架構)
- [API 端點](#api-端點)
- [資料流](#資料流)
- [外部依賴](#外部依賴)
- [認證機制](#認證機制)
- [安全設計](#安全設計)
- [環境變數](#環境變數)

---

## 專案概觀

一個以 Rust/Axum 為核心的網路爬蟲平台，整合 Google Custom Search、ArXiv、iTunes Podcast、AI Agent 等資料來源，將爬取結果以 Markdown / JSON / PDF / Audio 格式存入 S3 相容的物件儲存（RustFS）。

**技術棧**

| 層次 | 技術 |
|------|------|
| API Server | Rust 1.88, Axum 0.7, Tokio |
| Frontend | React 18, Vite, Tailwind CSS |
| Task Queue | Redis 7 (RPUSH / BLPOP) |
| Object Storage | RustFS (S3 相容) |
| 爬蟲引擎 | reqwest + html2text，可選 Chromium (chromiumoxide) |
| LLM | OpenAI API (agent 模式) |

---

## 目錄結構

```
lab_crawler_platform_rust/
├── backend-api/                  # Rust/Axum API server
│   ├── src/
│   │   ├── main.rs               # 啟動、Router、Graceful Shutdown
│   │   ├── config.rs             # 環境變數驗證（fail-fast）
│   │   ├── state.rs              # AppState、MetricsState
│   │   ├── worker.rs             # 背景工作者（Redis 消費者）
│   │   ├── api/
│   │   │   ├── crawl.rs          # POST /api/agent-crawl, /api/batch-crawl
│   │   │   ├── search.rs         # POST /api/search-aggregate
│   │   │   ├── arxiv.rs          # POST /api/arxiv-search
│   │   │   ├── podcast.rs        # POST /api/podcast-search
│   │   │   ├── exploration.rs    # POST /api/ai-exploration
│   │   │   ├── general.rs        # GET /, /healthz, /api/metrics
│   │   │   └── storage.rs        # GET /api/storage-stats, POST /api/storage/delete
│   │   ├── services/
│   │   │   ├── mod.rs            # URL 驗證（SSRF 防護）
│   │   │   ├── crawler.rs        # CrawlerRequest、HTTP 爬取、域名節流
│   │   │   ├── queue.rs          # Redis 佇列封裝
│   │   │   └── s3.rs             # RustFS 上傳、Bucket 管理
│   │   └── middleware/
│   │       └── auth.rs           # Bearer Token 驗證
│   ├── Cargo.toml
│   └── Dockerfile
│
├── crawl/                        # 爬蟲 library（可選 Chromium）
│   ├── src/
│   │   ├── crawler.rs            # AsyncWebCrawler 主協調器
│   │   ├── strategies/
│   │   │   ├── http.rs           # 純 HTTP 爬取 + Readability
│   │   │   ├── browser.rs        # BrowserPool (chromiumoxide)
│   │   │   ├── agent.rs          # LLM Agent 控制
│   │   │   └── markdown.rs       # HTML → Markdown 轉換
│   │   └── python_binding.rs     # PyO3 Python 綁定
│   └── Dockerfile
│
├── frontend/                     # React SPA
│   ├── src/
│   │   ├── App.jsx               # 根元件、任務狀態管理
│   │   ├── api.js                # apiFetch wrapper（auth 由 nginx 注入）
│   │   └── components/
│   │       ├── TaskForm.jsx       # 多策略表單（Search/Crawl/ArXiv/Podcast/Explore）
│   │       ├── DataStorage.jsx    # 檔案瀏覽器（Presigned URL）
│   │       ├── MetricsPanel.jsx   # 即時指標
│   │       ├── SearchAggregator.jsx
│   │       ├── TaskList.jsx
│   │       └── DashboardTabs.jsx
│   ├── nginx.conf.template        # Nginx 反向代理 + auth 注入
│   ├── docker-entrypoint.sh       # envsubst 替換 ${API_KEY}
│   └── Dockerfile
│
├── rustfs/                       # S3 相容物件儲存
│   └── Dockerfile                # 下載預建 RustFS binary
│
├── docker-compose.yml
├── .env.example
└── build.sh
```

---

## 服務架構

```
Browser
  │
  │ HTTP :5173
  ▼
┌─────────────────────┐
│  Frontend (Nginx)   │  React SPA，靜態檔案
│                     │  注入 Authorization: Bearer ${API_KEY}
└────────┬────────────┘
         │ HTTP :8000 (proxy_pass)
         ▼
┌─────────────────────┐
│  Backend API        │  Axum Router
│  (Rust/Tokio)       │  Auth Middleware → Handlers
│                     │
│  ┌───────────────┐  │     ┌──────────────┐
│  │  HTTP Worker  │◄─┼─────│  Redis :6379  │
│  │  (background) │  │     │  crawl_queue  │
│  └───────────────┘  │     └──────────────┘
└────────┬────────────┘
         │ S3 API :9000
         ▼
┌─────────────────────┐
│  RustFS             │  S3 相容物件儲存
│  :9000 (API)        │  markdown / JSON / PDF / audio
│  :9001 (Console)    │
└─────────────────────┘

External:
  OpenAI API     — Agent 模式 LLM 推論
  Google CSE     — 搜尋聚合
  ArXiv API      — 學術論文
  iTunes API     — Podcast 搜尋
```

---

## API 端點

所有 `/api/*` 端點需要 `Authorization: Bearer <API_KEY>`，並受 200 並發上限保護。

| 方法 | 路徑 | 說明 |
|------|------|------|
| GET  | `/` | Health check |
| GET  | `/healthz` | Redis + S3 connectivity check |
| GET  | `/api/metrics` | 即時指標（queue_size, active_workers, avg_latency_ms） |
| POST | `/api/search-aggregate` | Google Custom Search + 批次爬取，支援多關鍵字 / 日期窗口分批 |
| POST | `/api/arxiv-search` | ArXiv 論文搜尋，下載 PDF 至 S3 |
| POST | `/api/podcast-search` | iTunes 搜尋 + RSS 解析，下載音訊至 S3 |
| POST | `/api/agent-crawl` | 單一 URL 爬取，可接 LLM Prompt（gpt-5.4-mini） |
| POST | `/api/batch-crawl` | 多 URL 批次爬取，`sync: false` 轉非同步佇列 |
| POST | `/api/ai-exploration` | 多頁跟隨連結爬取（同域內） |
| GET  | `/api/storage-stats` | 列出 RustFS 所有 Bucket 及檔案統計 |
| POST | `/api/storage/delete` | 刪除指定 Bucket |

### 請求範例

**search-aggregate**
```json
{
  "keywords": ["LLM", "Transformer"],
  "num_results": 200,
  "time_limit": "m3",
  "target_website": true,
  "job_id": "my-search-job"
}
```

**batch-crawl**
```json
{
  "urls": ["https://example.com/a", "https://example.com/b"],
  "sync": false,
  "job_id": "my-batch-job",
  "ignore_links": true
}
```

---

## 資料流

### A. 同步爬取（agent-crawl）

```
前端 → POST /api/agent-crawl
         │
         ├─ 1. Auth Middleware（Bearer Token 驗證）
         ├─ 2. validate_url()（SSRF 防護，DNS 解析 + IP 白名單檢查）
         ├─ 3. call_crawler_service()
         │       ├─ per-domain throttle（同 host 間隔 ≥ 1s）
         │       ├─ HTTP GET（timeout 60s，含 redirect 跟隨）
         │       ├─ HTML → Markdown（html2text + Readability）
         │       └─ 若有 prompt → OpenAI Chat Completions API
         ├─ 4. save_to_rustfs_content()
         │       ├─ 樂觀 put_object（bucket 已存在時跳過 create）
         │       ├─ NoSuchBucket → create_bucket（30s timeout）
         │       └─ 全抖動指數退避重試（最多 3 次，ceiling 8s）
         └─ 5. 回傳 {results: [{url, success, markdown, s3_bucket, s3_path}]}
```

### B. 非同步批次爬取（batch-crawl, sync: false）

```
前端 → POST /api/batch-crawl
         │
         ├─ 1. 驗證所有 URL（buffer_unordered(8) 並發）
         ├─ 2. RPUSH crawl_queue <task_json>
         └─ 3. 立即回傳 {task_enqueued: true}

Worker（背景）:
  BLPOP crawl_queue 5s
    ├─ call_crawler_service()
    ├─ 對每個成功結果存 S3 (.md)
    └─ 存 summary.json
```

### C. 搜尋聚合（search-aggregate）

```
前端 → POST /api/search-aggregate {keywords, num_results}
         │
         ├─ 對每個 keyword（動態剩餘配額，基於 seen_urls.len() 非 raw hits）:
         │    └─ 若 num_results > 100：切分最多 5 個 180 天日期窗口
         │         每窗口 Google CSE 最多 100 筆（start ≤ 91）
         │         → 單 keyword 最多 500 筆
         │
         ├─ 所有 URL 去重（HashSet 即時去重，邊抓邊去）
         ├─ call_crawler_service() 批次爬取所有 URL
         └─ 回傳 aggregated results + 存 S3
```

### D. 關鍵字配額分配邏輯

```rust
// 每個 keyword 的配額 = 剩餘總量（非固定預分配）
// 保證稀疏關鍵字不浪費後續關鍵字的配額
let per_keyword_limit = (request.num_results - seen_urls.len() as i32)
    .min(100 * MAX_DATE_BATCHES);  // 最多 500/keyword
```

---

## 外部依賴

| 服務 | 用途 | 配置方式 |
|------|------|----------|
| Redis 7 | 任務佇列（RPUSH/BLPOP） | `REDIS_URL` |
| RustFS | S3 相容物件儲存 | `RUSTFS_ENDPOINT`, `RUSTFS_ACCESS_KEY`, `RUSTFS_SECRET_KEY` |
| OpenAI API | LLM 推論（Agent 模式） | `OPENAI_API_KEY` |
| Google Custom Search | 網頁搜尋 | `GOOGLE_API_KEY`, `GOOGLE_CX` |
| ArXiv API | 學術論文（公開，無需金鑰） | — |
| iTunes Search API | Podcast（公開，無需金鑰） | — |

---

## 認證機制

**方式**：Bearer Token（靜態 API Key）

```
使用者 → 瀏覽器
           │
           │（不含任何 key，純 SPA）
           ▼
        Nginx（前端 container）
           │
           │ 注入 Authorization: Bearer ${API_KEY}
           ▼
        Backend API
           │
           ├─ middleware/auth.rs 驗證 token
           ├─ 成功 → next.run(request)
           └─ 失敗 → 401 Unauthorized
```

- **生產環境**：Nginx `proxy_set_header` 在伺服器端注入，key 不進入瀏覽器 bundle
- **開發環境**：Vite `server.proxy` configure callback 注入（`vite.config.js`）
- **CORS**：限定 `ALLOWED_ORIGINS`（逗號分隔），不接受 wildcard

---

## 安全設計

### SSRF 防護（`services/mod.rs`）

爬取前對所有使用者提供的 URL 執行：

1. 拒絕非 `http`/`https` scheme
2. 解析 hostname → DNS 查詢所有 A/AAAA record
3. 拒絕以下 IP 範圍：

   | 範圍 | 說明 |
   |------|------|
   | `127.0.0.0/8` | Loopback |
   | `10.0.0.0/8` | Private Class A |
   | `172.16.0.0/12` | Private Class B |
   | `192.168.0.0/16` | Private Class C |
   | `169.254.0.0/16` | Link-local |
   | `::1` | IPv6 Loopback |
   | `fc00::/7` | IPv6 Private |

4. DNS 查詢並發上限：`buffer_unordered(8)`，每次查詢 5s timeout

### 域名節流（`services/crawler.rs`）

- 每個 host 最短間隔 1 秒（slot reservation pattern）
- 閒置 60 秒後自動從 map 清除（Lazy eviction，超過 512 個 entry 觸發）

### 其他

| 機制 | 說明 |
|------|------|
| 並發上限 | 保護路由最多 200 個並發請求，超過立即回 503 |
| Graceful Shutdown | 兩階段：等待進行中任務完成 → 10s 超時強制退出 |
| Mutex Poison Recovery | `lock().unwrap_or_else(|e| e.into_inner())` |
| S3 重試 | 全抖動指數退避，ceiling 8s，最多 3 次 |
| 請求超時 | HTTP 爬取 60s end-to-end（含 redirect 跟隨 + body 讀取） |
| Nginx proxy timeout | `proxy_read_timeout 600s`（大量爬取不 504） |

---

## 環境變數

```bash
# 認證
API_KEY=your-secret-key

# CORS（逗號分隔）
ALLOWED_ORIGINS=http://localhost:5173

# Redis
REDIS_URL=redis://redis:6379/0

# RustFS
RUSTFS_ACCESS_KEY=rustfsadmin
RUSTFS_SECRET_KEY=rustfsadmin
RUSTFS_ENDPOINT=http://rustfs:9000          # 容器內部通訊
RUSTFS_PUBLIC_ENDPOINT=http://localhost:9000 # Presigned URL 基礎路徑

# 外部 API
OPENAI_API_KEY=sk-...
GOOGLE_API_KEY=AIza...
GOOGLE_CX=your-cx-id
```

---

## 核心 Rust 模組

| 模組 | 職責 |
|------|------|
| `main.rs` | Router 組裝、middleware stack、graceful shutdown 兩階段排水 |
| `config.rs` | 啟動時驗證所有必要環境變數，缺少即 `exit(1)` |
| `state.rs` | 共享狀態：S3 clients × 2（內部/公開）、Redis pool、API key、domain throttle map、metrics |
| `worker.rs` | 背景 loop：BLPOP → crawl → S3，shutdown channel 監聽 |
| `services/mod.rs` | SSRF URL 驗證，`validate_urls()` 批次並發 |
| `services/crawler.rs` | HTTP 爬取、redirect 跟隨、域名節流、OpenAI 呼叫、Markdown 轉換 |
| `services/s3.rs` | 樂觀 put → bucket 建立 → 重試，Presigned URL 生成，bucket name sanitize |
| `services/queue.rs` | Redis RPUSH / BLPOP / PING 封裝 |
| `middleware/auth.rs` | Bearer token 比對，失敗回 401 |
