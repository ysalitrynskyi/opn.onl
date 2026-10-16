import { useEffect, useState, type FormEvent } from 'react';
import { Plus, ChevronDown, Lock, Calendar, Flame, ShieldCheck, MousePointer } from 'lucide-react';
import { motion, AnimatePresence } from 'framer-motion';
import { API_ENDPOINTS, SessionExpiredError, authFetch, type CheckCodeResponse } from '../../config/api';
import logger from '../../utils/logger';
import { savePendingUrl } from '../../utils/pendingUrl';
import { localIsoDate } from '../../utils/localIsoDate';
import type { AppSettings } from './types';
import CharCount from './CharCount';
import UtmBuilder from './UtmBuilder';
import { DestinationCheckButton, DestinationStatus } from './DestinationCheck';
import { useDestinationCheck } from './useDestinationCheck';
import {
    CLICK_LIMIT_ERROR,
    CLICK_LIMIT_HINT,
    MAX_NOTES_CHARS,
    START_BEFORE_EXPIRY,
    aliasHintFrom,
    burnCapHint,
    datetimeLocalToUtc,
    parseClickLimit,
    type AliasHint,
} from './linkFields';

interface CreateLinkFormProps {
    appSettings: AppSettings;
    /** Homepage URL taken before this form mounts. Ignored after the first render. */
    initialUrl: string;
    onCreated: () => void;
    onError: (message: string) => void;
}

function isAbort(error: unknown): boolean {
    return typeof error === 'object' && error !== null && 'name' in error && (error as { name: unknown }).name === 'AbortError';
}

const fieldClass = 'w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint';
const labelText = 'font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5';
const labelClass = `block ${labelText}`;
const iconLabelClass = `flex items-center gap-1.5 ${labelText}`;

export default function CreateLinkForm({ appSettings, initialUrl, onCreated, onError }: CreateLinkFormProps) {
    const [newUrl, setNewUrl] = useState(initialUrl);
    const [alias, setAlias] = useState('');
    const [aliasHint, setAliasHint] = useState<AliasHint | null>(null);
    const [title, setTitle] = useState('');
    const [notes, setNotes] = useState('');
    const [password, setPassword] = useState('');
    const [expiresAt, setExpiresAt] = useState('');
    const [expiresTime, setExpiresTime] = useState('23:59');
    const [startsAt, setStartsAt] = useState('');
    const [maxClicks, setMaxClicks] = useState('');
    const [creating, setCreating] = useState(false);
    const [showAdvanced, setShowAdvanced] = useState(false);
    const [burnAfterReading, setBurnAfterReading] = useState(false);
    const [safeLinkInterstitial, setSafeLinkInterstitial] = useState(false);
    const destination = useDestinationCheck(newUrl);

    // Below the configured minimum the server only repeats the length rule,
    // so the form waits until the alias could actually be free or taken.
    useEffect(() => {
        if (!appSettings.custom_aliases_enabled) return;
        const code = alias;
        if (code.length < appSettings.min_alias_length) {
            setAliasHint(null);
            return;
        }
        // Drop the previous answer immediately so a slow response cannot sit
        // under an alias it was not about.
        setAliasHint(null);
        const controller = new AbortController();
        let ignore = false;
        const timer = window.setTimeout(async () => {
            try {
                const res = await authFetch(`${API_ENDPOINTS.checkCode}?${new URLSearchParams({ code })}`, {
                    signal: controller.signal,
                });
                if (ignore) return;
                if (!res.ok) {
                    setAliasHint(null);
                    return;
                }
                const data = await res.json() as CheckCodeResponse;
                if (ignore || data.code !== code) return;
                setAliasHint(aliasHintFrom(data));
            } catch (error) {
                if (ignore || isAbort(error)) return;
                setAliasHint(null);
            }
        }, 300);
        return () => {
            ignore = true;
            window.clearTimeout(timer);
            controller.abort();
        };
    }, [alias, appSettings.custom_aliases_enabled, appSettings.min_alias_length]);

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

        // Burn is stored as a one-click cap when no cap is sent. A number here
        // would replace that, so the field is disabled and left out of the body.
        let clickLimit: number | null = null;
        if (!burnAfterReading && maxClicks.trim()) {
            const parsed = parseClickLimit(maxClicks);
            if (!parsed.ok) {
                onError(CLICK_LIMIT_ERROR);
                setCreating(false);
                return;
            }
            clickLimit = parsed.value;
        }

        let expirationDate: string | undefined;
        if (expiresAt) {
            const parsed = new Date(`${expiresAt}T${expiresTime || '23:59'}:00`);
            if (Number.isNaN(parsed.getTime())) {
                onError('Enter a valid expiration');
                setCreating(false);
                return;
            }
            expirationDate = parsed.toISOString();
        }

        let startDate: string | undefined;
        if (startsAt) {
            const parsed = datetimeLocalToUtc(startsAt);
            if (!parsed) {
                onError('Enter a valid start date');
                setCreating(false);
                return;
            }
            startDate = parsed;
        }
        if (startDate && expirationDate && new Date(startDate).getTime() >= new Date(expirationDate).getTime()) {
            onError(START_BEFORE_EXPIRY);
            setCreating(false);
            return;
        }

        const body: Record<string, unknown> = { original_url: newUrl };
        if (alias) body.custom_alias = alias;
        if (title) body.title = title;
        if (notes) body.notes = notes;
        if (password) body.password = password;
        if (expirationDate) body.expires_at = expirationDate;
        if (startDate) body.starts_at = startDate;
        if (clickLimit != null) body.max_clicks = clickLimit;
        if (burnAfterReading) body.burn_after_reading = true;
        if (safeLinkInterstitial) body.safe_link_interstitial = true;

        try {
            const res = await authFetch(API_ENDPOINTS.links, {
                method: 'POST',
                body: JSON.stringify(body),
            });

            if (res.ok) {
                setNewUrl('');
                setAlias('');
                setTitle('');
                setNotes('');
                setPassword('');
                setExpiresAt('');
                setExpiresTime('23:59');
                setStartsAt('');
                setMaxClicks('');
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

    const limitHint = burnAfterReading ? burnCapHint(1) : CLICK_LIMIT_HINT;

    return (
        <div className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle mb-8">
            <h2 className="font-display text-lg font-bold text-ink tracking-tight mb-4">Create new link</h2>
            <form onSubmit={handleCreate}>
                <div className="flex flex-col gap-3 sm:flex-row sm:items-start">
                    <label htmlFor="create-url" className="sr-only">Destination URL</label>
                    <input
                        id="create-url"
                        type="url"
                        required
                        placeholder="https://example.com/long-url"
                        className="min-w-0 w-full flex-1 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                        value={newUrl}
                        onChange={(e) => setNewUrl(e.target.value)}
                    />
                    <div className="flex flex-col gap-1.5 sm:shrink-0">
                        <DestinationCheckButton
                            pending={destination.pending}
                            disabled={!newUrl.trim()}
                            onClick={() => void destination.check()}
                        />
                        <DestinationStatus status={destination.status} />
                    </div>
                    <button
                        type="button"
                        onClick={pasteClipboardUrl}
                        className="inline-flex shrink-0 items-center justify-center rounded-lg border border-line2 bg-surface px-4 py-2.5 text-sm font-medium text-ink transition-colors hover:border-ink/30"
                    >
                        Paste
                    </button>
                    {appSettings.custom_aliases_enabled && (
                        <>
                            <label htmlFor="create-alias" className="sr-only">Custom alias</label>
                            <input
                                id="create-alias"
                                type="text"
                                placeholder={`alias (${appSettings.min_alias_length}-${appSettings.max_alias_length} chars)`}
                                aria-describedby={aliasHint ? 'create-alias-hint' : undefined}
                                className="w-full sm:w-48 rounded-lg border border-line2 bg-surface px-4 py-2.5 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                                value={alias}
                                onChange={(e) => setAlias(e.target.value.replace(/[^a-zA-Z0-9_-]/g, ''))}
                                minLength={appSettings.min_alias_length}
                                maxLength={appSettings.max_alias_length}
                            />
                        </>
                    )}
                    <button
                        type="submit"
                        disabled={creating}
                        className="inline-flex shrink-0 items-center justify-center gap-2 rounded-lg bg-primary-600 px-6 py-2.5 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-70"
                    >
                        {creating ? (
                            <div className="h-4 w-4 rounded-full border-2 border-white/30 border-t-white animate-spin" />
                        ) : (
                            <Plus className="h-4 w-4" />
                        )}
                        Create
                    </button>
                </div>
                {aliasHint && (
                    <p
                        id="create-alias-hint"
                        role="status"
                        className={`mt-2 text-xs ${aliasHint.ok ? 'text-success' : 'text-danger'}`}
                    >
                        {aliasHint.text}
                    </p>
                )}

                <button
                    type="button"
                    aria-expanded={showAdvanced}
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
                                    <label htmlFor="create-title" className={labelClass}>
                                        Title (private unless the link is on your bio page)
                                    </label>
                                    <input
                                        id="create-title"
                                        type="text"
                                        placeholder="e.g. Marketing campaign Q1"
                                        className={fieldClass}
                                        value={title}
                                        onChange={(e) => setTitle(e.target.value)}
                                        maxLength={100}
                                    />
                                </div>

                                <div>
                                    <label htmlFor="create-notes" className={labelClass}>Notes</label>
                                    <textarea
                                        id="create-notes"
                                        rows={3}
                                        maxLength={MAX_NOTES_CHARS}
                                        placeholder="Private notes about this link"
                                        className={`${fieldClass} py-2.5 resize-y`}
                                        value={notes}
                                        onChange={(e) => setNotes(e.target.value.slice(0, MAX_NOTES_CHARS))}
                                    />
                                    <CharCount length={notes.length} max={MAX_NOTES_CHARS} />
                                </div>

                                <div className="grid sm:grid-cols-2 gap-4">
                                    <div>
                                        <label htmlFor="create-password" className={iconLabelClass}>
                                            <Lock className="h-3.5 w-3.5" />
                                            Password Protection
                                        </label>
                                        <input
                                            id="create-password"
                                            type="password"
                                            placeholder="Leave empty for no password"
                                            className={fieldClass}
                                            value={password}
                                            onChange={(e) => setPassword(e.target.value)}
                                        />
                                    </div>
                                    <div>
                                        <label htmlFor="create-expires" className={iconLabelClass}>
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

                                <div className="grid sm:grid-cols-2 gap-4">
                                    <div>
                                        <label htmlFor="create-starts" className={iconLabelClass}>
                                            <Calendar className="h-3.5 w-3.5" />
                                            Start date
                                        </label>
                                        <input
                                            id="create-starts"
                                            type="datetime-local"
                                            step={60}
                                            className={fieldClass}
                                            value={startsAt}
                                            onChange={(e) => setStartsAt(e.target.value)}
                                        />
                                        <p className="text-xs text-faint mt-1.5">
                                            Your timezone: {Intl.DateTimeFormat().resolvedOptions().timeZone}
                                            {startsAt && !Number.isNaN(new Date(startsAt).getTime()) && (
                                                <span className="ml-2">
                                                    → Starts: {new Date(startsAt).toLocaleString()}
                                                </span>
                                            )}
                                        </p>
                                    </div>
                                    <div>
                                        <label htmlFor="create-max-clicks" className={iconLabelClass}>
                                            <MousePointer className="h-3.5 w-3.5" />
                                            Click limit
                                        </label>
                                        <input
                                            id="create-max-clicks"
                                            type="number"
                                            inputMode="numeric"
                                            disabled={burnAfterReading}
                                            aria-describedby="create-max-clicks-hint"
                                            placeholder="No limit"
                                            className={`${fieldClass} disabled:opacity-70`}
                                            value={burnAfterReading ? '1' : maxClicks}
                                            onChange={(e) => setMaxClicks(e.target.value)}
                                        />
                                        <p id="create-max-clicks-hint" className="text-xs text-faint mt-1.5">
                                            {limitHint}
                                        </p>
                                    </div>
                                </div>

                                <UtmBuilder url={newUrl} onApply={setNewUrl} />

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
