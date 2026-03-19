import React, { useState } from 'react';
import { Trash2, Inbox } from 'lucide-react';
import ConfirmModal from './ConfirmModal';

const getStatusColor = (status) => {
    switch (status) {
        case '完成': return 'bg-green-100 text-green-600';
        case '進行中': return 'bg-blue-100 text-blue-600';
        case '佇列中': return 'bg-yellow-100 text-yellow-600';
        case '失敗': return 'bg-red-100 text-red-600';
        default: return 'bg-gray-100 text-gray-600';
    }
};

const getProgressBarColor = (status) => {
    switch (status) {
        case '完成': return 'bg-green-500';
        case '進行中': return 'bg-blue-500';
        case '佇列中': return 'bg-yellow-500';
        case '失敗': return 'bg-red-500';
        default: return 'bg-gray-500';
    }
};

const TaskList = ({ tasks, onViewResults, onDeleteTask }) => {
    const [deleteConfirm, setDeleteConfirm] = useState({ isOpen: false, taskId: null });

    const handleDeleteClick = (taskId, e) => {
        e.stopPropagation();
        setDeleteConfirm({ isOpen: true, taskId });
    };

    const handleConfirmDelete = () => {
        if (deleteConfirm.taskId) {
            onDeleteTask(deleteConfirm.taskId);
        }
    };

    if (!tasks || tasks.length === 0) {
        return (
            <div className="ios-card p-8 text-center">
                <div className="flex flex-col items-center space-y-3">
                    <div className="w-12 h-12 rounded-full bg-gray-100 flex items-center justify-center">
                        <Inbox className="w-6 h-6 text-gray-400" />
                    </div>
                    <p className="text-gray-500 text-[15px]">目前沒有任務</p>
                    <p className="text-gray-400 text-[13px]">建立新任務以開始爬取資料</p>
                </div>
            </div>
        );
    }

    return (
        <>
            <div className="custom-scroll max-h-[85vh] overflow-y-auto pr-1 pb-4">
                <div className="space-y-3">
                    {tasks.map((task) => (
                        <div key={task.id} className="ios-card p-4 relative overflow-hidden group active:scale-[0.99] transition-transform duration-150">
                            {/* Progress Bar Background (Subtle) */}
                            <div className="absolute bottom-0 left-0 h-1 w-full bg-gray-100">
                                <div className={`h-full ${getProgressBarColor(task.status)}`} style={{ width: `${task.progress}%` }}></div>
                            </div>

                            <div className="flex justify-between items-start mb-1">
                                <div className="flex items-center">
                                    <span className="text-[13px] font-semibold text-gray-400">{task.id}</span>
                                    <span className={`ml-2 inline-flex items-center px-2.5 py-0.5 rounded-full text-[11px] font-medium ${getStatusColor(task.status)}`}>
                                        {task.status}
                                    </span>
                                </div>
                                <div className="flex items-center space-x-2">
                                    <span className="text-[13px] text-gray-400 font-medium">{task.time}</span>
                                    <button
                                        onClick={(e) => handleDeleteClick(task.id, e)}
                                        className="text-gray-400 hover:text-red-500 transition-colors p-1 cursor-pointer"
                                        aria-label="刪除任務"
                                        title="刪除任務"
                                    >
                                        <Trash2 size={14} />
                                    </button>
                                </div>
                            </div>

                            <p className="text-[15px] font-medium text-black mb-2 line-clamp-2 leading-snug">{task.prompt}</p>

                            <div className="flex justify-between items-center">
                                <div className="text-[12px] text-gray-500 bg-gray-100 px-2 py-1 rounded-md inline-block">
                                    {task.strategy}
                                </div>
                                {task.status === '完成' && (
                                    <button
                                        className="text-actionblue text-[13px] font-medium hover:opacity-70 flex items-center cursor-pointer"
                                        onClick={() => onViewResults(task)}
                                    >
                                        檢視結果 <span className="ml-0.5">&rsaquo;</span>
                                    </button>
                                )}
                            </div>
                        </div>
                    ))}
                </div>
            </div>

            <ConfirmModal
                isOpen={deleteConfirm.isOpen}
                onClose={() => setDeleteConfirm({ isOpen: false, taskId: null })}
                onConfirm={handleConfirmDelete}
                title="刪除任務"
                message="確定要刪除此任務嗎？此操作無法復原。"
            />
        </>
    );
};

export default TaskList;
