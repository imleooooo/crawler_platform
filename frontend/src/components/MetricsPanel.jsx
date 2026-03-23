import { apiFetch } from '../api';
import React from 'react';
import { Inbox, Cog, Clock, Activity, Zap } from 'lucide-react';

const MetricsPanel = () => {
    const [metrics, setMetrics] = React.useState({
        queue_size: 0,
        active_workers: 0,
        avg_latency: 0
    });

    React.useEffect(() => {
        const fetchMetrics = async () => {
            try {
                const response = await apiFetch('/api/metrics');
                if (response.ok) {
                    const data = await response.json();
                    setMetrics(data);
                }
            } catch (error) {
                console.error('Error fetching metrics:', error);
            }
        };

        fetchMetrics();
        const interval = setInterval(fetchMetrics, 3000);
        return () => clearInterval(interval);
    }, []);

    return (
        <div className="py-4">
            {/* Bento Container */}
            <div className="bg-white rounded-2xl shadow-sm border border-gray-100 overflow-hidden">
                {/* Header Bar */}
                <div className="px-5 py-3 border-b border-gray-100 flex items-center justify-between">
                    <div className="flex items-center space-x-2">
                        <Activity className="w-4 h-4 text-blue-500" />
                        <span className="text-[13px] font-semibold text-gray-600">系統監控</span>
                    </div>
                    <div className="flex items-center space-x-1.5">
                        <Zap className="w-3 h-3 text-green-500" />
                        <span className="text-[11px] text-gray-500">即時更新</span>
                    </div>
                </div>

                {/* Bento Grid */}
                <div className="grid grid-cols-3 divide-x divide-gray-100">
                    {/* Metric 1 - Queue Size */}
                    <div className="p-5 group hover:bg-blue-50/50 transition-colors">
                        <div className="flex items-center space-x-2 mb-3">
                            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-blue-500 to-blue-600 flex items-center justify-center shadow-sm shadow-blue-500/25">
                                <Inbox className="w-4 h-4 text-white" />
                            </div>
                            <span className="text-[12px] font-medium text-gray-500 uppercase tracking-wider">Queue</span>
                        </div>
                        <div className="flex items-baseline space-x-1">
                            <span className="text-3xl font-bold text-gray-900">{metrics.queue_size}</span>
                            <span className="text-[12px] text-gray-500">tasks</span>
                        </div>
                    </div>

                    {/* Metric 2 - Active Workers */}
                    <div className="p-5 group hover:bg-green-50/50 transition-colors">
                        <div className="flex items-center space-x-2 mb-3">
                            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-green-500 to-emerald-600 flex items-center justify-center shadow-sm shadow-green-500/25">
                                <Cog className="w-4 h-4 text-white" />
                            </div>
                            <span className="text-[12px] font-medium text-gray-500 uppercase tracking-wider">Workers</span>
                        </div>
                        <div className="flex items-baseline space-x-1">
                            <span className="text-3xl font-bold text-gray-900">{metrics.active_workers}</span>
                            <span className="text-[12px] text-gray-500">active</span>
                        </div>
                    </div>

                    {/* Metric 3 - Latency */}
                    <div className="p-5 group hover:bg-orange-50/50 transition-colors">
                        <div className="flex items-center space-x-2 mb-3">
                            <div className="w-8 h-8 rounded-lg bg-gradient-to-br from-orange-500 to-amber-600 flex items-center justify-center shadow-sm shadow-orange-500/25">
                                <Clock className="w-4 h-4 text-white" />
                            </div>
                            <span className="text-[12px] font-medium text-gray-500 uppercase tracking-wider">Latency</span>
                        </div>
                        <div className="flex items-baseline space-x-1">
                            <span className="text-3xl font-bold text-gray-900">{metrics.avg_latency}</span>
                            <span className="text-[12px] text-gray-500">sec</span>
                        </div>
                    </div>
                </div>
            </div>
        </div>
    );
};

export default MetricsPanel;
