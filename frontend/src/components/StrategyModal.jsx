import React, { useEffect } from 'react';
import { X } from 'lucide-react';

const StrategyModal = ({ isOpen, onClose, title, icon: Icon, iconBg, children, onConfirm }) => {
    // Prevent body scroll when modal is open
    useEffect(() => {
        if (isOpen) {
            document.body.style.overflow = 'hidden';
        } else {
            document.body.style.overflow = 'unset';
        }
        return () => {
            document.body.style.overflow = 'unset';
        };
    }, [isOpen]);

    // Handle escape key
    useEffect(() => {
        const handleEscape = (e) => {
            if (e.key === 'Escape') onClose();
        };
        if (isOpen) {
            window.addEventListener('keydown', handleEscape);
        }
        return () => window.removeEventListener('keydown', handleEscape);
    }, [isOpen, onClose]);

    if (!isOpen) return null;

    return (
        <div className="fixed inset-0 z-50 flex items-center justify-center p-4">
            {/* Backdrop */}
            <div
                className="absolute inset-0 bg-black/40 backdrop-blur-sm ios-modal-backdrop"
                onClick={onClose}
            />

            {/* Modal Sheet */}
            <div className="relative w-full max-w-lg bg-white rounded-2xl ios-modal-sheet max-h-[85vh] flex flex-col shadow-2xl">
                {/* Header */}
                <div className="flex items-center justify-between px-6 py-4 border-b border-gray-100">
                    <div className="flex items-center space-x-3">
                        {Icon && (
                            <div className={`w-10 h-10 rounded-xl ${iconBg} flex items-center justify-center shadow-sm`}>
                                <Icon className="w-5 h-5 text-white" />
                            </div>
                        )}
                        <h2 className="text-[20px] font-bold text-black">{title}</h2>
                    </div>
                    <button
                        onClick={onClose}
                        className="w-8 h-8 rounded-full bg-gray-100 flex items-center justify-center text-gray-500 hover:bg-gray-200 transition-colors cursor-pointer"
                        aria-label="關閉"
                    >
                        <X className="w-5 h-5" />
                    </button>
                </div>

                {/* Content */}
                <div className="flex-1 overflow-y-auto px-6 py-4 custom-scroll">
                    {children}
                </div>

                {/* Footer Actions */}
                <div className="px-6 py-4 border-t border-gray-100 bg-gray-50/80 flex space-x-3">
                    <button
                        onClick={onClose}
                        className="flex-1 py-3 text-[17px] font-medium text-gray-600 bg-gray-200 rounded-xl hover:bg-gray-300 transition-colors cursor-pointer"
                    >
                        取消
                    </button>
                    <button
                        onClick={onConfirm}
                        className="flex-1 py-3 text-[17px] font-medium text-white bg-actionblue rounded-xl hover:bg-actionhover transition-colors shadow-lg shadow-blue-500/30 cursor-pointer"
                    >
                        確認配置
                    </button>
                </div>
            </div>
        </div>
    );
};

export default StrategyModal;
