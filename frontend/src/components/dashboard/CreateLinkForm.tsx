import { useState, type FormEvent } from 'react';
import { Plus, ChevronDown, Lock, Calendar, Flame, ShieldCheck } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, SessionExpiredError, authFetch } from '../../config/api';
import logger from '../../utils/logger';
import { savePendingUrl } from '../../utils/pendingUrl';
import { localIsoDate } from '../../utils/localIsoDate';
import type { AppSettings } from './types';

interface CreateLinkFormProps {
    appSettings: AppSettings;
    /** Homepage URL taken before this form mounts. Ignored after the first render. */
    initialUrl: string;
    onCreated: () => void;
    onError: (message: string) => void;
}

export default function CreateLinkForm({ appSettings, initialUrl, onCreated, onError }: CreateLinkFormProps) {
    const [newUrl, setNewUrl] = useState(initialUrl);
    const [alias, setAlias] = useState('');
    const [title, setTitle] = useState('');
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState('');
    const [expiresTime, setExpiresTime] = useState('23:59');
    const [creating, setCreating] = useState(false);
    const [showAdvanced, setShowAdvanced] = useState(false);
    const [burnAfterReading, setBurnAfterReading] = useState(false);
    const [safeLinkInterstitial, setSafeLinkInterstitial] = useState(false);

    // Clipboard is read only from this click. A mount or focus listener prompts
    // (or, in some browsers, reads) without the user asking.
    const pasteClipboardUrl = async () => {
        try {
            const text = (await navigator.clipboard.readText()).trim();
            if (/^https?:\/\/.+/.test(text)) {
                setNewUrl(text);
                onError('');
            } else {
                onError('Clipboard does not contain an http(s) URL');
            }
        } catch {
            onError('Clipboard access was denied. Paste the URL into the field instead.');
        }
    };

    const handleCreate = async (e: FormEvent) => {
        e.preventDefault();
        setCreating(true);
        onError('');

        // Validate alias length if provided
        if (alias) {
            if (alias.length < appSettings.min_alias_length) {
                onError(`Alias must be at least ${appSettings.min_alias_length} characters`);
                setCreating(false);
                return;
            }
            if (alias.length > appSettings.max_alias_length) {
                onError(`Alias must be at most ${appSettings.max_alias_length} characters`);
                setCreating(false);
                return;
            }
            // Check for valid characters
            if (!/^[a-zA-Z0-9_-]+$/.test(alias)) {
                onError('Alias can only contain letters, numbers, hyphens, and underscores');
                setCreating(false);
                return;
            }
            // Check for leading/trailing special chars
            if (/^[-_]|[-_]$/.test(alias)) {
                onError('Alias cannot start or end with hyphen or underscore');
                setCreating(false);
                return;
            }
        }

        try {
            // Build expiration datetime with timezone
            let expirationDate: string | undefined;
            if (expiresAt) {
                const dateTime = `${expiresAt}T${expiresTime || '23:59'}:00`;
                expirationDate = new Date(dateTime).toISOString();
            }

            const res = await authFetch(API_ENDPOINTS.links, {
                method: 'POST',
                body: JSON.stringify({
                    original_url: newUrl,
                    custom_alias: alias || undefined,
                    title: title || undefined,
                    password: password || undefined,
                    expires_at: expirationDate,
                    burn_after_reading: burnAfterReading || undefined,
                    safe_link_interstitial: safeLinkInterstitial || undefined,
                }),
            });

            if (res.ok) {
                setNewUrl('');
                setAlias('');
                setTitle('');
                setPassword('');
                setExpiresAt('');
                setExpiresTime('23:59');
                setBurnAfterReading(false);
                setSafeLinkInterstitial(false);
                setShowAdvanced(false);
                onCreated();
            } else {
                const data = await res.json();
                onError(data.error || 'Failed to create link');
            }
        } catch (error) {
            logger.error('Failed to create link', error);
            if (error instanceof SessionExpiredError) {
                // The session ended while the page was open. authFetch is
                // sending the user to /login; the URL comes back with them.
                savePendingUrl(newUrl);
                onError(error.message);
            } else {
                onError('Network error. Please try again.');
            }
        } finally {
            setCreating(false);
        }
    };

    return (
        <div className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle mb-8">
            <h2 className="font-display text-lg font-bold text-ink tracking-tight mb-4">Create new link</h2>
            <form onSubmit={handleCreate}>
                <div className="flex flex-col sm:flex-row gap-3">
                    <input
                        type="url"
                        required
                        placeholder="https://example.com/long-url"
                        className="flex-1 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                        value={newUrl}
                        onChange={(e) => setNewUrl(e.target.value)}
                    />
                    <button
                        type="button"
                        onClick={pasteClipboardUrl}
                        className="inline-flex items-center justify-center rounded-lg border border-line2 bg-surface px-4 py-2.5 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        Paste
                    </button>
                    {appSettings.custom_aliases_enabled && (
                        <input
                            type="text"
                            placeholder={`alias (${appSettings.min_alias_length}-${appSettings.max_alias_length} chars)`}
                            className="sm:w-48 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                            value={alias}
                            onChange={(e) => setAlias(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ''))}
                            minLength={appSettings.min_alias_length}
                            maxLength={appSettings.max_alias_length}
                        />
                    )}
                    <button
                        type="submit"
                        disabled={creating}
                        className="inline-flex items-center justify-center gap-2 rounded-lg bg-primary-600 px-6 py-2.5 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                    >
                        {creating ? (
                            <div className="h-4 w-4 rounded-full border-2 border-white/30 border-t-white animate-spin" />
                        ) : (
                            <Plus className="h-4 w-4" />
                        )}
                        Create
                    </button>
                </div>

                <button
                    type="button"
                    onClick={() => setShowAdvanced(!showAdvanced)}
                    className="mt-3 inline-flex items-center gap-1 text-sm text-muted transition-colors hover:text-ink"
                >
                    <ChevronDown className={`h-4 w-4 transition-transform ${showAdvanced ? 'rotate-180' : ''}`} />
                    Advanced options
                </button>

                <AnimatePresence>
                    {showAdvanced && (
                        <motion.div
                            initial={{ height: 0, opacity: 0 }}
                            animate={{ height: 'auto', opacity: 1 }}
                            exit={{ height: 0, opacity: 0 }}
                            className="overflow-hidden"
                        >
                            <div className="space-y-4 mt-4 pt-4 border-t border-line">
                                {/* Title - full width */}
                                <div>
                                    <label htmlFor="create-title" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                        Title (private unless the link is on your bio page)
                                    </label>
                                    <input
                                        id="create-title"
                                        type="text"
                                        placeholder="e.g. Marketing campaign Q1"
                                        className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                        value={title}
                                        onChange={(e) => setTitle(e.target.value)}
                                        maxLength={100}
                                    />
                                </div>

                                <div className="grid sm:grid-cols-2 gap-4">
                                    <div>
                                        <label htmlFor="create-password" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                            <Lock className="h-3.5 w-3.5" />
                                            Password Protection
                                        </label>
                                        <input
                                            id="create-password"
                                            type="password"
                                            placeholder="Leave empty for no password"
                                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                            value={password}
                                            onChange={(e) => setPassword(e.target.value)}
                                        />
                                    </div>
                                    <div>
                                        <label htmlFor="create-expires" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                            <Calendar className="h-3.5 w-3.5" />
                                            Expiration
                                        </label>
                                        <div className="flex gap-2">
                                            <input
                                                id="create-expires"
                                                type="date"
                                                className="flex-1 rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                                                value={expiresAt}
                                                onChange={(e) => setExpiresAt(e.target.value)}
                                                min={localIsoDate()}
                                            />
                                            <input
                                                type="time"
                                                aria-label="Expiration time"
                                                className="w-28 rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                                                value={expiresTime}
                                                onChange={(e) => setExpiresTime(e.target.value)}
                                            />
                                        </div>
                                        <p className="text-xs text-faint mt-1.5">
                                            Your timezone: {Intl.DateTimeFormat().resolvedOptions().timeZone}
                                            {expiresAt && expiresTime && (
                                                <span className="ml-2">
                                                    → Expires: {new Date(`${expiresAt}T${expiresTime}`).toLocaleString()}
                                                </span>
                                            )}
                                        </p>
                                    </div>
                                </div>

                                {appSettings.burn_after_reading_enabled && (
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
                                )}

                                {appSettings.safe_link_interstitial_enabled && (
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
                            </div>
                        </motion.div>
                    )}
                </AnimatePresence>
            </form>
        </div>
    );
}
