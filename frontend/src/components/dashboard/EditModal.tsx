import { useState } from 'react';
import { motion } from 'framer-motion';
import { X } from 'lucide-react';
import type { LinkData, LinkUpdatePayload } from './types';

interface EditModalProps {
    link: LinkData;
    onClose: () => void;
    onSave: (id: number, data: LinkUpdatePayload) => Promise<void>;
}

export default function EditModal({ link, onClose, onSave }: EditModalProps) {
    const [url, setUrl] = useState(link.original_url);
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState(link.expires_at?.split('T')[0] || '');
    const [removePassword, setRemovePassword] = useState(false);
    const [removeExpiration, setRemoveExpiration] = useState(false);
    const [saving, setSaving] = useState(false);

    const handleSave = async () => {
        setSaving(true);
        await onSave(link.id, {
            original_url: url !== link.original_url ? url : undefined,
            password: password || undefined,
            expires_at: expiresAt && !removeExpiration ? new Date(expiresAt).toISOString() : undefined,
            remove_password: removePassword || undefined,
            remove_expiration: removeExpiration || undefined,
        });
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

                <div className="space-y-4">
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
