import { useEffect, useState } from 'react';
import { useParams, Link as RouterLink } from 'react-router-dom';
import { motion } from 'framer-motion';
import { ExternalLink, Lock, Clock, MousePointer, Globe, AlertTriangle, ArrowRight } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';
import SEO from '../components/SEO';

interface LinkPreview {
    code: string;
    short_url: string;
    original_url: string;
    domain: string;
    has_password: boolean;
    is_expired: boolean;
    created_at: string;
    click_count: number;
}

export default function Preview() {
    const { code } = useParams<{ code: string }>();
    const [preview, setPreview] = useState<LinkPreview | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');

    useEffect(() => {
        const fetchPreview = async () => {
            try {
                const cleanCode = code?.replace(/\+$/, '');
                const res = await fetch(`${API_ENDPOINTS.base}/${cleanCode}/preview`);
                if (res.ok) {
                    const data = await res.json();
                    setPreview(data);
                } else {
                    setError('Link not found');
                }
            } catch {
                setError('Failed to load preview');
            } finally {
                setLoading(false);
            }
        };

        if (code) {
            fetchPreview();
        }
    }, [code]);

    if (loading) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="h-8 w-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
            </div>
        );
    }

    if (error || !preview) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center px-4">
                <SEO 
                    title="Link Not Found"
                    description="The requested link preview could not be found."
                    noIndex={true}
                />
                <div className="text-center">
                    <AlertTriangle className="h-16 w-16 text-amber-500 mx-auto mb-4" />
                    <h1 className="text-2xl font-bold text-slate-900 mb-2">Link Not Found</h1>
                    <p className="text-slate-600 mb-6">This link doesn't exist or has been deleted.</p>
                    <RouterLink 
                        to="/"
                        className="inline-flex items-center gap-2 bg-primary-600 text-white px-6 py-3 rounded-lg font-medium hover:bg-primary-700"
                    >
                        Go Home
                    </RouterLink>
                </div>
            </div>
        );
    }

    const formattedDate = new Date(preview.created_at).toLocaleDateString('en-US', {
        year: 'numeric',
        month: 'long',
        day: 'numeric',
    });

    return (
        <div className="min-h-[60vh] flex items-center justify-center px-4 py-12">
            <SEO 
                title={`Preview: ${preview.code}`}
                description={`Link preview for ${preview.short_url} → ${preview.domain}`}
                noIndex={true}
            />
            
            <motion.div
                initial={{ opacity: 0, y: 20 }}
                animate={{ opacity: 1, y: 0 }}
                className="max-w-lg w-full"
            >
                <div className="bg-white rounded-2xl shadow-xl border border-slate-200 overflow-hidden">
                    {/* Header */}
                    <div className="bg-gradient-to-r from-primary-600 to-primary-700 px-6 py-5">
                        <h1 className="text-xl font-bold text-white flex items-center gap-2">
                            <Globe className="h-5 w-5" />
                            Link Preview
                        </h1>
                        <p className="text-primary-100 text-sm mt-1">
                            opn.onl/{preview.code}
                        </p>
                    </div>

                    {/* Content */}
                    <div className="p-6 space-y-5">
                        {/* Destination URL */}
                        <div>
                            <label className="block text-xs font-semibold text-slate-500 uppercase tracking-wider mb-2">
                                Destination
                            </label>
                            <div className="flex items-center gap-3 p-4 bg-slate-50 rounded-xl">
                                <div className="h-10 w-10 bg-white rounded-lg border border-slate-200 flex items-center justify-center flex-shrink-0">
                                    <img 
                                        src={`https://www.google.com/s2/favicons?domain=${preview.domain}&sz=32`} 
                                        alt=""
                                        className="h-5 w-5"
                                        onError={(e) => { (e.target as HTMLImageElement).src = '/favicon.svg'; }}
                                    />
                                </div>
                                <div className="min-w-0 flex-1">
                                    <p className="font-medium text-slate-900 truncate">{preview.domain}</p>
                                    <p className="text-sm text-slate-500 truncate">{preview.original_url}</p>
                                </div>
                                <a 
                                    href={preview.original_url}
                                    target="_blank"
                                    rel="noreferrer"
                                    className="text-slate-400 hover:text-slate-600"
                                >
                                    <ExternalLink className="h-4 w-4" />
                                </a>
                            </div>
                        </div>

                        {/* Stats */}
                        <div className="grid grid-cols-2 gap-4">
                            <div className="p-4 bg-slate-50 rounded-xl">
                                <div className="flex items-center gap-2 text-slate-500 text-sm mb-1">
                                    <MousePointer className="h-4 w-4" />
                                    Clicks
                                </div>
                                <p className="text-2xl font-bold text-slate-900">{preview.click_count.toLocaleString()}</p>
                            </div>
                            <div className="p-4 bg-slate-50 rounded-xl">
                                <div className="flex items-center gap-2 text-slate-500 text-sm mb-1">
                                    <Clock className="h-4 w-4" />
                                    Created
                                </div>
                                <p className="text-sm font-medium text-slate-900">{formattedDate}</p>
                            </div>
                        </div>

                        {/* Warnings */}
                        {(preview.has_password || preview.is_expired) && (
                            <div className="space-y-2">
                                {preview.has_password && (
                                    <div className="flex items-center gap-2 p-3 bg-amber-50 border border-amber-200 rounded-lg text-amber-800 text-sm">
                                        <Lock className="h-4 w-4 flex-shrink-0" />
                                        This link is password protected
                                    </div>
                                )}
                                {preview.is_expired && (
                                    <div className="flex items-center gap-2 p-3 bg-red-50 border border-red-200 rounded-lg text-red-800 text-sm">
                                        <Clock className="h-4 w-4 flex-shrink-0" />
                                        This link has expired
                                    </div>
                                )}
                            </div>
                        )}

                        {/* Actions */}
                        <div className="pt-2">
                            {!preview.is_expired ? (
                                <a
                                    href={`/${preview.code}`}
                                    className="w-full flex items-center justify-center gap-2 bg-primary-600 text-white px-6 py-3 rounded-xl font-medium hover:bg-primary-700 transition-colors"
                                >
                                    Visit Link
                                    <ArrowRight className="h-4 w-4" />
                                </a>
                            ) : (
                                <button
                                    disabled
                                    className="w-full flex items-center justify-center gap-2 bg-slate-200 text-slate-500 px-6 py-3 rounded-xl font-medium cursor-not-allowed"
                                >
                                    Link Expired
                                </button>
                            )}
                        </div>
                    </div>

                    {/* Footer */}
                    <div className="px-6 py-4 bg-slate-50 border-t border-slate-200">
                        <p className="text-xs text-slate-500 text-center">
                            Add <code className="bg-white px-1.5 py-0.5 rounded border border-slate-200">+</code> to any opn.onl link to see this preview
                        </p>
                    </div>
                </div>

                {/* Create your own CTA */}
                <motion.div
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    transition={{ delay: 0.3 }}
                    className="mt-6 text-center"
                >
                    <p className="text-slate-600 text-sm mb-3">Want to create your own short links?</p>
                    <RouterLink 
                        to="/register"
                        className="text-primary-600 hover:text-primary-700 font-medium text-sm"
                    >
                        Get started for free →
                    </RouterLink>
                </motion.div>
            </motion.div>
        </div>
    );
}




