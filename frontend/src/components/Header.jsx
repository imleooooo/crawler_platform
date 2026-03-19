import React from 'react';
import { Bug } from 'lucide-react';

const Header = () => {
    return (
        <header className="sticky top-0 z-50">
            {/* Clean White Background */}
            <div className="absolute inset-0 bg-white/90 backdrop-blur-xl" />

            {/* Subtle Border Bottom */}
            <div className="absolute bottom-0 left-0 right-0 h-[1px] bg-stone-200" />

            {/* Content */}
            <div className="relative max-w-7xl mx-auto px-4 sm:px-6 lg:px-8 h-16 flex items-center">
                {/* Logo & Title */}
                <div className="flex items-center space-x-3">
                    {/* Logo */}
                    <div className="relative">
                        <div className="w-10 h-10 rounded-xl bg-gradient-to-br from-blue-500 via-purple-500 to-pink-500 flex items-center justify-center shadow-lg shadow-purple-500/25">
                            <Bug className="w-5 h-5 text-white" />
                        </div>
                        {/* Glow Effect */}
                        <div className="absolute inset-0 w-10 h-10 rounded-xl bg-gradient-to-br from-blue-500 via-purple-500 to-pink-500 blur-lg opacity-40 -z-10" />
                    </div>

                    {/* Title */}
                    <div>
                        <h1 className="text-[20px] font-bold tracking-tight">
                            <span className="text-gray-900">Crawl</span><span className="text-blue-600">Lab</span>
                        </h1>
                        <div className="flex items-center space-x-1.5">
                            <div className="w-1.5 h-1.5 rounded-full bg-green-500 animate-pulse" />
                            <span className="text-[11px] text-gray-500 font-medium">Online</span>
                        </div>
                    </div>
                </div>
            </div>
        </header>
    );
};

export default Header;
