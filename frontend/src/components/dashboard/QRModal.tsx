import { useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { Download } from 'lucide-react';
import { API_ENDPOINTS, authFetch } from '../../config/api';
import type { LinkData } from './types';

const COLOR_PRESETS = [
    { name: 'Cobalt', value: '#2f37d8' },
    { name: 'Ink', value: '#0f1115' },
    { name: 'Emerald', value: '#059669' },
    { name: 'Rose', value: '#e11d48' },
    { name: 'Amber', value: '#d97706' },
];

export default function QRModal({
    link,
    onClose,
    brandingEnabled = true,
}: {
    link: LinkData;
    onClose: () => void;
    brandingEnabled?: boolean;
}) {
    const [qrUrl, setQrUrl] = useState<string | null>(null);
    const [loading, setLoading] = useState(true);
    const [error, setError] = useState('');
    const [color, setColor] = useState('#2f37d8');
    const [useLogo, setUseLogo] = useState(false);
    const [format, setFormat] = useState<'png' | 'svg'>('png');
    const qrUrlRef = useRef<string | null>(null);

    // Re-fetch whenever a branding option changes. Debounced so dragging the
    // color picker doesn't hammer the backend.
    useEffect(() => {
        let cancelled = false;
        setLoading(true);
        setError('');
        const timer = setTimeout(async () => {
            try {
                const endpoint = brandingEnabled
                    ? API_ENDPOINTS.linkQr(link.id, { color, logo: useLogo, format })
                    : API_ENDPOINTS.linkQr(link.id);
                const res = await authFetch(endpoint);
                if (cancelled) return;
                if (res.ok) {
                    const blob = await res.blob();
                    if (cancelled) return;
                    const url = URL.createObjectURL(blob);
                    if (qrUrlRef.current) URL.revokeObjectURL(qrUrlRef.current);
                    qrUrlRef.current = url;
                    setQrUrl(url);
                } else {
                    setError('Failed to load QR code');
                }
            } catch {
                if (!cancelled) setError('Failed to load QR code');
            } finally {
                if (!cancelled) setLoading(false);
            }
        }, 250);
        return () => {
            cancelled = true;
            clearTimeout(timer);
        };
    }, [link.id, color, useLogo, format, brandingEnabled]);

    // Revoke the last object URL on unmount.
    useEffect(
        () => () => {
            if (qrUrlRef.current) {
                URL.revokeObjectURL(qrUrlRef.current);
                qrUrlRef.current = null;
            }
        },
        []
    );

    const downloadQR = () => {
        if (!qrUrl) return;
        const ext = brandingEnabled ? format : 'png';
        const a = document.createElement('a');
        a.href = qrUrl;
        a.download = `qr-${link.code}.${ext}`;
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
                className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-md w-full p-6 text-center"
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
                        <img src={qrUrl || ''} alt="QR Code" className="w-48 h-48" />
                    )}
                </div>
                <p className="font-mono text-sm text-faint mb-5">{link.code}</p>

                {brandingEnabled && (
                    <div className="text-left space-y-4 mb-6">
                        {/* Color */}
                        <div>
                            <span className="block text-xs font-semibold uppercase tracking-wide text-faint mb-2">Color</span>
                            <div className="flex items-center gap-2">
                                {COLOR_PRESETS.map(p => (
                                    <button
                                        key={p.value}
                                        type="button"
                                        onClick={() => setColor(p.value)}
                                        aria-label={p.name}
                                        aria-pressed={color.toLowerCase() === p.value.toLowerCase()}
                                        className={`h-7 w-7 rounded-full border transition-transform hover:scale-110 ${
                                            color.toLowerCase() === p.value.toLowerCase()
                                                ? 'border-ink ring-2 ring-ink/20 ring-offset-1 ring-offset-surface'
                                                : 'border-line2'
                                        }`}
                                        style={{ backgroundColor: p.value }}
                                    />
                                ))}
                                <label className="relative h-7 w-7 rounded-full border border-line2 overflow-hidden cursor-pointer" title="Custom color">
                                    <input
                                        type="color"
                                        value={color}
                                        onChange={e => setColor(e.target.value)}
                                        className="absolute inset-0 h-[200%] w-[200%] -translate-x-1/4 -translate-y-1/4 cursor-pointer"
                                        aria-label="Custom color"
                                    />
                                </label>
                            </div>
                        </div>

                        {/* Logo */}
                        <button
                            type="button"
                            onClick={() => setUseLogo(v => !v)}
                            aria-pressed={useLogo}
                            className="flex w-full items-center justify-between rounded-lg border border-line2 px-3 py-2 text-sm transition-colors hover:border-ink/30"
                        >
                            <span className="font-medium text-muted">Logo in center</span>
                            <span
                                className={`relative h-5 w-9 rounded-full transition-colors ${useLogo ? 'bg-primary-600' : 'bg-line2'}`}
                            >
                                <span
                                    className={`absolute top-0.5 h-4 w-4 rounded-full bg-white transition-transform ${useLogo ? 'translate-x-4' : 'translate-x-0.5'}`}
                                />
                            </span>
                        </button>

                        {/* Format */}
                        <div>
                            <span className="block text-xs font-semibold uppercase tracking-wide text-faint mb-2">Format</span>
                            <div className="grid grid-cols-2 gap-2">
                                {(['png', 'svg'] as const).map(f => (
                                    <button
                                        key={f}
                                        type="button"
                                        onClick={() => setFormat(f)}
                                        aria-pressed={format === f}
                                        className={`rounded-lg border px-3 py-1.5 text-sm font-medium uppercase transition-colors ${
                                            format === f
                                                ? 'border-primary-600 bg-primary-50 text-primary-700'
                                                : 'border-line2 text-muted hover:border-ink/30'
                                        }`}
                                    >
                                        {f}
                                    </button>
                                ))}
                            </div>
                        </div>
                    </div>
                )}

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
