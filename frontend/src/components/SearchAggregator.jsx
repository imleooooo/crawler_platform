import React, { useState } from 'react';
import { Search, Loader2, FileJson, FileText, ExternalLink } from 'lucide-react';

const SearchAggregator = () => {
    const [keywords, setKeywords] = useState('');
    const [timeLimit, setTimeLimit] = useState('');
    const [site, setSite] = useState('');
    const [results, setResults] = useState(null);
    const [loading, setLoading] = useState(false);
    const [error, setError] = useState(null);
    const [viewMode, setViewMode] = useState('markdown'); // 'markdown' or 'json'

    const handleSearch = async (e) => {
        e.preventDefault();
        if (!keywords.trim()) return;

        setLoading(true);
        setError(null);
        setResults(null);

        try {
            const keywordList = keywords.split(',').map(k => k.trim()).filter(k => k);

            // Note: In a real app, this URL should be configured via environment variables
            // Assuming Vite proxy is set up or CORS is handled
            const response = await fetch('http://localhost:8000/api/search-aggregate', {
                method: 'POST',
                headers: {
                    'Content-Type': 'application/json',
                },
                body: JSON.stringify({
                    keywords: keywordList,
                    time_limit: timeLimit || null,
                    site: site || null
                }),
            });

            if (!response.ok) {
                throw new Error(`Error: ${response.statusText}`);
            }

            const data = await response.json();
            setResults(data.data);
        } catch (err) {
            setError(err.message);
        } finally {
            setLoading(false);
        }
    };

    return (
        <div className="p-6 max-w-7xl mx-auto space-y-8">
            <div className="text-center space-y-4">
                <h1 className="text-4xl font-bold tracking-tight text-gray-900 dark:text-white">
                    Search Aggregator
                </h1>
                <p className="text-lg text-gray-600 dark:text-gray-400">
                    Enter keywords separated by commas to aggregate search results.
                </p>
            </div>

            <form onSubmit={handleSearch} className="max-w-2xl mx-auto space-y-4">
                <div className="relative">
                    <div className="absolute inset-y-0 left-0 flex items-center pl-3 pointer-events-none">
                        <Search className="w-5 h-5 text-gray-400" />
                    </div>
                    <input
                        type="text"
                        className="block w-full p-4 pl-10 text-sm text-gray-900 border border-gray-300 rounded-lg bg-gray-50 focus:ring-blue-500 focus:border-blue-500 dark:bg-gray-700 dark:border-gray-600 dark:placeholder-gray-400 dark:text-white dark:focus:ring-blue-500 dark:focus:border-blue-500"
                        placeholder="AI Agents, Large Language Models, Web Scraping..."
                        value={keywords}
                        onChange={(e) => setKeywords(e.target.value)}
                        required
                    />
                    <button
                        type="submit"
                        disabled={loading}
                        className="absolute right-2.5 bottom-2.5 bg-blue-700 hover:bg-blue-800 focus:ring-4 focus:outline-none focus:ring-blue-300 font-medium rounded-lg text-sm px-4 py-2 dark:bg-blue-600 dark:hover:bg-blue-700 dark:focus:ring-blue-800 disabled:opacity-50"
                    >
                        {loading ? <Loader2 className="w-5 h-5 animate-spin" /> : 'Search'}
                    </button>
                </div>

                <div className="grid grid-cols-2 gap-4">
                    <div>
                        <label htmlFor="timeLimit" className="block mb-2 text-sm font-medium text-gray-900 dark:text-white">Time Limit</label>
                        <select
                            id="timeLimit"
                            className="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block w-full p-2.5 dark:bg-gray-700 dark:border-gray-600 dark:placeholder-gray-400 dark:text-white dark:focus:ring-blue-500 dark:focus:border-blue-500"
                            value={timeLimit}
                            onChange={(e) => setTimeLimit(e.target.value)}
                        >
                            <option value="">Any time</option>
                            <option value="d1">Past 24 hours</option>
                            <option value="w1">Past week</option>
                            <option value="m1">Past month</option>
                            <option value="y1">Past year</option>
                        </select>
                    </div>
                    <div>
                        <label htmlFor="site" className="block mb-2 text-sm font-medium text-gray-900 dark:text-white">Site Filter (Optional)</label>
                        <input
                            type="text"
                            id="site"
                            className="bg-gray-50 border border-gray-300 text-gray-900 text-sm rounded-lg focus:ring-blue-500 focus:border-blue-500 block w-full p-2.5 dark:bg-gray-700 dark:border-gray-600 dark:placeholder-gray-400 dark:text-white dark:focus:ring-blue-500 dark:focus:border-blue-500"
                            placeholder="e.g. reddit.com"
                            value={site}
                            onChange={(e) => setSite(e.target.value)}
                        />
                    </div>
                </div>
            </form>

            {error && (
                <div className="p-4 mb-4 text-sm text-red-800 rounded-lg bg-red-50 dark:bg-gray-800 dark:text-red-400" role="alert">
                    <span className="font-medium">Error!</span> {error}
                </div>
            )}

            {results && (
                <div className="space-y-6">
                    <div className="flex justify-between items-center">
                        <h2 className="text-2xl font-bold text-gray-900 dark:text-white">Results ({results.length})</h2>
                        <div className="flex space-x-2 bg-gray-100 dark:bg-gray-800 p-1 rounded-lg">
                            <button
                                onClick={() => setViewMode('markdown')}
                                className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${viewMode === 'markdown'
                                    ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-white shadow-sm'
                                    : 'text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200'
                                    }`}
                            >
                                <div className="flex items-center space-x-1">
                                    <FileText className="w-4 h-4" />
                                    <span>Markdown</span>
                                </div>
                            </button>
                            <button
                                onClick={() => setViewMode('json')}
                                className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${viewMode === 'json'
                                    ? 'bg-white dark:bg-gray-600 text-gray-900 dark:text-white shadow-sm'
                                    : 'text-gray-500 hover:text-gray-700 dark:text-gray-400 dark:hover:text-gray-200'
                                    }`}
                            >
                                <div className="flex items-center space-x-1">
                                    <FileJson className="w-4 h-4" />
                                    <span>JSON</span>
                                </div>
                            </button>
                        </div>
                    </div>

                    <div className="grid gap-6">
                        {results.map((item, index) => (
                            <div key={index} className="bg-white dark:bg-gray-800 border border-gray-200 dark:border-gray-700 rounded-lg shadow-sm overflow-hidden">
                                <div className="p-4 border-b border-gray-200 dark:border-gray-700 bg-gray-50 dark:bg-gray-900 flex justify-between items-center">
                                    <a
                                        href={item.url}
                                        target="_blank"
                                        rel="noopener noreferrer"
                                        className="text-blue-600 dark:text-blue-400 hover:underline flex items-center space-x-2 truncate max-w-xl"
                                    >
                                        <span className="truncate">{item.url}</span>
                                        <ExternalLink className="w-4 h-4 flex-shrink-0" />
                                    </a>
                                    <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${item.success
                                        ? 'bg-green-100 text-green-800 dark:bg-green-900 dark:text-green-300'
                                        : 'bg-red-100 text-red-800 dark:bg-red-900 dark:text-red-300'
                                        }`}>
                                        {item.success ? 'Success' : 'Failed'}
                                    </span>
                                </div>

                                <div className="p-4 overflow-auto max-h-96">
                                    {viewMode === 'markdown' ? (
                                        <pre className="whitespace-pre-wrap font-mono text-sm text-gray-700 dark:text-gray-300">
                                            {item.markdown || item.error || 'No content available'}
                                        </pre>
                                    ) : (
                                        <pre className="font-mono text-sm text-gray-700 dark:text-gray-300">
                                            {JSON.stringify(item, null, 2)}
                                        </pre>
                                    )}
                                </div>
                            </div>
                        ))}
                    </div>
                </div>
            )}
        </div>
    );
};

export default SearchAggregator;
