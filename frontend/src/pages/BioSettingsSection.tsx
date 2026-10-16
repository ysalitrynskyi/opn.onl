import { useCallback, useEffect, useRef, useState } from 'react';
import { motion } from 'framer-motion';
import { ArrowDown, ArrowUp, Loader2, User } from 'lucide-react';
import { API_ENDPOINTS, authFetch, responseError, shortLinkHost, totalCount } from '../config/api';
import { MAX_BIO_ORDER, bioOrderFits } from './bioOrder';
import { isSafeHttpUrl } from '../utils';
import { pluralize } from '../utils/plural';
import { BIO_THEMES, bioThemeId, type BioThemeId } from './bioThemes';

const MAX_AVATAR_URL_CHARS = 2048;
const MAX_BIO_LABEL_CHARS = 80;
/** `GET /links` never returns more than this. */
const BIO_LINK_PAGE = 1000;
const MAX_BIO_PAGES = 20;
const INCOMPLETE_LINKS = 'Could not load every link on your bio page, so they cannot be reordered.';

interface BioLinkRow {
    id: number;
    code: string;
    title: string | null;
    bio_visible?: boolean;
    bio_label?: string | null;
    bio_position?: number | null;
    org_id?: number | null;
    created_at?: string;
}

interface BioCarrier {
    id: number;
    avatar_url: string | null;
    bio_username?: string | null;
    bio_enabled?: boolean;
    bio_theme?: string | null;
}

interface BioSettingsSectionProps<T extends BioCarrier> {
    profile: T;
    onProfile: (next: T) => void;
    onError: (message: string) => void;
    onSuccess: (message: string) => void;
    inputClass: string;
    labelClass: string;
}

function isBioLink(value: unknown): value is BioLinkRow {
    if (!value || typeof value !== 'object') return false;
    const link = value as BioLinkRow;
    return typeof link.id === 'number' && typeof link.code === 'string';
}

/** Milliseconds since the epoch. Missing or unparseable instants sort last
 *  among a tied position (oldest). Comparing the RFC 3339 text is wrong:
 *  `…T00:00:00.1Z` sorts after `…T00:00:00.12Z` because `Z` sorts after `2`,
 *  but 0.1s is earlier than 0.12s. */
function createdAtMillis(value: string | undefined): number {
    if (!value) return Number.NEGATIVE_INFINITY;
    const parsed = Date.parse(value);
    return Number.isNaN(parsed) ? Number.NEGATIVE_INFINITY : parsed;
}

/** Same order as `GET /api/bio/:username`: position ascending, nulls last,
 *  then newest instant, then highest id. */
function sortBioLinks(links: BioLinkRow[]): BioLinkRow[] {
    return [...links].sort((a, b) => {
        const ap = a.bio_position ?? Number.POSITIVE_INFINITY;
        const bp = b.bio_position ?? Number.POSITIVE_INFINITY;
        if (ap !== bp) return ap - bp;
        const at = createdAtMillis(a.created_at);
        const bt = createdAtMillis(b.created_at);
        if (at !== bt) return bt - at;
        return b.id - a.id;
    });
}

function linkName(link: BioLinkRow): string {
    const label = link.bio_label?.trim();
    return label || link.title || link.code;
}

function shownError(err: unknown, fallback: string): string {
    return err instanceof Error && err.message ? err.message : fallback;
}

export default function BioSettingsSection<T extends BioCarrier>({
    profile,
    onProfile,
    onError,
    onSuccess,
    inputClass,
    labelClass,
}: BioSettingsSectionProps<T>) {
    const [bioUsername, setBioUsername] = useState(profile.bio_username ?? '');
    const [bioEnabled, setBioEnabled] = useState(Boolean(profile.bio_enabled));
    const [bioTheme, setBioTheme] = useState<BioThemeId>(bioThemeId(profile.bio_theme));
    const [avatarUrl, setAvatarUrl] = useState(profile.avatar_url ?? '');
    const [avatarFailed, setAvatarFailed] = useState(false);
    const [savingBio, setSavingBio] = useState(false);
    const [bioLinks, setBioLinks] = useState<BioLinkRow[]>([]);
    const [orgLinkCount, setOrgLinkCount] = useState(0);
    const [labelDrafts, setLabelDrafts] = useState<Record<number, string>>({});
    const labelDraftsRef = useRef(labelDrafts);
    // What the server holds for each label. A save that returns after the
    // person typed on is compared against this, not against the row the
    // click handler closed over.
    const storedLabels = useRef<Record<number, string>>({});
    const labelInFlight = useRef<Set<number>>(new Set());
    const labelPending = useRef<Set<number>>(new Set());
    const [linksAsked, setLinksAsked] = useState(false);
    const [linksLoading, setLinksLoading] = useState(false);
    const [linksReady, setLinksReady] = useState(false);
    const [linksError, setLinksError] = useState<string | null>(null);
    const [savingOrder, setSavingOrder] = useState(false);
    // The list is a burst of its own. Skip it on a Settings visit that is not
    // about the bio page: the account page already loads four resources, and
    // one more trips the per-second limit.
    const wantsLinks = Boolean(profile.bio_enabled) || linksAsked;

    const replaceDrafts = useCallback((next: Record<number, string>) => {
        labelDraftsRef.current = next;
        setLabelDrafts(next);
    }, []);

    useEffect(() => {
        if (!wantsLinks) return;
        let cancelled = false;
        (async () => {
            setLinksLoading(true);
            setLinksReady(false);
            setLinksError(null);
            try {
                const collected: BioLinkRow[] = [];
                let offset = 0;
                let incomplete = false;
                for (let page = 0; page < MAX_BIO_PAGES; page += 1) {
                    const params = new URLSearchParams({
                        bio_visible: 'true',
                        limit: String(BIO_LINK_PAGE),
                        offset: String(offset),
                    });
                    const res = await authFetch(`${API_ENDPOINTS.links}?${params}`);
                    if (!res.ok) throw new Error(await responseError(res, 'Failed to load links'));
                    const data: unknown = await res.json();
                    if (cancelled) return;
                    const rows = Array.isArray(data) ? data.filter(isBioLink) : [];
                    const total = totalCount(res);
                    collected.push(...rows);
                    const pageShort = rows.length < BIO_LINK_PAGE;
                    const reachedTotal = total != null && collected.length >= total;
                    if (pageShort || reachedTotal || rows.length === 0) {
                        if (total != null && collected.length < total) incomplete = true;
                        break;
                    }
                    // A full page and no total: another page may exist, and
                    // ordering a partial id list is rejected.
                    if (total == null || page + 1 >= MAX_BIO_PAGES) {
                        incomplete = true;
                        break;
                    }
                    offset += rows.length;
                }
                if (cancelled) return;
                const personal = sortBioLinks(collected.filter((link) => link.org_id == null && link.bio_visible === true));
                setBioLinks(personal);
                setOrgLinkCount(collected.filter((link) => link.org_id != null && link.bio_visible === true).length);
                storedLabels.current = Object.fromEntries(personal.map((link) => [link.id, link.bio_label ?? '']));
                replaceDrafts({ ...storedLabels.current });
                if (incomplete) {
                    setLinksError(INCOMPLETE_LINKS);
                    return;
                }
                setLinksReady(true);
            } catch (err) {
                if (cancelled) return;
                const message = shownError(err, 'Failed to load links');
                setBioLinks([]);
                setOrgLinkCount(0);
                setLinksError(message);
            } finally {
                if (!cancelled) setLinksLoading(false);
            }
        })();
        return () => { cancelled = true; };
    }, [profile.id, wantsLinks, replaceDrafts]);

    useEffect(() => {
        setAvatarFailed(false);
    }, [profile.avatar_url]);

    const handleSaveBio = async (e: React.FormEvent) => {
        e.preventDefault();
        setSavingBio(true);
        onError('');
        onSuccess('');
        try {
            const nextAvatar = avatarUrl.trim();
            const savedAvatar = (profile.avatar_url ?? '').trim();
            if (nextAvatar && !isSafeHttpUrl(nextAvatar)) {
                throw new Error('Avatar must be an http or https URL');
            }
            if (nextAvatar.length > MAX_AVATAR_URL_CHARS) {
                throw new Error('Avatar URL must be at most 2048 characters');
            }

            const res = await authFetch(API_ENDPOINTS.bioSettings, {
                method: 'PUT',
                body: JSON.stringify({
                    bio_username: bioUsername || '',
                    bio_enabled: bioEnabled,
                    bio_theme: bioTheme,
                }),
            });
            if (!res.ok) throw new Error(await responseError(res, 'Failed to save bio settings'));
            const data = await res.json() as {
                bio_username?: string | null;
                bio_enabled?: boolean;
                bio_theme?: string | null;
            };
            let next: T = {
                ...profile,
                bio_username: data.bio_username ?? null,
                bio_enabled: Boolean(data.bio_enabled),
                bio_theme: data.bio_theme ?? null,
            };
            setBioUsername(next.bio_username ?? '');
            setBioEnabled(Boolean(next.bio_enabled));
            setBioTheme(bioThemeId(next.bio_theme));

            if (nextAvatar !== savedAvatar) {
                const avatarRes = await authFetch(API_ENDPOINTS.updateProfile, {
                    method: 'PUT',
                    body: JSON.stringify({ avatar_url: nextAvatar }),
                });
                if (!avatarRes.ok) {
                    onProfile(next);
                    throw new Error(await responseError(avatarRes, 'Failed to save avatar'));
                }
                const saved = await avatarRes.json() as { avatar_url?: string | null };
                next = { ...next, avatar_url: saved.avatar_url ?? null };
                setAvatarUrl(next.avatar_url ?? '');
            }
            onProfile(next);
            onSuccess('Bio page settings saved');
        } catch (err) {
            onError(shownError(err, 'Failed to save bio settings'));
        } finally {
            setSavingBio(false);
        }
    };

    const saveLabel = async (link: BioLinkRow) => {
        const id = link.id;
        const currentDraft = () => labelDraftsRef.current[id] ?? '';
        // One request per link. A blur while that request is out remembers to
        // save again when it returns, and the late body must not replace what
        // the person has typed since.
        if (labelInFlight.current.has(id)) {
            labelPending.current.add(id);
            return;
        }
        const sent = currentDraft();
        if (sent === (storedLabels.current[id] ?? '')) return;
        labelInFlight.current.add(id);
        onError('');
        try {
            const res = await authFetch(API_ENDPOINTS.linkUpdate(id), {
                method: 'PUT',
                body: JSON.stringify({ bio_label: sent }),
            });
            if (!res.ok) throw new Error(await responseError(res, 'Failed to save label'));
            const saved = await res.json() as { bio_label?: string | null };
            const stored = saved.bio_label ?? null;
            storedLabels.current = { ...storedLabels.current, [id]: stored ?? '' };
            setBioLinks((prev) => prev.map((row) => (row.id === id ? { ...row, bio_label: stored } : row)));
            // The server may have trimmed the text. Show its version unless
            // the person has typed on since.
            if (currentDraft() === sent) replaceDrafts({ ...labelDraftsRef.current, [id]: stored ?? '' });
        } catch (err) {
            onError(shownError(err, 'Failed to save label'));
        } finally {
            labelInFlight.current.delete(id);
            if (labelPending.current.delete(id) && currentDraft() !== (storedLabels.current[id] ?? '')) {
                void saveLabel(link);
            }
        }
    };

    const reorderLocked = linksError != null || !linksReady || !bioOrderFits(bioLinks.length);
    const moveLink = async (index: number, direction: -1 | 1) => {
        const nextIndex = index + direction;
        if (reorderLocked || nextIndex < 0 || nextIndex >= bioLinks.length || savingOrder) return;
        const previous = bioLinks;
        const reordered = [...bioLinks];
        const [moved] = reordered.splice(index, 1);
        reordered.splice(nextIndex, 0, moved);
        setBioLinks(reordered);
        setSavingOrder(true);
        onError('');
        try {
            const res = await authFetch(API_ENDPOINTS.bioOrder, {
                method: 'PUT',
                body: JSON.stringify({ link_ids: reordered.map((link) => link.id) }),
            });
            if (!res.ok) throw new Error(await responseError(res, 'Failed to reorder links'));
            setBioLinks(reordered.map((link, position) => ({ ...link, bio_position: position })));
        } catch (err) {
            setBioLinks(previous);
            onError(shownError(err, 'Failed to reorder links'));
        } finally {
            setSavingOrder(false);
        }
    };

    const savedAvatar = (profile.avatar_url ?? '').trim();
    const previewReady = Boolean(profile.bio_enabled)
        && savedAvatar !== ''
        && avatarUrl.trim() === savedAvatar
        && isSafeHttpUrl(savedAvatar);
    const initial = (bioUsername || profile.bio_username || '?').replace('@', '').charAt(0).toUpperCase() || '?';

    return (
        <motion.section
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            transition={{ delay: 0.18 }}
            className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden"
        >
            <div className="p-6 border-b border-line">
                <div className="flex items-center gap-3">
                    <div className="flex h-10 w-10 items-center justify-center rounded-full border border-line bg-paper">
                        <User className="h-5 w-5 text-muted" />
                    </div>
                    <div>
                        <h2 className="font-display text-lg font-bold text-ink tracking-tight">Public profile</h2>
                        <p className="text-sm text-muted">A link-in-bio page for your links — off until you turn it on</p>
                    </div>
                </div>
            </div>
            <form onSubmit={handleSaveBio} className="p-6 space-y-4">
                <div>
                    <label htmlFor="bio-username" className="block text-sm font-medium text-ink mb-1.5">Username</label>
                    <div className="flex items-center gap-2">
                        <span className="text-sm text-faint">{shortLinkHost()}/@</span>
                        <input
                            id="bio-username"
                            type="text"
                            value={bioUsername}
                            onChange={(e) => setBioUsername(e.target.value.toLowerCase().replace(/[^a-z0-9_-]/g, ''))}
                            placeholder="yourname"
                            maxLength={30}
                            className={inputClass}
                        />
                    </div>
                </div>
                <label className="flex items-center gap-2.5 text-sm text-ink cursor-pointer">
                    <input
                        type="checkbox"
                        checked={bioEnabled}
                        onChange={(e) => setBioEnabled(e.target.checked)}
                        className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
                    />
                    Make my bio page public
                </label>
                <fieldset>
                    <legend className={labelClass}>Theme</legend>
                    <div className="flex flex-wrap gap-2">
                        {BIO_THEMES.map((theme) => (
                            <label
                                key={theme.id || 'light'}
                                className={`inline-flex items-center gap-2 rounded-lg border px-3 py-2 text-sm cursor-pointer ${bioTheme === theme.id ? 'border-primary-500 bg-primary-50 text-ink' : 'border-line2 text-ink'}`}
                            >
                                <input
                                    type="radio"
                                    name="bio-theme"
                                    value={theme.id}
                                    checked={bioTheme === theme.id}
                                    onChange={() => setBioTheme(theme.id)}
                                    className="h-4 w-4 border-line2 text-primary-600 focus:ring-primary-500"
                                />
                                <span
                                    aria-hidden="true"
                                    className={`h-3.5 w-3.5 rounded-full border border-line2 ${theme.id === 'ink' ? 'bg-ink' : theme.id === 'cobalt' ? 'bg-primary-700' : 'bg-surface'}`}
                                />
                                {theme.name}
                            </label>
                        ))}
                    </div>
                </fieldset>
                <div>
                    <label htmlFor="bio-avatar" className={labelClass}>Avatar URL</label>
                    <div className="flex items-center gap-3">
                        {previewReady && !avatarFailed ? (
                            <img
                                src={API_ENDPOINTS.bioAvatar(savedAvatar)}
                                alt="Avatar preview"
                                className="h-12 w-12 rounded-full object-cover border border-line2"
                                onError={() => setAvatarFailed(true)}
                            />
                        ) : (
                            <div className="flex h-12 w-12 items-center justify-center rounded-full border border-line bg-primary-100 text-sm font-bold text-primary-700">
                                {initial}
                            </div>
                        )}
                        <input
                            id="bio-avatar"
                            type="url"
                            value={avatarUrl}
                            onChange={(e) => setAvatarUrl(e.target.value)}
                            placeholder="https://example.com/avatar.png"
                            maxLength={MAX_AVATAR_URL_CHARS}
                            className={inputClass}
                        />
                    </div>
                    <p className="text-xs text-faint mt-1">An http or https image. Leave blank to remove it. The preview uses your public page.</p>
                </div>
                <div className="flex items-center gap-3">
                    <button
                        type="submit"
                        disabled={savingBio}
                        className="rounded-lg bg-primary-600 px-4 py-2 font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50"
                    >
                        {savingBio ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Save'}
                    </button>
                    {bioEnabled && bioUsername && (
                        <a href={`/@${bioUsername}`} target="_blank" rel="noreferrer" className="text-sm font-medium text-primary-600 hover:text-primary-700">
                            View page →
                        </a>
                    )}
                </div>
            </form>
            <div className="px-6 pb-6 space-y-3">
                <div>
                    <h3 className="font-display text-sm font-bold text-ink">Links on your page</h3>
                    {!wantsLinks && (
                        <button
                            type="button"
                            onClick={() => setLinksAsked(true)}
                            className="mt-2 text-sm font-medium text-primary-600 hover:text-primary-700"
                        >
                            Show links on your page
                        </button>
                    )}
                    {linksError && (
                        <p role="alert" className="text-sm text-danger mt-1">{linksError}</p>
                    )}
                    {!linksError && !linksLoading && linksReady && (
                        <p className="text-xs text-faint mt-1">
                            {bioLinks.length === 0
                                ? 'No links on your bio page yet. Turn on “Show this link on my bio page” in a link’s edit dialog.'
                                : `${bioLinks.length} ${pluralize(bioLinks.length, 'link')} on your page. Leave a label blank to use the link’s title.`}
                            {orgLinkCount > 0 && ` ${orgLinkCount} ${pluralize(orgLinkCount, 'organization link')} ${pluralize(orgLinkCount, 'stays', 'stay')} at the end of the page and ${pluralize(orgLinkCount, 'is', 'are')} not part of this order.`}
                            {!bioOrderFits(bioLinks.length) && ` Reordering is available for up to ${MAX_BIO_ORDER} links. Hide one to rearrange them.`}
                        </p>
                    )}
                </div>
                {linksLoading ? (
                    <p className="text-sm text-faint">Loading your bio links…</p>
                ) : bioLinks.length > 0 && (
                    <ul className="space-y-2" aria-busy={savingOrder}>
                        {bioLinks.map((link, index) => {
                            const name = linkName(link);
                            return (
                                <li key={link.id} className="rounded-lg border border-line p-3 space-y-2">
                                    <div className="flex items-center justify-between gap-3">
                                        <div className="min-w-0">
                                            <p className="font-medium text-ink truncate">{name}</p>
                                            <p className="font-mono text-xs text-faint truncate">{link.code}</p>
                                        </div>
                                        <span className="inline-flex items-center gap-1">
                                            <button
                                                type="button"
                                                aria-label={`Move ${name} up`}
                                                disabled={index === 0 || savingOrder || reorderLocked}
                                                onClick={() => moveLink(index, -1)}
                                                className="rounded p-1 text-faint transition-colors hover:text-ink disabled:opacity-40 disabled:hover:text-faint"
                                            >
                                                <ArrowUp className="h-3.5 w-3.5" />
                                            </button>
                                            <button
                                                type="button"
                                                aria-label={`Move ${name} down`}
                                                disabled={index === bioLinks.length - 1 || savingOrder || reorderLocked}
                                                onClick={() => moveLink(index, 1)}
                                                className="rounded p-1 text-faint transition-colors hover:text-ink disabled:opacity-40 disabled:hover:text-faint"
                                            >
                                                <ArrowDown className="h-3.5 w-3.5" />
                                            </button>
                                        </span>
                                    </div>
                                    <div>
                                        <label htmlFor={`bio-label-${link.id}`} className="block text-xs text-muted mb-1">
                                            Label <span className="sr-only">for {name}</span>
                                        </label>
                                        <input
                                            id={`bio-label-${link.id}`}
                                            type="text"
                                            value={labelDrafts[link.id] ?? ''}
                                            maxLength={MAX_BIO_LABEL_CHARS}
                                            placeholder="Shown instead of the title"
                                            onChange={(e) => replaceDrafts({ ...labelDraftsRef.current, [link.id]: e.target.value })}
                                            onBlur={() => saveLabel(link)}
                                            onKeyDown={(e) => {
                                                if (e.key === 'Enter') {
                                                    e.preventDefault();
                                                    e.currentTarget.blur();
                                                }
                                            }}
                                            className={inputClass}
                                        />
                                    </div>
                                </li>
                            );
                        })}
                    </ul>
                )}
            </div>
        </motion.section>
    );
}
