import React, { useEffect, useState } from 'react';
import { FileText, ChevronRight, Loader2, Download, ExternalLink } from 'lucide-react';

const DataStorage = () => {
    const [stats, setStats] = useState({
        total_files: 0,
        total_size_display: '0 B',
        recent_files: []
    });
    const [loading, setLoading] = useState(true);

    useEffect(() => {
        const fetchStats = async () => {
            try {
                const response = await fetch('http://localhost:8000/api/storage-stats');
                if (response.ok) {
                    const data = await response.json();
                    setStats(data);
                }
            } catch (error) {
                console.error("Failed to fetch storage stats:", error);
            } finally {
                setLoading(false);
            }
        };

        fetchStats();
        // Poll every 10 seconds to keep fresh
        const interval = setInterval(fetchStats, 10000);
        return () => clearInterval(interval);
    }, []);

    const getTypeColor = (type) => {
        switch (type) {
            case 'PDF': return 'bg-orange-500';
            case 'MP3': return 'bg-pink-500';
            case 'JSON': return 'bg-green-500';
            case 'MD': return 'bg-blue-500';
            default: return 'bg-gray-500';
        }
    };

    return (
        <div className="space-y-6">
            {/* Storage Cards */}
            <div className="grid grid-cols-2 gap-4">

                <a
                    href="http://localhost:9001"
                    target="_blank"
                    rel="noopener noreferrer"
                    className="ios-card p-4 flex flex-col items-center text-center active:bg-gray-50 transition hover:bg-gray-50 cursor-pointer block"
                >
                    <div className="w-12 h-12 rounded-full bg-orange-100 flex items-center justify-center mb-3">
                        <FileText className="text-orange-500 h-6 w-6" />
                    </div>
                    <div className="flex items-center justify-center space-x-1">
                        <p className="text-[15px] font-semibold text-black">FileStore (RustFS)</p>
                        <ExternalLink className="w-3 h-3 text-gray-400" />
                    </div>
                    {loading ? (
                        <div className="h-4 w-16 bg-gray-200 animate-pulse rounded mt-1 mx-auto"></div>
                    ) : (
                        <p className="text-[12px] text-gray-500 mt-1">{stats.total_files} Files ({stats.total_size_display})</p>
                    )}
                </a>
            </div>

            {/* Inset Grouped List for Recent Items */}
            <div>
                <h4 className="text-[13px] font-medium text-gray-500 uppercase mb-2 ml-4">最新入庫項目 (RustFS Recent)</h4>
                <div className="bg-white rounded-xl overflow-hidden border border-gray-200 divide-y divide-gray-100 mx-1">
                    {loading ? (
                        <div className="p-8 flex justify-center text-gray-400">
                            <Loader2 className="animate-spin w-6 h-6" />
                        </div>
                    ) : (stats.recent_files || []).length === 0 ? (
                        <div className="p-8 text-center text-gray-400 text-sm">
                            No files in RustFS yet.
                        </div>
                    ) : (
                        (stats.recent_files || []).map((file, idx) => (
                            <div key={idx} className="flex items-center justify-between p-3.5 active:bg-gray-50 transition cursor-pointer group">
                                <div className="flex items-center overflow-hidden">
                                    <div className={`w-8 h-8 rounded-lg ${getTypeColor(file.type)} flex items-center justify-center text-white mr-3 flex-shrink-0`}>
                                        <span className="text-[10px] font-bold">{file.type}</span>
                                    </div>
                                    <div className="min-w-0">
                                        <p className="text-[15px] font-medium text-black truncate pr-4">{file.name}</p>
                                        <p className="text-[12px] text-gray-400">
                                            {new Date(file.last_modified).toLocaleString()} • {Math.round(file.size / 1024)} KB
                                        </p>
                                    </div>
                                </div>
                                <div className="flex items-center space-x-3">
                                    <a
                                        href={file.url}
                                        download
                                        onClick={(e) => e.stopPropagation()}
                                        className="p-1.5 rounded-full bg-gray-100 text-gray-600 hover:bg-green-100 hover:text-green-600 transition opacity-0 group-hover:opacity-100 cursor-pointer"
                                        aria-label="下載檔案"
                                        title="下載檔案"
                                    >
                                        <Download className="w-4 h-4" />
                                    </a>
                                    <ChevronRight className="h-4 w-4 text-gray-300" />
                                </div>
                            </div>
                        ))
                    )}
                </div>
            </div>
        </div>
    );
};

export default DataStorage;
