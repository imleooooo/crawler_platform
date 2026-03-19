import React, { useState } from 'react';
import TaskList from './TaskList';
import DataStorage from './DataStorage';

import { FileJson, FileText, ExternalLink, Trash2, ChevronLeft, ChevronRight } from 'lucide-react';

const DashboardTabs = ({ searchResults, clearSearchResults, tasks, onViewTaskResults, deleteTask }) => {
    const [activeTab, setActiveTab] = useState('tasks');
    const [viewMode, setViewMode] = useState('markdown'); // 'markdown' or 'json'
    const [currentPage, setCurrentPage] = useState(1);
    const ITEMS_PER_PAGE = 10;

    React.useEffect(() => {
        setCurrentPage(1);
    }, [searchResults]);

    const handleViewResults = (task) => {
        if (onViewTaskResults) {
            onViewTaskResults(task);
        }
        setActiveTab('results');
    };

    return (
        <div className="md:col-span-2 h-full">
            {/* iOS Segmented Control */}
            <div className="bg-[#E5E5EA] p-1 rounded-xl flex mb-6 mx-1">
                <button
                    className={`flex-1 py-1.5 text-[13px] font-semibold rounded-[9px] transition-all duration-200 cursor-pointer ${activeTab === 'tasks'
                        ? 'bg-white text-black shadow-sm'
                        : 'bg-transparent text-gray-500 hover:text-gray-700'
                        }`}
                    onClick={() => setActiveTab('tasks')}
                >
                    任務列表 (Queue)
                </button>
                <button
                    className={`flex-1 py-1.5 text-[13px] font-semibold rounded-[9px] transition-all duration-200 cursor-pointer ${activeTab === 'filestore'
                        ? 'bg-white text-black shadow-sm'
                        : 'bg-transparent text-gray-500 hover:text-gray-700'
                        }`}
                    onClick={() => setActiveTab('filestore')}
                >
                    資料儲存 (FileStore & DB)
                </button>
                {searchResults && (
                    <button
                        className={`flex-1 py-1.5 text-[13px] font-semibold rounded-[9px] transition-all duration-200 cursor-pointer ${activeTab === 'results'
                            ? 'bg-white text-black shadow-sm'
                            : 'bg-transparent text-gray-500 hover:text-gray-700'
                            }`}
                        onClick={() => setActiveTab('results')}
                    >
                        搜尋結果 (Results)
                    </button>
                )}
            </div>

            {activeTab === 'tasks' && (
                <div>
                    <h3 className="text-[22px] font-bold text-black mb-4 px-1">即時任務監控</h3>
                    <TaskList tasks={tasks} onViewResults={handleViewResults} onDeleteTask={deleteTask} />
                </div>
            )}

            {activeTab === 'filestore' && (
                <div>
                    <h3 className="text-[22px] font-bold text-black mb-4 px-1">結構化資料庫與原始檔案</h3>
                    <DataStorage />
                </div>
            )}

            {activeTab === 'results' && searchResults && (
                <div>
                    <div className="flex justify-between items-center mb-4 px-1">
                        <h3 className="text-[22px] font-bold text-black">搜尋聚合結果 ({searchResults.length})</h3>
                        <div className="flex space-x-2">
                            <button
                                onClick={clearSearchResults}
                                className="px-3 py-1.5 rounded-md text-sm font-medium text-red-600 hover:bg-red-50 transition-colors flex items-center space-x-1"
                            >
                                <Trash2 className="w-4 h-4" />
                                <span>Clear</span>
                            </button>
                            <div className="flex space-x-2 bg-gray-100 p-1 rounded-lg">
                                <button
                                    onClick={() => setViewMode('markdown')}
                                    className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${viewMode === 'markdown'
                                        ? 'bg-white text-black shadow-sm'
                                        : 'text-gray-500 hover:text-gray-700'
                                        }`}
                                >
                                    <div className="flex items-center space-x-1">
                                        <FileText className="w-4 h-4" />
                                        <span>MD</span>
                                    </div>
                                </button>
                                <button
                                    onClick={() => setViewMode('json')}
                                    className={`px-3 py-1.5 rounded-md text-sm font-medium transition-colors ${viewMode === 'json'
                                        ? 'bg-white text-black shadow-sm'
                                        : 'text-gray-500 hover:text-gray-700'
                                        }`}
                                >
                                    <div className="flex items-center space-x-1">
                                        <FileJson className="w-4 h-4" />
                                        <span>JSON</span>
                                    </div>
                                </button>
                            </div>
                        </div>
                    </div>

                    <div className="grid gap-6">
                        {(() => {
                            const validResults = searchResults.filter(item => item.success !== false && !item.error);
                            const totalPages = Math.ceil(validResults.length / ITEMS_PER_PAGE);
                            const startIndex = (currentPage - 1) * ITEMS_PER_PAGE;
                            const currentResults = validResults.slice(startIndex, startIndex + ITEMS_PER_PAGE);

                            return (
                                <>
                                    {currentResults.map((item, index) => (
                                        <div key={startIndex + index} className="bg-white border border-gray-200 rounded-lg shadow-sm overflow-hidden">
                                            {item.pdf_url ? (
                                                // ArXiv Result Card
                                                <div className="p-4">
                                                    <div className="flex justify-between items-start mb-2">
                                                        <h4 className="text-lg font-bold text-black">{item.title}</h4>
                                                        <span className="px-2 py-0.5 rounded-full bg-purple-100 text-purple-800 text-xs font-medium whitespace-nowrap">
                                                            ArXiv
                                                        </span>
                                                    </div>
                                                    <p className="text-sm text-gray-600 mb-2">
                                                        <span className="font-semibold">Authors:</span> {item.authors.join(', ')}
                                                    </p>
                                                    <p className="text-sm text-gray-500 mb-3">
                                                        <span className="font-semibold">Published:</span> {new Date(item.published).toLocaleDateString()}
                                                    </p>
                                                    <div className="flex items-center space-x-4">
                                                        <a
                                                            href={item.pdf_url}
                                                            target="_blank"
                                                            rel="noopener noreferrer"
                                                            className="flex items-center text-blue-600 hover:underline text-sm font-medium cursor-pointer"
                                                        >
                                                            <ExternalLink className="w-4 h-4 mr-1" />
                                                            View PDF
                                                        </a>
                                                        {item.s3_path && (
                                                            <span className="text-xs text-gray-500">
                                                                {item.s3_path}
                                                            </span>
                                                        )}
                                                    </div>
                                                </div>
                                            ) : item.audio_url ? (
                                                // Podcast Result Card
                                                <div className="p-4">
                                                    <div className="flex justify-between items-start mb-2">
                                                        <h4 className="text-lg font-bold text-black">{item.title}</h4>
                                                        <span className="px-2 py-0.5 rounded-full bg-pink-100 text-pink-800 text-xs font-medium whitespace-nowrap">
                                                            Podcast
                                                        </span>
                                                    </div>
                                                    <p className="text-sm text-gray-600 mb-2">
                                                        <span className="font-semibold">Podcast:</span> {item.podcast}
                                                    </p>
                                                    <p className="text-sm text-gray-500 mb-3">
                                                        <span className="font-semibold">Published:</span> {item.published}
                                                    </p>
                                                    <div className="mb-3">
                                                        <audio controls className="w-full h-8">
                                                            <source src={item.audio_url} type="audio/mpeg" />
                                                            Your browser does not support the audio element.
                                                        </audio>
                                                    </div>
                                                    <div className="flex items-center space-x-4">
                                                        <span className="text-xs text-gray-400">
                                                            Saved to: {item.local_path}
                                                        </span>
                                                    </div>
                                                </div>
                                            ) : (
                                                // Standard Search Result / AI Exploration Result
                                                <>
                                                    <div className="p-4 border-b border-gray-200 bg-gray-50 flex justify-between items-center">
                                                        <a
                                                            href={item.url}
                                                            target="_blank"
                                                            rel="noopener noreferrer"
                                                            className="text-blue-600 hover:underline flex items-center space-x-2 truncate max-w-xl"
                                                        >
                                                            <span className="truncate">{item.url}</span>
                                                            <ExternalLink className="w-4 h-4 flex-shrink-0" />
                                                        </a>
                                                        <span className={`px-2.5 py-0.5 rounded-full text-xs font-medium ${item.success
                                                            ? 'bg-green-100 text-green-800'
                                                            : 'bg-red-100 text-red-800'
                                                            }`}>
                                                            {item.success ? 'Success' : 'Failed'}
                                                        </span>
                                                    </div>

                                                    <div className="p-4 overflow-auto max-h-96">
                                                        {item.local_path && (
                                                            <p className="text-xs text-gray-400 mb-2">
                                                                Saved to: {item.local_path}
                                                            </p>
                                                        )}
                                                        {viewMode === 'markdown' ? (
                                                            <pre className="whitespace-pre-wrap font-mono text-sm text-gray-700">
                                                                {item.markdown || item.error || 'No content available'}
                                                            </pre>
                                                        ) : (
                                                            <pre className="font-mono text-sm text-gray-700">
                                                                {JSON.stringify(item, null, 2)}
                                                            </pre>
                                                        )}
                                                    </div>
                                                </>
                                            )}
                                        </div>
                                    ))}

                                    {/* Pagination Controls */}
                                    {totalPages > 1 && (
                                        <div className="flex items-center justify-center space-x-4 mt-8 pb-8">
                                            <button
                                                onClick={() => setCurrentPage(prev => Math.max(prev - 1, 1))}
                                                disabled={currentPage === 1}
                                                className={`p-2 rounded-full transition-colors ${
                                                    currentPage === 1
                                                        ? 'text-gray-300 cursor-not-allowed'
                                                        : 'text-gray-600 hover:bg-gray-100 hover:text-black'
                                                }`}
                                            >
                                                <ChevronLeft className="w-6 h-6" />
                                            </button>
                                            
                                            <span className="text-sm font-medium text-gray-600">
                                                Page {currentPage} of {totalPages}
                                            </span>

                                            <button
                                                onClick={() => setCurrentPage(prev => Math.min(prev + 1, totalPages))}
                                                disabled={currentPage === totalPages}
                                                className={`p-2 rounded-full transition-colors ${
                                                    currentPage === totalPages
                                                        ? 'text-gray-300 cursor-not-allowed'
                                                        : 'text-gray-600 hover:bg-gray-100 hover:text-black'
                                                }`}
                                            >
                                                <ChevronRight className="w-6 h-6" />
                                            </button>
                                        </div>
                                    )}
                                </>
                            );
                        })()}
                    </div>
                </div>
            )}
        </div>
    );
};

export default DashboardTabs;
