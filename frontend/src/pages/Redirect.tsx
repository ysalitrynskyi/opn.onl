import { useEffect, useState } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { API_ENDPOINTS } from '../config/api';

/**
 * Redirect component - handles short link redirects by checking if a code exists
 * and either redirecting to the target URL or showing 404
 */
export default function Redirect() {
    const { code } = useParams<{ code: string }>();
    const navigate = useNavigate();
    const [checking, setChecking] = useState(true);
    const [error, setError] = useState<string | null>(null);

    useEffect(() => {
        if (!code) {
            navigate('/404', { replace: true });
            return;
        }

        // Check if this code exists by hitting the preview endpoint
        const checkAndRedirect = async () => {
            try {
                // First, check if link exists via preview
                const previewRes = await fetch(`${API_ENDPOINTS.preview(code)}`);
                
                if (previewRes.ok) {
                    const data = await previewRes.json();
                    
                    // If link is password protected, redirect to password page
                    if (data.has_password) {
                        navigate(`/password/${code}`, { replace: true });
                        return;
                    }
                    
                    // If link is expired, show error
                    if (data.is_expired) {
                        setError('This link has expired');
                        setChecking(false);
                        return;
                    }
                    
                    // Link exists and is not password protected - redirect via API
                    // Use the backend API URL for the actual redirect
                    const apiUrl = import.meta.env.VITE_API_URL || 'http://localhost:3000';
                    window.location.href = `${apiUrl}/${code}`;
                } else if (previewRes.status === 404) {
                    // Link not found - show 404
                    navigate('/404', { replace: true });
                } else {
                    // Some other error
                    setError('Failed to load link');
                    setChecking(false);
                }
            } catch (err) {
                console.error('Redirect error:', err);
                // On network error, still try to redirect via API (might work)
                const apiUrl = import.meta.env.VITE_API_URL || 'http://localhost:3000';
                window.location.href = `${apiUrl}/${code}`;
            }
        };

        checkAndRedirect();
    }, [code, navigate]);

    if (error) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="text-center">
                    <h1 className="text-2xl font-bold text-slate-800 mb-2">Link Unavailable</h1>
                    <p className="text-slate-600">{error}</p>
                </div>
            </div>
        );
    }

    if (checking) {
        return (
            <div className="min-h-[60vh] flex items-center justify-center">
                <div className="flex flex-col items-center gap-4">
                    <div className="w-8 h-8 border-4 border-primary-200 border-t-primary-600 rounded-full animate-spin" />
                    <p className="text-slate-500 text-sm">Redirecting...</p>
                </div>
            </div>
        );
    }

    return null;
}
