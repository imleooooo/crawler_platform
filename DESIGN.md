# Lab Crawler Platform — 設計理念

## 這個平台是什麼

一個為 **LLM 資料準備**而建的網路爬蟲平台。

LLM 的能力上限由訓練資料決定。要讓模型理解特定領域（技術文章、學術論文、社群討論、Podcast 逐字稿），必須能夠大量、穩定、有組織地取得這些內容，並轉換成模型可以直接消費的格式。這個平台的每一個設計決策，都圍繞這個核心目標展開。

---

## 設計目標

### 1. 最終產物是 Markdown，不是 HTML

LLM 處理的是自然語言文字，不是 DOM 結構。HTML 中充滿了導覽列、廣告、JavaScript、樣式標籤，這些對模型是雜訊。

平台在爬取後會立即執行 HTML → Markdown 的轉換（`html2text` + Readability 算法），最終存入 S3 的是乾淨的 Markdown 文字，而不是原始 HTML。這代表：

- 訓練資料可以直接餵給模型
- 儲存空間遠小於原始 HTML
- 人工審查時可讀性高

### 2. 多來源的統一格式

LLM 需要多樣化的訓練資料。平台整合了多個性質截然不同的資料來源，全部輸出統一的 `{url, title, published_at, markdown}` 結構：

| 來源 | 內容類型 | 用途 |
|------|----------|------|
| Google Custom Search | 任意網頁文章 | 通用領域語料 |
| ArXiv | 學術論文 PDF | 技術/科研語料 |
| iTunes + RSS | Podcast 音訊 | 口語/對話語料 |
| 直接爬取 URL | 任意頁面 | 指定頁面精準採集 |
| AI Exploration | 同域多頁跟隨 | 深度主題爬取 |

統一格式讓下游的資料處理 pipeline 不需要針對每個來源特別處理。

### 3. 資料以「任務」為單位組織

每個爬取任務對應 S3 中的一個獨立 Bucket（`job_id` 或自動生成）。這讓研究者可以：

- 按主題管理資料集（`ml-papers-2024`、`techcrunch-ai`）
- 選擇性地納入或排除特定資料集
- 精確追蹤每個 Markdown 文件的來源 URL

```
s3://ml-papers-2024/
  ├── paper_0_abc123.pdf
  ├── crawl_0_a1b2c3d4.md      ← 論文全文 Markdown
  ├── crawl_1_e5f6a7b8.md
  └── summary.json             ← 整個任務的 metadata
```

### 4. Agent 模式：LLM 輔助 LLM 資料準備

平台支援 Agent 模式，讓一個 LLM（GPT）協助準備另一個 LLM 的訓練資料。爬取後的 Markdown 可以直接作為 context，搭配 prompt 進行：

- 摘要提取（從 10,000 字文章提取核心段落）
- 結構化轉換（將文章轉成 Q&A 對話格式）
- 相關性過濾（判斷是否與主題相關）

這在 RAG（Retrieval-Augmented Generation）的資料準備階段特別有用。

---

## 資料流的四個階段

```
Stage 1: Discovery（發現）
  使用者提供關鍵字或 URL 清單
  Google CSE 搜尋 → 候選 URL 集合
  支援 date window 分批，打破 100 筆/keyword 的 API 限制

        ↓

Stage 2: Acquisition（取得）
  對每個 URL 發送 HTTP 請求
  - 域名節流：同一 host 間隔 ≥ 1s（避免被 ban，也是對站方的尊重）
  - Redirect 跟隨：每 hop 都節流，整條 chain 60s deadline
  - SSRF 防護：URL 驗證，拒絕內部位址

        ↓

Stage 3: Transformation（轉換）
  HTML → Markdown（html2text + Readability）
  提取結構化 metadata：title、published_at
  可選：Agent 模式，由 LLM 進一步處理

        ↓

Stage 4: Storage（儲存）
  Markdown + JSON 存入 RustFS（S3 相容）
  按 job_id 組織到獨立 Bucket
  Presigned URL 讓前端/下游系統直接存取，不經過後端
```

---

## 為什麼這樣設計

### 同步 vs. 非同步任務

單一頁面爬取（`agent-crawl`）幾秒完成，同步回應合理。但「爬 500 個 URL」可能耗時數分鐘，HTTP 請求無法無限等待。

**設計**：大量任務走 Redis 佇列，立即回傳 `task_enqueued: true`，背景 worker 異步執行。這讓 API 保持低延遲，使用者的研究工作流不被阻塞。

### 為什麼要域名節流

爬蟲平台如果不控制頻率，很容易因為 rate limit 或 ban 而失去資料來源。更重要的是，大量並發請求對目標站點是不公平的負擔。

Slot Reservation 設計確保：N 個並發對同一 domain 的請求，會自動排成 0s、1s、2s、... 的間隔，而不是同時打出。

```rust
// 在 lock 內預佔時間槽，而非記錄「上次發送時間」
let next_avail = map.get(&domain).copied().unwrap_or(now);
let sleep = next_avail.saturating_duration_since(now);
map.insert(domain, now.max(next_avail) + Duration::from_secs(1));
```

### 為什麼用 RustFS 而非雲端 S3

訓練資料可能包含大量版權內容或尚未公開的研究資料，不適合存放在第三方雲端。自架 RustFS 提供 S3 相容 API，讓現有的 `aws-sdk-s3` 工具鏈直接可用，同時資料完全自控。

### 為什麼要 Markdown 而非直接存 HTML

|  | HTML | Markdown |
|--|------|----------|
| Token 消耗 | 高（充滿 HTML tag） | 低（純文字） |
| 模型理解 | 差（需要學習 HTML 語法） | 好（接近自然語言） |
| 人工審查 | 困難 | 容易 |
| 儲存大小 | 大（原始 HTML） | 小（50-80% 壓縮） |

對 LLM fine-tuning 和 RAG 來說，Markdown 是實質上的標準格式。

---

## 核心技術挑戰與解法

### Google CSE 每 keyword 100 筆上限

Google Custom Search API 的 `start` 參數最大 91，造成單一 keyword 硬限 100 筆。對需要數百筆樣本的資料集來說不夠用。

**解法**：對同一 keyword 切割不重疊的 6 個月日期窗口，每窗口各取最多 100 筆。5 個窗口 = 單 keyword 最多 500 筆，涵蓋近 2.5 年的內容。

```
keyword "transformer architecture"，num_results=300：
  窗口 0：最近 180 天         → 最多 100 筆
  窗口 1：180~360 天前        → 最多 100 筆
  窗口 2：360~540 天前        → 最多 100 筆
  共最多 300 筆，橫跨 18 個月的內容演進
```

有設定 `time_limit` 時跳過窗口分批，避免日期條件衝突。

### 跨 Keyword 的 URL 重複問題

多個 keyword 可能指向相同的文章（例如「LLM」和「Large Language Model」都可能找到同一篇論文）。如果以 raw hit count 計算配額，重複 URL 會用盡預算，後續 keyword 被截斷。

**解法**：全程用 `HashSet<String>` 追蹤已見 URL，配額基於 **unique URL 數量** 而非 raw hit count。這同時解決了三個問題：

1. 重複 URL 不佔用配額
2. 稀疏 keyword 不浪費後續 keyword 的份額
3. 最終結果不需要另外做 dedup

```rust
let mut seen_urls: HashSet<String> = HashSet::new();

for keyword in &request.keywords {
    // 剩餘配額 = 目標總量 - 目前已有的 unique URL 數
    let per_keyword_limit = (num_results - seen_urls.len() as i32).min(500);
    // 插入自動去重
    seen_urls.insert(link);
}
```

### S3 的 Eventual Consistency 與重試設計

Bucket 建立後不是立即可用，`put_object` 可能在 `create_bucket` 成功後數百毫秒內仍回傳 `NoSuchBucket`。

**設計**：樂觀先嘗試 `put_object`（大多數情況 Bucket 已存在），失敗才建立 Bucket，然後以 Full-Jitter Backoff 重試。Jitter 確保多個 worker 同時失敗時不會同步重試造成雪崩。

### SSRF：爬蟲本身就是攻擊向量

爬蟲的核心功能是「訪問使用者指定的 URL」，這天然形成 SSRF 攻擊面。攻擊者可以提交 `http://169.254.169.254/`（雲端 IMDS）或 `http://redis:6379/` 讀取內部服務。

**設計**：在所有 handler 的上游設一道統一的 URL 驗證關卡，包含 scheme 白名單、literal IP 檢查、DNS 解析後的 IP 範圍檢查。注意：DNS rebinding 仍可在通過驗證後繞過，這個檢查是 defence-in-depth 的第一層，不是完整防護。

---

## 系統邊界設計

```
使用者（研究者）
    │
    │  瀏覽器不持有任何 secret
    │  API Key 由 Nginx 在伺服器端注入
    ▼
Frontend（Nginx + React SPA）
    │
    │  Authorization: Bearer ${API_KEY}  ← 在此注入
    ▼
Backend API（Rust/Axum）
    │
    ├── 同步任務 ──────────────────────────────────────────────────────┐
    │   fetch → transform → S3 → 回傳結果                              │
    │                                                                   │
    └── 非同步任務 ──────────────────┐                                 │
        立即回傳 task_enqueued: true  │                                 │
                                     ▼                                  │
                              Redis Queue                               │
                                     │                                  │
                              Background Worker                         │
                              fetch → transform → S3                   │
                                                                        ▼
                                                                  RustFS (S3)
                                                                  Presigned URL
                                                                        │
                                                                        ▼
                                                              下游 LLM Pipeline
                                                              (fine-tuning / RAG)
```

**核心原則**：後端是資料轉換的執行者，不是儲存中繼站。結果直接寫入 S3，前端透過 Presigned URL 直接存取，後端不持有也不傳遞大型資料。

---

## 設計上的取捨

| 決策 | 選擇 | 放棄 | 原因 |
|------|------|------|------|
| 任務佇列 | Redis RPUSH/BLPOP | Kafka、RabbitMQ | 不需要 ACK，足夠簡單；資料集的爬蟲任務容許重試 |
| 物件儲存 | 自架 RustFS | AWS S3 | 資料自控；訓練資料可能含有版權內容 |
| HTML 轉換 | html2text + Readability | 保留原始 HTML | 目標消費者是 LLM，Markdown > HTML |
| 認證 | 單一靜態 Bearer Token | OAuth、JWT | Lab 環境，研究者自用，複雜認證是過度設計 |
| 爬蟲執行 | 原生 HTTP（reqwest） | Headless Browser | 速度快、資源省；需要 JS 渲染的場景用 Agent 模式補足 |
| 語言 | Rust | Python、Go | I/O 密集的爬蟲工作適合 async；型別系統減少 runtime 錯誤 |
