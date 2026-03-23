import { apiFetch } from '../api';
import React, { useState } from 'react';
import { Upload, Download, ChevronRight, Search, FileText, Bot, Eye, Mic, Brain, Plug, Github, Check, Plus, Loader2 } from 'lucide-react';
import StrategyModal from './StrategyModal';

// Strategy definitions
const STRATEGIES = [
    { id: 'search', label: '搜尋聚合', Icon: Search, color: 'bg-blue-500', desc: '多關鍵字批次搜尋並聚合結果' },
    { id: 'textCrawl', label: '純文字/輕量爬取', Icon: FileText, color: 'bg-green-500', desc: '快速擷取網頁文字內容' },
    { id: 'agent', label: '瀏覽器代理人', Icon: Bot, color: 'bg-red-500', desc: 'AI 自動操作瀏覽器執行任務' },
    { id: 'visual', label: '視覺 OCR & PDF', Icon: Eye, color: 'bg-orange-500', desc: '圖片文字辨識與 PDF 解析' },
    { id: 'audio', label: '音訊處理', Icon: Mic, color: 'bg-purple-500', desc: '語音轉文字與音訊分析' },
    { id: 'aiExploration', label: 'AI 探索', Icon: Brain, color: 'bg-cyan-500', desc: 'AI 自動分析頁面結構並爬取' },
    { id: 'api', label: '特定平台 API', Icon: Plug, color: 'bg-indigo-500', desc: 'Reddit、Github 等平台 API' },
    { id: 'download', label: '指定下載功能', Icon: Download, color: 'bg-pink-500', desc: 'Podcast、ArXiv 論文下載' },
];

const TaskForm = ({ setSearchResults, ...props }) => {
    // Strategy selection state
    const [strategies, setStrategies] = useState({
        search: false,
        textCrawl: false,
        agent: false,
        visual: false,
        audio: false,
        aiExploration: false,
        api: false,
        download: false,
    });

    // Modal state
    const [activeModal, setActiveModal] = useState(null);

    // Loading state
    const [isSubmitting, setIsSubmitting] = useState(false);

    // Common state
    const [jobId, setJobId] = useState('');

    // Search Aggregation state
    const [searchTerms, setSearchTerms] = useState('');
    const [numResults, setNumResults] = useState(10);
    const [searchTimeLimit, setSearchTimeLimit] = useState('');
    const [searchSite, setSearchSite] = useState('');
    const [targetWebsite, setTargetWebsite] = useState(false);
    const [ignoreLinks, setIgnoreLinks] = useState(false);

    // Text Crawl state
    const [crawlMode, setCrawlMode] = useState('Prompt');
    const [batchUrls, setBatchUrls] = useState('');
    const [textCrawlUrl, setTextCrawlUrl] = useState('');

    // Agent state
    const [agentPrompt, setAgentPrompt] = useState('');

    // Visual state
    const [visualInputDir, setVisualInputDir] = useState('');
    const [visualModel, setVisualModel] = useState('');

    // Audio state
    const [audioInputDir, setAudioInputDir] = useState('');
    const [audioModel, setAudioModel] = useState('');

    // AI Exploration state
    const [explorationUrl, setExplorationUrl] = useState('');
    const [explorationLimit, setExplorationLimit] = useState(1);

    // API state
    const [apiTypes, setApiTypes] = useState({ reddit: false, github: false });
    const [redditQuery, setRedditQuery] = useState('');
    const [githubQuery, setGithubQuery] = useState('');

    // Download state
    const [downloadTypes, setDownloadTypes] = useState({ podcast: false, arxiv: false });
    const [podcastKeywords, setPodcastKeywords] = useState('');
    const [podcastYear, setPodcastYear] = useState('');
    const [podcastLimit, setPodcastLimit] = useState(5);
    const [arxivKeywords, setArxivKeywords] = useState('');
    const [arxivYear, setArxivYear] = useState('');
    const [arxivLimit, setArxivLimit] = useState(5);

    const openModal = (strategyId) => {
        setActiveModal(strategyId);
    };

    const closeModal = () => {
        setActiveModal(null);
    };

    const confirmStrategy = (strategyId) => {
        setStrategies(prev => ({ ...prev, [strategyId]: true }));
        closeModal();
    };

    const removeStrategy = (strategyId, e) => {
        e.stopPropagation();
        setStrategies(prev => ({ ...prev, [strategyId]: false }));
    };

    const isAnyStrategySelected = Object.values(strategies).some(Boolean);

    // ==================== SUBMIT HANDLERS ====================
    const handleSubmit = async (e) => {
        e.preventDefault();
        if (isSubmitting) return;

        setIsSubmitting(true);
        try {
            if (strategies.search) {
                await handleSearchSubmit();
            } else if (strategies.agent) {
                await handleAgentSubmit();
            } else if (strategies.textCrawl) {
                await handleTextCrawlSubmit();
            } else if (strategies.download && downloadTypes.podcast) {
                await handlePodcastSubmit();
            } else if (strategies.download && downloadTypes.arxiv) {
                await handleArxivSubmit();
            } else if (strategies.aiExploration) {
                await handleAIExplorationSubmit();
            } else {
                alert('請先配置一個策略');
            }
        } finally {
            setIsSubmitting(false);
        }
    };

    // Sanitize bucket name for S3 compatibility: lowercase, alphanumeric + hyphens, 3-63 chars
    const sanitizeBucketName = (name) => {
        let sanitized = name
            .toLowerCase()
            .replace(/[^a-z0-9-]/g, '-')  // Replace invalid chars with hyphen
            .replace(/-+/g, '-')           // Collapse multiple hyphens
            .replace(/^-+|-+$/g, '');      // Trim leading/trailing hyphens

        // Ensure minimum length of 3
        if (sanitized.length < 3) {
            sanitized = sanitized + '-job';
        }
        // Truncate to max 63 chars
        if (sanitized.length > 63) {
            sanitized = sanitized.substring(0, 63).replace(/-+$/g, '');
        }
        return sanitized;
    };

    const createTask = (prompt, strategy) => {
        const rawJobId = jobId.trim() || `job-${Date.now()}-${Math.floor(Math.random() * 1000)}`;
        const finalJobId = sanitizeBucketName(rawJobId);
        const newTask = {
            id: finalJobId,
            prompt,
            strategy,
            status: '進行中',
            time: new Date().toLocaleTimeString([], { hour: '2-digit', minute: '2-digit' }),
            progress: 0,
            results: null
        };
        if (props.addTask) props.addTask(newTask);
        setJobId('');
        return newTask;
    };

    const simulateProgress = (taskId) => {
        let progress = 0;
        const interval = setInterval(() => {
            progress += Math.floor(Math.random() * 10) + 5;
            if (progress > 90) progress = 90;
            if (props.updateTask) props.updateTask(taskId, { progress });
        }, 500);
        return interval;
    };

    const handleSearchSubmit = async () => {
        if (!searchTerms.trim()) {
            alert('請輸入搜尋詞');
            return;
        }
        const newTask = createTask(`搜尋聚合: ${searchTerms.split('\n')[0]}...`, '搜尋聚合');
        const progressInterval = simulateProgress(newTask.id);

        try {
            const keywordList = searchTerms.split('\n').map(k => k.trim()).filter(k => k);
            const response = await apiFetch('/api/search-aggregate', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({
                    keywords: keywordList,
                    num_results: numResults,
                    time_limit: searchTimeLimit || undefined,
                    site: searchSite || undefined,
                    target_website: targetWebsite,
                    job_id: newTask.id,
                    ignore_links: ignoreLinks,
                }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            setSearchResults(data.data);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: data.data });
            alert('搜尋聚合完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`搜尋失敗: ${err.message}`);
        }
    };

    const handleAgentSubmit = async () => {
        if (!agentPrompt.trim()) { alert('請輸入代理人指令'); return; }
        const newTask = createTask(`Agent: ${agentPrompt.substring(0, 50)}...`, '瀏覽器代理人');
        const progressInterval = setInterval(() => {
            if (props.updateTask) props.updateTask(newTask.id, { progress: Math.min(90, newTask.progress + 2) });
        }, 800);

        try {
            const response = await apiFetch('/api/agent-crawl', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ url: "https://google.com", prompt: agentPrompt, model: "gpt-4o", job_id: newTask.id, ignore_links: ignoreLinks }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            setSearchResults(data.data);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: data.data });
            alert('Agent 任務完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`Agent 任務失敗: ${err.message}`);
        }
    };

    const handleTextCrawlSubmit = async () => {
        const isBatch = crawlMode === 'Batch';
        const inputContent = isBatch ? batchUrls : textCrawlUrl;
        if (!inputContent.trim()) { alert('請輸入 URL'); return; }

        const newTask = createTask(isBatch ? `Batch Crawl (${batchUrls.split('\n').length} URLs)` : `Text Crawl: ${textCrawlUrl}`, '純文字/輕量爬取');
        const progressInterval = simulateProgress(newTask.id);

        try {
            const urlList = isBatch ? batchUrls.split('\n').map(u => u.trim()).filter(u => u) : [textCrawlUrl.trim()];
            const response = await apiFetch('/api/batch-crawl', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ urls: urlList, run_mode: 'lite', sync: true, job_id: newTask.id, ignore_links: ignoreLinks }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            const displayResult = Array.isArray(data.data) ? data.data : [{ url: "Batch Crawl Task", success: true, markdown: `Status: ${data.data.status}`, ...data.data }];
            setSearchResults(displayResult);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: displayResult });
            alert('爬取完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`爬取失敗: ${err.message}`);
        }
    };

    const handlePodcastSubmit = async () => {
        if (!podcastKeywords.trim()) { alert('請輸入 Podcast 關鍵字'); return; }
        const newTask = createTask(`Podcast 下載: ${podcastKeywords}`, '指定下載');
        const progressInterval = simulateProgress(newTask.id);

        try {
            const response = await apiFetch('/api/podcast-search', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ keywords: podcastKeywords, year: podcastYear, limit: podcastLimit, job_id: newTask.id }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            setSearchResults(data.data);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: data.data });
            alert('Podcast 下載完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`Podcast 下載失敗: ${err.message}`);
        }
    };

    const handleArxivSubmit = async () => {
        if (!arxivKeywords.trim()) { alert('請輸入 ArXiv 關鍵字'); return; }
        const newTask = createTask(`ArXiv 下載: ${arxivKeywords}`, '指定下載');
        const progressInterval = simulateProgress(newTask.id);

        try {
            const response = await apiFetch('/api/arxiv-search', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ keywords: arxivKeywords, year: arxivYear, limit: arxivLimit, job_id: newTask.id }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            setSearchResults(data.data);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: data.data });
            alert('ArXiv 下載完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`ArXiv 下載失敗: ${err.message}`);
        }
    };

    const handleAIExplorationSubmit = async () => {
        if (!explorationUrl.trim()) { alert('請輸入起始網址'); return; }
        const newTask = createTask(`AI 探索: ${explorationUrl}`, 'AI 探索');
        const progressInterval = setInterval(() => {
            if (props.updateTask) props.updateTask(newTask.id, { progress: Math.min(90, newTask.progress + 1) });
        }, 1000);

        try {
            const response = await apiFetch('/api/ai-exploration', {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify({ url: explorationUrl, limit: explorationLimit, job_id: newTask.id, ignore_links: ignoreLinks }),
            });
            clearInterval(progressInterval);
            if (!response.ok) throw new Error(`Error ${response.status}`);
            const data = await response.json();
            setSearchResults(data.data);
            if (props.updateTask) props.updateTask(newTask.id, { status: '完成', progress: 100, results: data.data });
            alert('AI 探索完成！');
        } catch (err) {
            clearInterval(progressInterval);
            if (props.updateTask) props.updateTask(newTask.id, { status: '失敗', progress: 100 });
            alert(`AI 探索失敗: ${err.message}`);
        }
    };

    // ==================== MODAL CONTENT RENDERERS ====================
    const renderSearchConfig = () => (
        <div className="space-y-4">
            <div>
                <label htmlFor="modal-search-terms" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">搜尋詞列表</label>
                <textarea
                    id="modal-search-terms"
                    rows="4"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    placeholder={`每行一個關鍵字，例如：\nAI status update 2025\nlatest news on quantum computing`}
                    value={searchTerms}
                    onChange={(e) => setSearchTerms(e.target.value)}
                />
            </div>
            <div>
                <label htmlFor="modal-num-results" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">結果數量</label>
                <input
                    type="number"
                    id="modal-num-results"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    value={numResults}
                    onChange={(e) => setNumResults(parseInt(e.target.value) || 10)}
                    min="1"
                    max="1000"
                />
            </div>
            <div className="grid grid-cols-2 gap-4">
                <div>
                    <label htmlFor="modal-time-limit" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">時間限制</label>
                    <select
                        id="modal-time-limit"
                        className="w-full ios-input ios-focus-ring bg-gray-50"
                        value={searchTimeLimit}
                        onChange={(e) => setSearchTimeLimit(e.target.value)}
                    >
                        <option value="">不限</option>
                        <option value="d1">過去 24 小時</option>
                        <option value="w1">過去一週</option>
                        <option value="m1">過去一個月</option>
                        <option value="y1">過去一年</option>
                    </select>
                </div>
                <div>
                    <label htmlFor="modal-site-filter" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">網站過濾</label>
                    <input
                        type="text"
                        id="modal-site-filter"
                        className="w-full ios-input ios-focus-ring bg-gray-50"
                        placeholder="例如: reddit.com"
                        value={searchSite}
                        onChange={(e) => setSearchSite(e.target.value)}
                    />
                </div>
            </div>
            <div className="flex items-center space-x-2 p-3 bg-gray-50 rounded-xl">
                <input
                    type="checkbox"
                    id="modal-target-website"
                    className="ios-toggle scale-75"
                    checked={targetWebsite}
                    onChange={(e) => setTargetWebsite(e.target.checked)}
                />
                <label htmlFor="modal-target-website" className="text-[15px] text-gray-700">Target Website (特定網站列表)</label>
            </div>
            <div className="flex items-center space-x-2 p-3 bg-gray-50 rounded-xl">
                <input
                    type="checkbox"
                    id="modal-ignore-links"
                    className="ios-toggle scale-75"
                    checked={ignoreLinks}
                    onChange={(e) => setIgnoreLinks(e.target.checked)}
                />
                <label htmlFor="modal-ignore-links" className="text-[15px] text-gray-700">移除 Markdown 連結 ([1])</label>
            </div>
        </div>
    );

    const renderTextCrawlConfig = () => (
        <div className="space-y-4">
            <div>
                <label className="block text-[13px] font-medium text-gray-500 uppercase mb-2">爬取模式</label>
                <div className="bg-gray-100 p-1 rounded-xl flex">
                    <button type="button" className={`flex-1 py-2 text-sm font-medium rounded-lg transition ${crawlMode === 'Prompt' ? 'bg-white shadow-sm text-black' : 'text-gray-500'}`} onClick={() => setCrawlMode('Prompt')}>單筆 URL</button>
                    <button type="button" className={`flex-1 py-2 text-sm font-medium rounded-lg transition ${crawlMode === 'Batch' ? 'bg-white shadow-sm text-black' : 'text-gray-500'}`} onClick={() => setCrawlMode('Batch')}>批次 URL</button>
                </div>
            </div>
            {crawlMode === 'Prompt' ? (
                <div>
                    <label htmlFor="modal-crawl-url" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">目標網址</label>
                    <input
                        type="url"
                        id="modal-crawl-url"
                        className="w-full ios-input ios-focus-ring bg-gray-50"
                        placeholder="https://www.example.com/page"
                        value={textCrawlUrl}
                        onChange={(e) => setTextCrawlUrl(e.target.value)}
                    />
                </div>
            ) : (
                <div>
                    <label htmlFor="modal-batch-urls" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">URL 列表 (每行一個)</label>
                    <textarea
                        id="modal-batch-urls"
                        rows="5"
                        className="w-full ios-input ios-focus-ring bg-gray-50"
                        placeholder={`https://example.com/page1\nhttps://example.com/page2`}
                        value={batchUrls}
                        onChange={(e) => setBatchUrls(e.target.value)}
                    />
                </div>
            )}
            <div className="flex items-center space-x-2 p-3 bg-gray-50 rounded-xl">
                <input
                    type="checkbox"
                    id="modal-text-ignore-links"
                    className="ios-toggle scale-75"
                    checked={ignoreLinks}
                    onChange={(e) => setIgnoreLinks(e.target.checked)}
                />
                <label htmlFor="modal-text-ignore-links" className="text-[15px] text-gray-700">移除 Markdown 連結 ([1])</label>
            </div>
        </div>
    );

    const renderAgentConfig = () => (
        <div className="space-y-4">
            <div>
                <label htmlFor="modal-agent-prompt" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">代理人指令 (Prompt)</label>
                <textarea
                    id="modal-agent-prompt"
                    rows="4"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    placeholder={`描述你希望 AI 執行的瀏覽器操作，例如：\n打開 'https://company.com'，點擊 'Products'，然後擷取所有產品名稱和價格。`}
                    value={agentPrompt}
                    onChange={(e) => setAgentPrompt(e.target.value)}
                />
            </div>
            <div className="flex items-center space-x-2 p-3 bg-gray-50 rounded-xl">
                <input
                    type="checkbox"
                    id="modal-agent-ignore-links"
                    className="ios-toggle scale-75"
                    checked={ignoreLinks}
                    onChange={(e) => setIgnoreLinks(e.target.checked)}
                />
                <label htmlFor="modal-agent-ignore-links" className="text-[15px] text-gray-700">移除 Markdown 連結 ([1])</label>
            </div>
        </div>
    );

    const renderVisualConfig = () => (
        <div className="space-y-4">
            <div>
                <label htmlFor="modal-visual-dir" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">文件輸入資料夾</label>
                <input
                    type="text"
                    id="modal-visual-dir"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    placeholder="/file_store/Q4_Reports/"
                    value={visualInputDir}
                    onChange={(e) => setVisualInputDir(e.target.value)}
                />
            </div>
            <div>
                <label htmlFor="modal-visual-model" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">VLLM 模型</label>
                <select
                    id="modal-visual-model"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    value={visualModel}
                    onChange={(e) => setVisualModel(e.target.value)}
                >
                    <option value="">請選擇模型</option>
                    <option value="Vision-Llama-7B-OCR">Vision-Llama-7B-OCR (高精度)</option>
                    <option value="Flash-ViT-1B-PDF">Flash-ViT-1B-PDF (極速)</option>
                    <option value="Custom-Vision-Model">自定義模型</option>
                </select>
            </div>
        </div>
    );

    const renderAudioConfig = () => (
        <div className="space-y-4">
            <div>
                <label htmlFor="modal-audio-dir" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">音訊輸入資料夾</label>
                <input
                    type="text"
                    id="modal-audio-dir"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    placeholder="/file_store/podcast_clips/raw/"
                    value={audioInputDir}
                    onChange={(e) => setAudioInputDir(e.target.value)}
                />
            </div>
            <div>
                <label htmlFor="modal-audio-model" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">VLLM 模型</label>
                <select
                    id="modal-audio-model"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    value={audioModel}
                    onChange={(e) => setAudioModel(e.target.value)}
                >
                    <option value="">請選擇模型</option>
                    <option value="Whisper-Large-V3">Whisper-Large-V3 (高精度)</option>
                    <option value="Fast-ASR-Base">Fast-ASR-Base (極速)</option>
                    <option value="Bilingual-ASR-Model">雙語模型</option>
                </select>
            </div>
        </div>
    );

    const renderAIExplorationConfig = () => (
        <div className="space-y-4">
            <p className="text-[14px] text-gray-500 leading-relaxed bg-cyan-50 p-3 rounded-xl">
                AI 將自動解析頁面結構，辨識文章連結並自動翻頁爬取。
            </p>
            <div>
                <label htmlFor="modal-exploration-url" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">起始網址</label>
                <input
                    type="text"
                    id="modal-exploration-url"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    placeholder="https://www.ithome.com.tw/"
                    value={explorationUrl}
                    onChange={(e) => setExplorationUrl(e.target.value)}
                />
            </div>
            <div>
                <label htmlFor="modal-exploration-limit" className="block text-[13px] font-medium text-gray-500 uppercase mb-2">翻頁數量</label>
                <input
                    type="number"
                    id="modal-exploration-limit"
                    className="w-full ios-input ios-focus-ring bg-gray-50"
                    value={explorationLimit}
                    onChange={(e) => setExplorationLimit(parseInt(e.target.value) || 1)}
                    min="1"
                    max="10"
                />
            </div>
            <div className="flex items-center space-x-2 p-3 bg-gray-50 rounded-xl">
                <input
                    type="checkbox"
                    id="modal-ai-ignore-links"
                    className="ios-toggle scale-75"
                    checked={ignoreLinks}
                    onChange={(e) => setIgnoreLinks(e.target.checked)}
                />
                <label htmlFor="modal-ai-ignore-links" className="text-[15px] text-gray-700">移除 Markdown 連結 ([1])</label>
            </div>
        </div>
    );

    const renderApiConfig = () => (
        <div className="space-y-4">
            {/* Reddit */}
            <div className="bg-gray-50 rounded-xl p-4">
                <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                        <div className="w-6 h-6 rounded-full bg-orange-500" />
                        <span className="text-[15px] font-medium text-black">Reddit API</span>
                    </div>
                    <input
                        type="checkbox"
                        checked={apiTypes.reddit}
                        onChange={(e) => setApiTypes(prev => ({ ...prev, reddit: e.target.checked }))}
                        className="ios-toggle scale-75"
                    />
                </div>
                {apiTypes.reddit && (
                    <textarea
                        rows="2"
                        className="w-full ios-input ios-focus-ring bg-white text-sm"
                        placeholder="例如: /r/MachineLearning, new threads about GPT-5"
                        value={redditQuery}
                        onChange={(e) => setRedditQuery(e.target.value)}
                    />
                )}
            </div>

            {/* Github */}
            <div className="bg-gray-50 rounded-xl p-4">
                <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                        <Github className="w-6 h-6 text-gray-800" />
                        <span className="text-[15px] font-medium text-black">Github API</span>
                    </div>
                    <input
                        type="checkbox"
                        checked={apiTypes.github}
                        onChange={(e) => setApiTypes(prev => ({ ...prev, github: e.target.checked }))}
                        className="ios-toggle scale-75"
                    />
                </div>
                {apiTypes.github && (
                    <textarea
                        rows="2"
                        className="w-full ios-input ios-focus-ring bg-white text-sm"
                        placeholder="例如: repo:google/flutter, latest issues"
                        value={githubQuery}
                        onChange={(e) => setGithubQuery(e.target.value)}
                    />
                )}
            </div>
        </div>
    );

    const renderDownloadConfig = () => (
        <div className="space-y-4">
            {/* Podcast */}
            <div className="bg-gray-50 rounded-xl p-4">
                <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                        <Mic className="w-5 h-5 text-purple-500" />
                        <span className="text-[15px] font-medium text-black">Podcast (RSS)</span>
                    </div>
                    <input
                        type="checkbox"
                        checked={downloadTypes.podcast}
                        onChange={(e) => setDownloadTypes(prev => ({ ...prev, podcast: e.target.checked }))}
                        className="ios-toggle scale-75"
                    />
                </div>
                {downloadTypes.podcast && (
                    <div className="space-y-3 mt-3">
                        <input type="text" className="w-full ios-input ios-focus-ring bg-white text-sm" placeholder="關鍵字: Lex Fridman, Tech News" value={podcastKeywords} onChange={(e) => setPodcastKeywords(e.target.value)} />
                        <div className="grid grid-cols-2 gap-3">
                            <input type="text" className="ios-input ios-focus-ring bg-white text-sm" placeholder="年份 (選填)" value={podcastYear} onChange={(e) => setPodcastYear(e.target.value)} />
                            <input type="number" className="ios-input ios-focus-ring bg-white text-sm" placeholder="數量" value={podcastLimit} onChange={(e) => setPodcastLimit(parseInt(e.target.value) || 5)} min="1" max="20" />
                        </div>
                    </div>
                )}
            </div>

            {/* ArXiv */}
            <div className="bg-gray-50 rounded-xl p-4">
                <div className="flex items-center justify-between mb-3">
                    <div className="flex items-center space-x-2">
                        <FileText className="w-5 h-5 text-orange-500" />
                        <span className="text-[15px] font-medium text-black">ArXiv (論文)</span>
                    </div>
                    <input
                        type="checkbox"
                        checked={downloadTypes.arxiv}
                        onChange={(e) => setDownloadTypes(prev => ({ ...prev, arxiv: e.target.checked }))}
                        className="ios-toggle scale-75"
                    />
                </div>
                {downloadTypes.arxiv && (
                    <div className="space-y-3 mt-3">
                        <input type="text" className="w-full ios-input ios-focus-ring bg-white text-sm" placeholder="關鍵字: LLM Agents, Quantum" value={arxivKeywords} onChange={(e) => setArxivKeywords(e.target.value)} />
                        <div className="grid grid-cols-2 gap-3">
                            <input type="text" className="ios-input ios-focus-ring bg-white text-sm" placeholder="年份 (選填)" value={arxivYear} onChange={(e) => setArxivYear(e.target.value)} />
                            <input type="number" className="ios-input ios-focus-ring bg-white text-sm" placeholder="數量" value={arxivLimit} onChange={(e) => setArxivLimit(parseInt(e.target.value) || 5)} min="1" max="20" />
                        </div>
                    </div>
                )}
            </div>
        </div>
    );

    const getModalContent = (strategyId) => {
        switch (strategyId) {
            case 'search': return renderSearchConfig();
            case 'textCrawl': return renderTextCrawlConfig();
            case 'agent': return renderAgentConfig();
            case 'visual': return renderVisualConfig();
            case 'audio': return renderAudioConfig();
            case 'aiExploration': return renderAIExplorationConfig();
            case 'api': return renderApiConfig();
            case 'download': return renderDownloadConfig();
            default: return null;
        }
    };

    const activeStrategy = STRATEGIES.find(s => s.id === activeModal);

    return (
        <div className="md:col-span-1 h-full">
            <div className="ios-card p-6 mb-6">
                <div className="flex items-center space-x-3 mb-6">
                    <div className="w-9 h-9 rounded-xl bg-gradient-to-br from-blue-500 to-indigo-600 flex items-center justify-center shadow-sm shadow-blue-500/25">
                        <Plus className="w-5 h-5 text-white" />
                    </div>
                    <h2 className="text-[20px] font-bold text-gray-900">新任務</h2>
                </div>

                <form onSubmit={handleSubmit}>
                    {/* Job ID */}
                    <div className="mb-6">
                        <label htmlFor="job-id-input" className="block text-[13px] font-medium text-gray-500 uppercase mb-2 ml-1">
                            專案 Job ID
                        </label>
                        <input
                            type="text"
                            id="job-id-input"
                            value={jobId}
                            onChange={(e) => {
                                // Sanitize for S3: lowercase, alphanumeric + hyphens only
                                const sanitized = e.target.value
                                    .toLowerCase()
                                    .replace(/[^a-z0-9-]/g, '-')
                                    .replace(/-+/g, '-');
                                setJobId(sanitized);
                            }}
                            className="w-full ios-input ios-focus-ring"
                            placeholder="例如: q4-data-pull-001 (小寫英數字和連字號)"
                        />
                    </div>

                    {/* Import/Export */}
                    <div className="flex space-x-3 mb-8">
                        <button type="button" className="flex-1 flex items-center justify-center px-4 py-2.5 text-[15px] font-medium rounded-xl text-actionblue bg-blue-50 active:bg-blue-100 transition duration-150 cursor-pointer" onClick={() => alert('Export config logic coming soon')}>
                            <Download className="h-4 w-4 mr-2" />
                            匯出配置
                        </button>
                        <button type="button" className="flex-1 flex items-center justify-center px-4 py-2.5 text-[15px] font-medium rounded-xl text-actionblue bg-blue-50 active:bg-blue-100 transition duration-150 cursor-pointer" onClick={() => document.getElementById('file-input-config').click()}>
                            <Upload className="h-4 w-4 mr-2" />
                            匯入配置
                        </button>
                        <input type="file" id="file-input-config" accept=".json" className="hidden" />
                    </div>

                    {/* Strategy Selection */}
                    <div className="mb-6">
                        <label className="block text-[13px] font-medium text-gray-500 uppercase mb-2 ml-1">
                            選擇處理策略
                        </label>
                        <div className="bg-white rounded-xl overflow-hidden border border-gray-200 divide-y divide-gray-100">
                            {STRATEGIES.map((item) => (
                                <div
                                    key={item.id}
                                    className={`strategy-card flex items-center justify-between p-3.5 cursor-pointer ${strategies[item.id] ? 'selected' : ''}`}
                                    onClick={() => openModal(item.id)}
                                >
                                    <div className="flex items-center flex-1 min-w-0">
                                        <div className={`w-9 h-9 rounded-xl ${item.color} flex items-center justify-center text-white mr-3 shadow-sm flex-shrink-0`}>
                                            <item.Icon className="w-5 h-5" />
                                        </div>
                                        <div className="min-w-0">
                                            <span className="text-[16px] font-medium text-black block">{item.label}</span>
                                            <span className="text-[12px] text-gray-500 block truncate">{item.desc}</span>
                                        </div>
                                    </div>
                                    <div className="flex items-center space-x-2 flex-shrink-0 ml-2">
                                        {strategies[item.id] && (
                                            <div
                                                className="w-6 h-6 rounded-full bg-green-500 flex items-center justify-center cursor-pointer hover:bg-red-500 transition-colors group"
                                                onClick={(e) => removeStrategy(item.id, e)}
                                                title="點擊移除"
                                            >
                                                <Check className="w-4 h-4 text-white group-hover:hidden" />
                                                <span className="text-white text-xs font-bold hidden group-hover:block">✕</span>
                                            </div>
                                        )}
                                        <ChevronRight className="h-5 w-5 text-gray-300" />
                                    </div>
                                </div>
                            ))}
                        </div>
                    </div>

                    {/* Selected Strategies Summary */}
                    {isAnyStrategySelected && (
                        <div className="mb-6 p-4 bg-blue-50 rounded-xl border border-blue-100">
                            <p className="text-[13px] font-medium text-blue-600 mb-2">已選擇的策略：</p>
                            <div className="flex flex-wrap gap-2">
                                {STRATEGIES.filter(s => strategies[s.id]).map(s => (
                                    <span key={s.id} className="inline-flex items-center px-3 py-1 rounded-full text-[13px] font-medium bg-white text-gray-700 border border-gray-200">
                                        <s.Icon className="w-3.5 h-3.5 mr-1.5" />
                                        {s.label}
                                    </span>
                                ))}
                            </div>
                        </div>
                    )}

                    {/* Submit */}
                    <button
                        type="submit"
                        disabled={!isAnyStrategySelected || isSubmitting}
                        className={`w-full py-3.5 text-[17px] font-semibold rounded-xl transition-all duration-200 flex items-center justify-center ${isAnyStrategySelected && !isSubmitting
                            ? 'ios-btn-primary shadow-lg shadow-blue-500/30 cursor-pointer'
                            : 'bg-gray-200 text-gray-400 cursor-not-allowed'
                            }`}
                    >
                        {isSubmitting ? (
                            <>
                                <Loader2 className="w-5 h-5 mr-2 animate-spin" />
                                處理中...
                            </>
                        ) : '啟動處理管道'}
                    </button>
                </form>
            </div>

            {/* Strategy Configuration Modal */}
            {activeStrategy && (
                <StrategyModal
                    isOpen={!!activeModal}
                    onClose={closeModal}
                    title={activeStrategy.label}
                    icon={activeStrategy.Icon}
                    iconBg={activeStrategy.color}
                    onConfirm={() => confirmStrategy(activeModal)}
                >
                    {getModalContent(activeModal)}
                </StrategyModal>
            )}
        </div>
    );
};

export default TaskForm;
