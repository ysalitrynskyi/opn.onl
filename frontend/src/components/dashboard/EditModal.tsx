import { useEffect, useState } from 'react';
import { motion } from 'framer-motion';
import { X, Flame, ShieldCheck, Route, ChevronDown } from 'lucide-react';
import { API_ENDPOINTS, authFetch } from '../../config/api';
import type { LinkData, LinkUpdatePayload, RoutingRule } from './types';
import RoutingRulesEditor from './RoutingRulesEditor';

interface EditModalProps {
    link: LinkData;
    onClose: () => void;
    onSave: (id: number, data: LinkUpdatePayload) => Promise<void>;
    burnEnabled?: boolean;
    interstitialEnabled?: boolean;
    routingEnabled?: boolean;
}

export default function EditModal({ link, onClose, onSave, burnEnabled = false, interstitialEnabled = false, routingEnabled = false }: EditModalProps) {
    const [url, setUrl] = useState(link.original_url);
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState(link.expires_at?.split('T')[0] || '');
    const [removePassword, setRemovePassword] = useState(false);
    const [removeExpiration, setRemoveExpiration] = useState(false);
    const [burnAfterReading, setBurnAfterReading] = useState(link.burn_after_reading ?? false);
    const [safeLinkInterstitial, setSafeLinkInterstitial] = useState(link.safe_link_interstitial ?? false);
    const [routingRules, setRoutingRules] = useState<RoutingRule[]>([]);
    const [showRouting, setShowRouting] = useState(false);
    const [saving, setSaving] = useState(false);

    useEffect(() => {
        if (!routingEnabled) return;
        let cancelled = false;
        (async () => {
            try {
                const res = await authFetch(API_ENDPOINTS.linkRules(link.id));
                if (res.ok && !cancelled) {
                    const data = await res.json();
                    if (Array.isArray(data) && data.length) {
                        setRoutingRules(data);
                        setShowRouting(true);
                    }
                }
            } catch { /* non-fatal: editor starts empty */ }
        })();
        return () => { cancelled = true; };
    }, [routingEnabled, link.id]);

    const handleSave = async () => {
        setSaving(true);
        await onSave(link.id, {
            original_url: url !== link.original_url ? url : undefined,
            password: password || undefined,
            expires_at: expiresAt && !removeExpiration ? new Date(expiresAt).toISOString() : undefined,
            remove_password: removePassword || undefined,
            remove_expiration: removeExpiration || undefined,
            burn_after_reading: burnEnabled ? burnAfterReading : undefined,
            safe_link_interstitial: interstitialEnabled ? safeLinkInterstitial : undefined,
        });
        if (routingEnabled) {
            const rules = routingRules.filter(r => r.destination_url.trim());
            try {
                await authFetch(API_ENDPOINTS.linkRules(link.id), {
                    method: 'PUT',
                    body: JSON.stringify({ rules }),
                });
            } catch { /* surfaced on next open */ }
        }
        setSaving(false);
        onClose();
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
                className="bg-surface rounded-2xl border border-line2 shadow-lift max-w-lg w-full p-6"
                onClick={e => e.stopPropagation()}
            >
                <div className="flex items-center justify-between mb-6">
                    <h3 className="font-display text-xl font-bold text-ink tracking-tight">Edit link</h3>
                    <button onClick={onClose} aria-label="Close" className="text-faint transition-colors hover:text-ink">
                        <X className="h-5 w-5" />
                    </button>
                </div>

                <div className="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
                    <div>
                        <label className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Short link</label>
                        <div className="rounded-lg border border-line bg-paper px-4 py-2 font-mono text-sm text-ink">
                            {link.code}
                        </div>
                    </div>

                    <div>
                        <label htmlFor="edit-url" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Destination URL</label>
                        <input
                            id="edit-url"
                            type="url"
                            value={url}
                            onChange={(e) => setUrl(e.target.value)}
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500"
                        />
                    </div>

                    <div>
                        <label htmlFor="edit-password" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                            {link.has_password ? 'Change password' : 'Add password'}
                        </label>
                        <input
                            id="edit-password"
                            type="password"
                            value={password}
                            onChange={(e) => setPassword(e.target.value)}
                            placeholder={link.has_password ? 'Leave empty to keep current' : 'Optional'}
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                        />
                        {link.has_password && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-muted">
                                <input
                                    type="checkbox"
                                    checked={removePassword}
                                    onChange={(e) => setRemovePassword(e.target.checked)}
                                    className="rounded border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                Remove password protection
                            </label>
                        )}
                    </div>

                    <div>
                        <label htmlFor="edit-expires" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Expiration date</label>
                        <input
                            id="edit-expires"
                            type="date"
                            value={expiresAt}
                            onChange={(e) => setExpiresAt(e.target.value)}
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                            min={new Date().toISOString().split('T')[0]}
                        />
                        {link.expires_at && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-muted">
                                <input
                                    type="checkbox"
                                    checked={removeExpiration}
                                    onChange={(e) => setRemoveExpiration(e.target.checked)}
                                    className="rounded border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                Remove expiration date
                            </label>
                        )}
                    </div>

                    {burnEnabled && (
                        <div>
                            <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                                <input
                                    type="checkbox"
                                    checked={burnAfterReading}
                                    onChange={(e) => setBurnAfterReading(e.target.checked)}
                                    className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                <span className="inline-flex items-center gap-1.5">
                                    <Flame className="h-3.5 w-3.5 text-primary-600" />
                                    Burn after reading — opens once, then self-destructs
                                </span>
                            </label>
                            {link.burned_at && (
                                <p className="mt-1.5 text-xs text-danger">This link has already been burned.</p>
                            )}
                        </div>
                    )}

                    {interstitialEnabled && (
                        <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                            <input
                                type="checkbox"
                                checked={safeLinkInterstitial}
                                onChange={(e) => setSafeLinkInterstitial(e.target.checked)}
                                className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                            />
                            <span className="inline-flex items-center gap-1.5">
                                <ShieldCheck className="h-3.5 w-3.5 text-primary-600" />
                                Show a safety interstitial before redirecting
                            </span>
                        </label>
                    )}

                    {routingEnabled && (
                        <div className="border-t border-line pt-4">
                            <button
                                type="button"
                                onClick={() => setShowRouting(v => !v)}
                                className="flex w-full items-center justify-between text-sm font-medium text-muted transition-colors hover:text-ink"
                            >
                                <span className="inline-flex items-center gap-1.5">
                                    <Route className="h-3.5 w-3.5 text-primary-600" />
                                    Smart routing{routingRules.length > 0 ? ` (${routingRules.length})` : ''}
                                </span>
                                <ChevronDown className={`h-4 w-4 transition-transform ${showRouting ? 'rotate-180' : ''}`} />
                            </button>
                            {showRouting && (
                                <div className="mt-3">
                                    <RoutingRulesEditor rules={routingRules} onChange={setRoutingRules} />
                                </div>
                            )}
                        </div>
                    )}
                </div>

                <div className="flex gap-3 mt-6">
                    <button
                        onClick={onClose}
                        className="flex-1 rounded-lg border border-line2 px-4 py-2 font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
                    >
                        Cancel
                    </button>
                    <button
                        onClick={handleSave}
                        disabled={saving}
                        className="flex-1 rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                    >
                        {saving ? 'Saving…' : 'Save changes'}
                    </button>
                </div>
            </motion.div>
        </motion.div>
    );
}
