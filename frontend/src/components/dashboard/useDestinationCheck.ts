import { useEffect, useRef, useState } from 'react';
import { API_ENDPOINTS, SessionExpiredError, authFetch, type UrlHealthResponse } from '../../config/api';
import { statusFromHealth, type DestinationStatus } from './linkFields';

export function useDestinationCheck(url: string) {
    const [pending, setPending] = useState(false);
    const [status, setStatus] = useState<DestinationStatus | null>(null);
    const urlRef = useRef(url);
    const pendingRef = useRef(false);
    urlRef.current = url;

    useEffect(() => {
        setStatus(null);
    }, [url]);

    const check = async () => {
        const requested = url.trim();
        // One request per click. A second click before the state re-renders
        // would otherwise fire again and spend the rate limit.
        if (!requested || pendingRef.current) return;
        pendingRef.current = true;
        setPending(true);
        setStatus(null);
        try {
            const res = await authFetch(API_ENDPOINTS.healthCheck, {
                method: 'POST',
                body: JSON.stringify({ url: requested }),
            });
            const data = await res.json().catch(() => null) as UrlHealthResponse | null;
            if (urlRef.current.trim() !== requested) return;
            setStatus(statusFromHealth(res.status, data));
        } catch (error) {
            if (urlRef.current.trim() !== requested) return;
            setStatus({
                ok: false,
                text: error instanceof SessionExpiredError
                    ? error.message
                    : 'Could not check this destination. Try again.',
            });
        } finally {
            pendingRef.current = false;
            setPending(false);
        }
    };

    return { pending, status, check };
}
