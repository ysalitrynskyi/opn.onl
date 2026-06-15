import { useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { Download } from 'lucide-react';
import { API_ENDPOINTS, authFetch } from '../../config/api';
import type { LinkData } from './types';

export default function QRModal({ link, onClose }: { link: LinkData; onClose: () => void }) {
    const [qrUrl, setQrUrl] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const qrUrlRef = useRef<string | null>(null);

    useEffect(() => {
        const fetchQR = async () => {
            try {
                const res = await authFetch(API_ENDPOINTS.linkQr(link.id));
                if (res.ok) {
                    const blob = await res.blob();
                    const url = URL.createObjectURL(blob);
                    qrUrlRef.current = url;
                    setQrUrl(url);
                } else {
                    setError('Failed to load QR code');
                }
            } catch {
                setError('Failed to load QR code');
            } finally {
                setLoading(false);
            }
        };
        fetchQR();
        return () => {
            if (qrUrlRef.current) {
                URL.revokeObjectURL(qrUrlRef.current);
                qrUrlRef.current = null;
            }
        };
    }, [link.id]);

    const downloadQR = () => {
        if (!qrUrl) return;
        const a = document.createElement('a');
        a.href = qrUrl;
        a.download = `qr-${link.code}.png`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
    };

    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 bg-ink/40 backdrop-blur-sm flex items-center justify-center z-50 p-4"
            onClick={onClose}
        >
            <motion.div
                initial={{ scale: 0.97, opacity: 0, y: 8 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.97, opacity: 0, y: 8 }}
                transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-sm w-full p-6 text-center"
                onClick={e => e.stopPropagation()}
            >
                <h3 className="font-display text-xl font-bold text-ink tracking-tight mb-4">QR code</h3>
                <div className="inline-block rounded-xl border border-line bg-white p-4 mb-4">
                    {loading ? (
                        <div className="w-48 h-48 flex items-center justify-center">
                            <div className="h-8 w-8 rounded-full border-2 border-line2 border-t-primary-600 animate-spin" role="status" aria-label="Loading QR code" />
                        </div>
                    ) : error ? (
                        <div className="w-48 h-48 flex items-center justify-center text-sm text-danger">
                            {error}
                        </div>
                    ) : (
                        <img
                            src={qrUrl || ''}
                            alt="QR Code"
                            className="w-48 h-48"
                        />
                    )}
                </div>
                <p className="font-mono text-sm text-faint mb-5">{link.code}</p>
                <div className="flex gap-3 justify-center">
                    <button
                        onClick={onClose}
                        className="rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                    >
                        Close
                    </button>
                    <button
                        onClick={downloadQR}
                        disabled={!qrUrl}
                        className="inline-flex items-center gap-2 rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                    >
                        <Download className="h-4 w-4" />
                        Download
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
}
