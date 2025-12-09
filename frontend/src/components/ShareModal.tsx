import { motion } from 'framer-motion';
import { X, Copy, Mail, Twitter, Facebook, Linkedin, MessageCircle, Check } from 'lucide-react';
import { useState } from 'react';
import { toast } from './Toast';

interface ShareModalProps {
    url: string;
    title?: string;
    onClose: () => void;
}

export default function ShareModal({ url, title = 'Check out this link', onClose }: ShareModalProps) {
    const [copied, setCopied] = useState(false);
    
    const handleCopy = async () => {
        try {
            await navigator.clipboard.writeText(url);
            setCopied(true);
            toast('Link copied!', 'success');
            setTimeout(() => setCopied(false), 2000);
        } catch {
            toast('Failed to copy', 'error');
        }
    };
    
    const shareOptions = [
        {
            name: 'Copy Link',
            icon: copied ? Check : Copy,
            color: 'bg-slate-100 text-slate-700 hover:bg-slate-200',
            onClick: handleCopy,
        },
        {
            name: 'Email',
            icon: Mail,
            color: 'bg-blue-100 text-blue-700 hover:bg-blue-200',
            onClick: () => window.open(`mailto:?subject=${encodeURIComponent(title)}&body=${encodeURIComponent(url)}`),
        },
        {
            name: 'Twitter',
            icon: Twitter,
            color: 'bg-sky-100 text-sky-700 hover:bg-sky-200',
            onClick: () => window.open(`https://twitter.com/intent/tweet?url=${encodeURIComponent(url)}&text=${encodeURIComponent(title)}`, '_blank'),
        },
        {
            name: 'Facebook',
            icon: Facebook,
            color: 'bg-blue-100 text-blue-700 hover:bg-blue-200',
            onClick: () => window.open(`https://www.facebook.com/sharer/sharer.php?u=${encodeURIComponent(url)}`, '_blank'),
        },
        {
            name: 'LinkedIn',
            icon: Linkedin,
            color: 'bg-blue-100 text-blue-700 hover:bg-blue-200',
            onClick: () => window.open(`https://www.linkedin.com/sharing/share-offsite/?url=${encodeURIComponent(url)}`, '_blank'),
        },
        {
            name: 'WhatsApp',
            icon: MessageCircle,
            color: 'bg-emerald-100 text-emerald-700 hover:bg-emerald-200',
            onClick: () => window.open(`https://wa.me/?text=${encodeURIComponent(title + ' ' + url)}`, '_blank'),
        },
    ];
    
    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-black/50 flex items-center justify-center z-50 p-4"
            onClick={onClose}
        >
            <motion.div
                initial={{ scale: 0.95, opacity: 0 }}
                animate={{ scale: 1, opacity: 1 }}
                exit={{ scale: 0.95, opacity: 0 }}
                className="bg-white rounded-2xl shadow-xl max-w-md w-full p-6"
                onClick={e => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-6">
                    <h3 className="text-xl font-bold text-slate-900">Share Link</h3>
                    <button
                        onClick={onClose}
                        className="p-2 text-slate-400 hover:text-slate-600 transition-colors rounded-lg hover:bg-slate-100"
                        aria-label="Close"
                    >
                        <X className="h-5 w-5" />
                    </button>
                </div>
                
                {/* URL Display */}
                <div className="bg-slate-50 rounded-lg p-3 mb-6 flex items-center gap-2">
                    <input
                        type="text"
                        value={url}
                        readOnly
                        className="flex-1 bg-transparent text-sm text-slate-600 outline-none"
                    />
                    <button
                        onClick={handleCopy}
                        className="p-2 text-primary-600 hover:bg-primary-50 rounded-lg transition-colors"
                        aria-label="Copy link"
                    >
                        {copied ? <Check className="h-4 w-4" /> : <Copy className="h-4 w-4" />}
                    </button>
                </div>
                
                {/* Share Options Grid */}
                <div className="grid grid-cols-3 gap-3">
                    {shareOptions.map((option) => (
                        <button
                            key={option.name}
                            onClick={option.onClick}
                            className={`flex flex-col items-center gap-2 p-4 rounded-xl transition-colors ${option.color}`}
                        >
                            <option.icon className="h-5 w-5" />
                            <span className="text-xs font-medium">{option.name}</span>
                        </button>
                    ))}
                </div>
            </motion.div>
        </motion.div>
    );
}


