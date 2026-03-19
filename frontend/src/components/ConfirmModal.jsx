import React, { useEffect } from 'react';
import { AlertTriangle } from 'lucide-react';

const ConfirmModal = ({ isOpen, onClose, onConfirm, title, message }) => {
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

            {/* Modal */}
            <div className="relative w-full max-w-sm bg-white rounded-2xl ios-modal-sheet shadow-2xl overflow-hidden">
                {/* Content */}
                <div className="p-6 text-center">
                    <div className="w-12 h-12 rounded-full bg-red-100 flex items-center justify-center mx-auto mb-4">
                        <AlertTriangle className="w-6 h-6 text-red-500" />
                    </div>
                    <h3 className="text-[18px] font-bold text-gray-900 mb-2">{title}</h3>
                    <p className="text-[15px] text-gray-500">{message}</p>
                </div>

                {/* Actions */}
                <div className="flex border-t border-gray-100">
                    <button
                        onClick={onClose}
                        className="flex-1 py-3.5 text-[17px] font-medium text-gray-600 hover:bg-gray-50 transition-colors cursor-pointer border-r border-gray-100"
                    >
                        取消
                    </button>
                    <button
                        onClick={() => {
                            onConfirm();
                            onClose();
                        }}
                        className="flex-1 py-3.5 text-[17px] font-medium text-red-500 hover:bg-red-50 transition-colors cursor-pointer"
                    >
                        確定刪除
                    </button>
                </div>
            </div>
        </div>
    );
};

export default ConfirmModal;
