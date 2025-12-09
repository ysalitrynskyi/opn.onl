import { useState, useEffect } from 'react';
import { ExternalLink, Globe, Image as ImageIcon } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';

interface LinkPreviewData {
    url: string;
    title: string | null;
    description: string | null;
    image: string | null;
    site_name: string | null;
    favicon: string | null;
}

interface LinkPreviewCardProps {
    url: string;
    className?: string;
    compact?: boolean;
}

export const LinkPreviewCard = ({ url, className = '', compact = false }: LinkPreviewCardProps) => {
    const [preview, setPreview] = useState<LinkPreviewData | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState(false);
    const [imageError, setImageError] = useState(false);

    useEffect(() => {
        const fetchPreview = async () => {
            try {
                setLoading(true);
                setError(false);
                const response = await fetch(API_ENDPOINTS.previewMetadata, {
                    method: 'POST',
                    headers: { 'Content-Type': 'application/json' },
                    body: JSON.stringify({ url }),
                });

                if (response.ok) {
                    const data = await response.json();
                    setPreview(data);
                } else {
                    setError(true);
                }
            } catch {
                setError(true);
            } finally {
                setLoading(false);
            }
        };

        fetchPreview();
    }, [url]);

    const getDomain = (url: string) => {
        try {
            return new URL(url).hostname;
        } catch {
            return url;
        }
    };

    if (loading) {
        return (
            <div className={`animate-pulse bg-slate-100 rounded-lg ${compact ? 'h-16' : 'h-32'} ${className}`}>
                <div className="flex items-center justify-center h-full text-slate-400 text-sm">
                    Loading preview...
                </div>
            </div>
        );
    }

    if (error || !preview) {
        return (
            <div className={`bg-slate-50 border border-slate-200 rounded-lg p-3 ${className}`}>
                <div className="flex items-center gap-2 text-slate-500">
                    <Globe className="h-4 w-4" />
                    <span className="text-sm truncate">{getDomain(url)}</span>
                </div>
            </div>
        );
    }

    if (compact) {
        return (
            <div className={`bg-white border border-slate-200 rounded-lg p-2 hover:border-slate-300 transition-colors ${className}`}>
                <div className="flex items-center gap-2">
                    {preview.favicon && !imageError ? (
                        <img
                            src={preview.favicon}
                            alt=""
                            className="w-4 h-4 rounded"
                            onError={() => setImageError(true)}
                        />
                    ) : (
                        <Globe className="w-4 h-4 text-slate-400" />
                    )}
                    <div className="flex-1 min-w-0">
                        <p className="text-sm font-medium text-slate-700 truncate">
                            {preview.title || getDomain(url)}
                        </p>
                        {preview.description && (
                            <p className="text-xs text-slate-500 truncate">
                                {preview.description}
                            </p>
                        )}
                    </div>
                </div>
            </div>
        );
    }

    return (
        <div className={`bg-white border border-slate-200 rounded-lg overflow-hidden hover:shadow-md transition-shadow ${className}`}>
            {/* Image */}
            {preview.image && !imageError ? (
                <div className="relative h-32 bg-slate-100">
                    <img
                        src={preview.image}
                        alt={preview.title || ''}
                        className="w-full h-full object-cover"
                        onError={() => setImageError(true)}
                    />
                </div>
            ) : (
                <div className="h-20 bg-gradient-to-br from-slate-100 to-slate-200 flex items-center justify-center">
                    <ImageIcon className="h-8 w-8 text-slate-300" />
                </div>
            )}

            {/* Content */}
            <div className="p-3">
                {/* Site info */}
                <div className="flex items-center gap-2 mb-2">
                    {preview.favicon && !imageError ? (
                        <img
                            src={preview.favicon}
                            alt=""
                            className="w-4 h-4 rounded"
                            onError={() => {}}
                        />
                    ) : (
                        <Globe className="w-4 h-4 text-slate-400" />
                    )}
                    <span className="text-xs text-slate-500 truncate">
                        {preview.site_name || getDomain(url)}
                    </span>
                </div>

                {/* Title */}
                <h3 className="text-sm font-semibold text-slate-800 line-clamp-2 mb-1">
                    {preview.title || getDomain(url)}
                </h3>

                {/* Description */}
                {preview.description && (
                    <p className="text-xs text-slate-500 line-clamp-2">
                        {preview.description}
                    </p>
                )}

                {/* Link */}
                <a
                    href={url}
                    target="_blank"
                    rel="noopener noreferrer"
                    className="inline-flex items-center gap-1 mt-2 text-xs text-primary-600 hover:text-primary-700"
                >
                    <ExternalLink className="h-3 w-3" />
                    Open link
                </a>
            </div>
        </div>
    );
};

export default LinkPreviewCard;

