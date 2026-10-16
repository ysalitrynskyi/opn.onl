import { useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { X, Flame, ShieldCheck, Route, ChevronDown, LayoutList, MousePointer, Calendar } from 'lucide-react';
import { API_ENDPOINTS, authFetch, failureMessage, responseError } from '../../config/api';
import { useFocusTrap } from '../../hooks/useFocusTrap';
import { localIsoDate } from '../../utils/localIsoDate';
import type { LinkData, LinkUpdatePayload, RoutingRule } from './types';
import { swatchColor, type CatalogItem } from './linkScope';
import RoutingRulesEditor from './RoutingRulesEditor';
import { rulesPayload, rulesSignature, sortByPriority } from './routingRules';
import CharCount from './CharCount';
import { DestinationCheckButton, DestinationStatus } from './DestinationCheck';
import { useDestinationCheck } from './useDestinationCheck';
import {
    CLICK_LIMIT_HINT,
    MAX_NOTES_CHARS,
    MAX_TITLE_CHARS,
    START_BEFORE_EXPIRY,
    burnCap,
    burnCapHint,
    clickLimitUpdate,
    datetimeLocalToUtc,
    datetimeLocalValue,
    initialClickLimit,
} from './linkFields';

interface EditModalProps {
    link: LinkData;
    onClose: () => void;
    onSave: (id: number, data: LinkUpdatePayload) => Promise<void>;
    burnEnabled?: boolean;
    interstitialEnabled?: boolean;
    routingEnabled?: boolean;
    bioEnabled?: boolean;
    folders?: CatalogItem[];
    tags?: CatalogItem[];
}

/**
 * The expiry's calendar day where the user is, for the date input. The API
 * sends UTC instants; slicing the string took the UTC day, which is already
 * tomorrow for an evening expiry anywhere west of UTC.
 */
function expiryDateInputValue(value: string | null): string {
    if (!value) return '';
    const date = new Date(value);
    return Number.isNaN(date.getTime()) ? '' : localIsoDate(date);
}

export default function EditModal({
    link,
    onClose,
    onSave,
    burnEnabled = false,
    interstitialEnabled = false,
    routingEnabled = false,
    bioEnabled = false,
    folders = [],
    tags = [],
}: EditModalProps) {
    const [url, setUrl] = useState(link.original_url);
    const [title, setTitle] = useState(link.title ?? '');
    const [notes, setNotes] = useState(link.notes ?? '');
    const [password, setPassword] = useState('');
    const originalExpiryDate = expiryDateInputValue(link.expires_at);
    const [expiresAt, setExpiresAt] = useState(originalExpiryDate);
    const originalStart = datetimeLocalValue(link.starts_at);
    const [startsAt, setStartsAt] = useState(originalStart);
    const [maxClicks, setMaxClicks] = useState(initialClickLimit(link.max_clicks));
    const [limitTouched, setLimitTouched] = useState(false);
    const [removePassword, setRemovePassword] = useState(false);
    const [removeExpiration, setRemoveExpiration] = useState(false);
    const [removeStarts, setRemoveStarts] = useState(false);
    const [removeMaxClicks, setRemoveMaxClicks] = useState(false);
    const [burnAfterReading, setBurnAfterReading] = useState(link.burn_after_reading ?? false);
    const destination = useDestinationCheck(url);
    const [safeLinkInterstitial, setSafeLinkInterstitial] = useState(link.safe_link_interstitial ?? false);
    const [bioVisible, setBioVisible] = useState(link.bio_visible ?? false);
    // Kept as the link's folder id even when that folder is not in the list,
    // so a save does not clear a folder the select cannot draw.
    const [folderValue, setFolderValue] = useState(link.folder_id != null ? String(link.folder_id) : '');
    const [selectedTagIds, setSelectedTagIds] = useState<number[]>(() => (link.tags ?? []).map(tag => tag.id));
    const [routingRules, setRoutingRules] = useState<RoutingRule[]>([]);
    // Saving replaces every stored rule, so rules are only ever sent once the
    // stored ones have been read: an unread list looks the same as "no rules".
    const [rulesLoad, setRulesLoad] = useState<'loading' | 'loaded' | 'failed'>('loading');
    // Set when the rules could not load because the session ended, which a retry cannot fix.
    const [rulesSessionMessage, setRulesSessionMessage] = useState('');
    const [loadedSignature, setLoadedSignature] = useState('');
    const [rulesAttempt, setRulesAttempt] = useState(0);
    const [showRouting, setShowRouting] = useState(false);
    const [saving, setSaving] = useState(false);
    const [saveError, setSaveError] = useState('');
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);

    useEffect(() => {
        if (!routingEnabled) return;
        let cancelled = false;
        (async () => {
            try {
                const res = await authFetch(API_ENDPOINTS.linkRules(link.id));
                if (!res.ok) throw new Error(`Routing rules request failed (${res.status})`);
                const data: unknown = await res.json();
                if (!Array.isArray(data)) throw new Error('Unexpected routing rules response');
                if (cancelled) return;
                const loaded = sortByPriority(data as RoutingRule[]);
                setRoutingRules(loaded);
                setLoadedSignature(rulesSignature(loaded));
                setRulesLoad('loaded');
                if (loaded.length) setShowRouting(true);
            } catch (error) {
                if (cancelled) return;
                setRulesSessionMessage(failureMessage(error, ''));
                setRulesLoad('failed');
            }
        })();
        return () => { cancelled = true; };
    }, [routingEnabled, link.id, rulesAttempt]);

    const retryRulesLoad = () => {
        setRulesSessionMessage('');
        setRulesLoad('loading');
        setRulesAttempt(n => n + 1);
    };

    const handleSave = async () => {
        setSaveError('');

        const expirationChanged = expiresAt !== originalExpiryDate;
        const shouldRemoveExpiration = removeExpiration
            || (Boolean(link.expires_at) && expirationChanged && !expiresAt);
        const startChanged = startsAt !== originalStart;
        const shouldRemoveStart = removeStarts
            || (Boolean(link.starts_at) && startChanged && !startsAt);

        let nextExpiresIso: string | undefined;
        if (expirationChanged && expiresAt && !shouldRemoveExpiration) {
            const parsed = new Date(`${expiresAt}T23:59:00`);
            if (Number.isNaN(parsed.getTime())) {
                setSaveError('Enter a valid expiration');
                return;
            }
            // Date-only value: end of that local day, matching the create form.
            nextExpiresIso = parsed.toISOString();
        }

        let nextStartsIso: string | undefined;
        if (startChanged && startsAt && !shouldRemoveStart) {
            const parsed = datetimeLocalToUtc(startsAt);
            if (!parsed) {
                setSaveError('Enter a valid start date');
                return;
            }
            nextStartsIso = parsed;
        }

        const datesTouched = startChanged || expirationChanged || removeStarts || removeExpiration;
        if (datesTouched) {
            const startInstant = shouldRemoveStart ? null : (nextStartsIso ?? (startChanged ? null : link.starts_at));
            const expiryInstant = shouldRemoveExpiration ? null : (nextExpiresIso ?? (expirationChanged ? null : link.expires_at));
            const startMs = startInstant ? new Date(startInstant).getTime() : NaN;
            const expiryMs = expiryInstant ? new Date(expiryInstant).getTime() : NaN;
            if (!Number.isNaN(startMs) && !Number.isNaN(expiryMs) && startMs >= expiryMs) {
                setSaveError(START_BEFORE_EXPIRY);
                return;
            }
        }

        const limit = clickLimitUpdate({
            raw: maxClicks,
            stored: link.max_clicks ?? null,
            remove: removeMaxClicks,
            burnOn: burnAfterReading,
        });
        if ('error' in limit) {
            setSaveError(limit.error);
            return;
        }

        setSaving(true);
        const payload: LinkUpdatePayload = {
            original_url: url !== link.original_url ? url : undefined,
            password: password || undefined,
            remove_password: removePassword || undefined,
            remove_expiration: shouldRemoveExpiration || undefined,
            burn_after_reading: burnEnabled ? burnAfterReading : undefined,
            safe_link_interstitial: interstitialEnabled ? safeLinkInterstitial : undefined,
            bio_visible: bioEnabled ? bioVisible : undefined,
        };
        if (title !== (link.title ?? '')) payload.title = title;
        if (notes !== (link.notes ?? '')) payload.notes = notes;
        if (nextExpiresIso) payload.expires_at = nextExpiresIso;
        if (shouldRemoveStart) payload.remove_starts_at = true;
        else if (nextStartsIso) payload.starts_at = nextStartsIso;
        if (limit.max_clicks != null) payload.max_clicks = limit.max_clicks;
        if (limit.remove_max_clicks) payload.remove_max_clicks = true;

        const beforeFolder = link.folder_id ?? null;
        const nextFolder = folderValue ? Number(folderValue) : null;
        if (nextFolder !== beforeFolder) {
            if (nextFolder == null) payload.remove_folder = true;
            else payload.folder_id = nextFolder;
        }

        try {
            await onSave(link.id, payload);

            const beforeTags = (link.tags ?? []).map(tag => tag.id);
            const added = selectedTagIds.filter(id => !beforeTags.includes(id));
            const removed = beforeTags.filter(id => !selectedTagIds.includes(id));
            if (added.length > 0) {
                const response = await authFetch(API_ENDPOINTS.linkTags(link.id), {
                    method: 'POST',
                    body: JSON.stringify({ tag_ids: added }),
                });
                if (!response.ok) throw new Error(await responseError(response, 'Failed to update tags'));
            }
            if (removed.length > 0) {
                const response = await authFetch(API_ENDPOINTS.linkTags(link.id), {
                    method: 'DELETE',
                    body: JSON.stringify({ tag_ids: removed }),
                });
                if (!response.ok) throw new Error(await responseError(response, 'Failed to update tags'));
            }

            const rules = routingRules.filter(r => r.destination_url.trim());
            if (routingEnabled && rulesLoad === 'loaded' && rulesSignature(rules) !== loadedSignature) {
                const response = await authFetch(API_ENDPOINTS.linkRules(link.id), {
                    method: 'PUT',
                    body: JSON.stringify({ rules: rulesPayload(rules) }),
                });
                if (!response.ok) {
                    throw new Error(await responseError(response, 'Failed to update routing rules'));
                }
            }

            onClose();
            const editId = link.id;
            requestAnimationFrame(() => {
                document.querySelector<HTMLElement>(`[data-edit-link="${editId}"]`)?.focus();
            });
        } catch (error) {
            setSaveError(error instanceof Error ? error.message : 'Failed to update link');
        } finally {
            setSaving(false);
        }
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
                ref={dialogRef}
                role="dialog"
                aria-modal="true"
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
                        <div className="flex flex-col gap-2 sm:flex-row sm:items-start">
                            <input
                                id="edit-url"
                                type="url"
                                value={url}
                                onChange={(e) => setUrl(e.target.value)}
                                className="min-w-0 w-full flex-1 rounded-lg border border-line2 bg-surface px-4 py-2 font-mono text-sm text-ink outline-none transition-colors focus:border-primary-500"
                            />
                            <DestinationCheckButton
                                pending={destination.pending}
                                disabled={!url.trim()}
                                onClick={() => void destination.check()}
                                className="py-2 w-full sm:w-auto"
                            />
                        </div>
                        <div className="mt-1.5">
                            <DestinationStatus status={destination.status} />
                        </div>
                    </div>

                    <div>
                        <label htmlFor="edit-title" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                            Title
                        </label>
                        <input
                            id="edit-title"
                            type="text"
                            value={title}
                            maxLength={MAX_TITLE_CHARS}
                            onChange={(e) => setTitle(e.target.value.slice(0, MAX_TITLE_CHARS))}
                            placeholder="Private unless the link is on your bio page"
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint"
                        />
                    </div>

                    <div>
                        <label htmlFor="edit-notes" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Notes</label>
                        <textarea
                            id="edit-notes"
                            rows={3}
                            maxLength={MAX_NOTES_CHARS}
                            value={notes}
                            onChange={(e) => setNotes(e.target.value.slice(0, MAX_NOTES_CHARS))}
                            placeholder="Private notes about this link"
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint resize-y"
                        />
                        <CharCount length={notes.length} max={MAX_NOTES_CHARS} />
                    </div>

                    <div className="grid sm:grid-cols-2 gap-4">
                        <div>
                            <label htmlFor="edit-folder" className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Folder</label>
                            <select
                                id="edit-folder"
                                value={folderValue}
                                onChange={(event) => setFolderValue(event.target.value)}
                                className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
                            >
                                <option value="">No folder</option>
                                {folders.map(folder => (
                                    <option key={folder.id} value={folder.id}>{folder.name}</option>
                                ))}
                                {link.folder_id != null && !folders.some(folder => folder.id === link.folder_id) && (
                                    <option value={link.folder_id}>Current folder</option>
                                )}
                            </select>
                        </div>
                        <fieldset>
                            <legend className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">Tags</legend>
                            {tags.length === 0 ? (
                                <p className="text-sm text-muted">No tags yet</p>
                            ) : (
                                <div className="flex flex-wrap gap-2">
                                    {tags.map(tag => {
                                        const color = swatchColor(tag.color);
                                        return (
                                            <label key={tag.id} className="inline-flex items-center gap-1.5 text-sm text-muted">
                                                <input
                                                    type="checkbox"
                                                    checked={selectedTagIds.includes(tag.id)}
                                                    onChange={() => {
                                                        setSelectedTagIds(current => (
                                                            current.includes(tag.id)
                                                                ? current.filter(id => id !== tag.id)
                                                                : [...current, tag.id]
                                                        ));
                                                    }}
                                                    className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                                                />
                                                {color && (
                                                    <span className="h-2 w-2 rounded-full" style={{ backgroundColor: color }} aria-hidden="true" />
                                                )}
                                                {tag.name}
                                            </label>
                                        );
                                    })}
                                </div>
                            )}
                        </fieldset>
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
                            min={localIsoDate()}
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

                    <div>
                        <label htmlFor="edit-starts" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                            <Calendar className="h-3.5 w-3.5" />
                            Start date
                        </label>
                        <input
                            id="edit-starts"
                            type="datetime-local"
                            step={60}
                            value={startsAt}
                            onChange={(e) => setStartsAt(e.target.value)}
                            disabled={removeStarts}
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 disabled:opacity-70"
                        />
                        {link.starts_at && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-muted">
                                <input
                                    type="checkbox"
                                    checked={removeStarts}
                                    onChange={(e) => setRemoveStarts(e.target.checked)}
                                    className="rounded border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                Remove start date
                            </label>
                        )}
                    </div>

                    <div>
                        <label htmlFor="edit-max-clicks" className="flex items-center gap-1.5 font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                            <MousePointer className="h-3.5 w-3.5" />
                            Click limit
                        </label>
                        <input
                            id="edit-max-clicks"
                            type="number"
                            min={1}
                            step={1}
                            inputMode="numeric"
                            disabled={burnAfterReading || removeMaxClicks}
                            aria-describedby="edit-max-clicks-hint"
                            placeholder={link.max_clicks != null ? 'Leave empty to keep the current limit' : 'No limit'}
                            value={burnAfterReading ? String(burnCap(link.max_clicks)) : maxClicks}
                            onChange={(e) => {
                                setLimitTouched(true);
                                setMaxClicks(e.target.value);
                            }}
                            className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint disabled:opacity-70"
                        />
                        <p id="edit-max-clicks-hint" className="text-xs text-faint mt-1.5">
                            {burnAfterReading ? burnCapHint(burnCap(link.max_clicks)) : CLICK_LIMIT_HINT}
                        </p>
                        {link.max_clicks != null && (
                            <label className="flex items-center gap-2 mt-2 text-sm text-muted">
                                <input
                                    type="checkbox"
                                    checked={burnAfterReading ? false : removeMaxClicks}
                                    disabled={burnAfterReading}
                                    onChange={(e) => setRemoveMaxClicks(e.target.checked)}
                                    className="rounded border-line2 text-primary-600 focus:ring-primary-500 disabled:opacity-70"
                                />
                                Remove limit
                            </label>
                        )}
                    </div>

                    {burnEnabled && (
                        <div>
                            <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                                <input
                                    type="checkbox"
                                    checked={burnAfterReading}
                                    onChange={(e) => {
                                        const next = e.target.checked;
                                        setBurnAfterReading(next);
                                        // A remove sent beside burn-on is ignored by the server
                                        // and would look like the cap was cleared.
                                        if (next) setRemoveMaxClicks(false);
                                        if (!next && (link.burn_after_reading ?? false) && !limitTouched) {
                                            setMaxClicks('');
                                        }
                                    }}
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

                    {bioEnabled && (
                        <label className="flex items-center gap-2.5 text-sm text-muted cursor-pointer">
                            <input
                                type="checkbox"
                                checked={bioVisible}
                                onChange={(e) => setBioVisible(e.target.checked)}
                                className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                            />
                            <span className="inline-flex items-center gap-1.5">
                                <LayoutList className="h-3.5 w-3.5 text-primary-600" />
                                Show this link on my bio page
                            </span>
                        </label>
                    )}

                    {routingEnabled && (
                        <div className="border-t border-line pt-4">
                            <button
                                type="button"
                                onClick={() => setShowRouting(v => !v)}
                                disabled={rulesLoad === 'failed'}
                                className="flex w-full items-center justify-between text-sm font-medium text-muted transition-colors hover:text-ink disabled:cursor-not-allowed disabled:opacity-60 disabled:hover:text-muted"
                            >
                                <span className="inline-flex items-center gap-1.5">
                                    <Route className="h-3.5 w-3.5 text-primary-600" />
                                    Smart routing{routingRules.length > 0 ? ` (${routingRules.length})` : ''}
                                </span>
                                <ChevronDown className={`h-4 w-4 transition-transform ${showRouting ? 'rotate-180' : ''}`} />
                            </button>
                            {rulesLoad === 'failed' && (
                                <div role="alert" className="mt-3 rounded-lg border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">
                                    {rulesSessionMessage ? (
                                        <p>{rulesSessionMessage}</p>
                                    ) : (
                                        <>
                                            <p>
                                                Couldn't load this link's routing rules, so they can't be edited right now.
                                                The rules it has stay as they are and your other changes still save.
                                            </p>
                                            <button type="button" onClick={retryRulesLoad} className="mt-1 font-medium underline">
                                                Try again
                                            </button>
                                        </>
                                    )}
                                </div>
                            )}
                            {showRouting && rulesLoad !== 'failed' && (
                                <div className="mt-3">
                                    {rulesLoad === 'loading' ? (
                                        <p className="text-xs text-faint">Loading routing rules…</p>
                                    ) : (
                                        <RoutingRulesEditor rules={routingRules} onChange={setRoutingRules} />
                                    )}
                                </div>
                            )}
                        </div>
                    )}
                </div>

                {saveError && (
                    <div role="alert" className="mt-4 rounded-lg border border-danger/30 bg-danger/5 px-3 py-2 text-sm text-danger">
                        {saveError}
                    </div>
                )}

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
