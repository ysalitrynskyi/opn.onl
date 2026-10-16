import { useEffect, useRef, useState, type FormEvent, type ReactNode } from 'react';
import { createPortal } from 'react-dom';
import { Link, useNavigate } from 'react-router-dom';
import { AlertTriangle, Check, Loader2, Plus, Users, X } from 'lucide-react';
import { API_ENDPOINTS, authFetch, failureMessage, responseError, SessionExpiredError } from '../config/api';
import SEO from '../components/SEO';
import { useFocusTrap } from '../hooks/useFocusTrap';
import { pluralize } from '../utils/plural';
import { safeLocalStorage } from '../utils/storage';
import logger from '../utils/logger';

// GET /orgs does not include the caller's role. The members list does, and
// every member may read it, so the role shown here is that row.
const AUDIT_PAGE = 50;

const ASSIGNABLE_ROLES = ['admin', 'editor', 'viewer'] as const;

interface Profile {
    id: number;
    email: string;
}

interface Org {
    id: number;
    name: string;
    slug: string;
    owner_id: number;
    created_at: string;
    member_count: number;
    link_count: number;
}

interface Member {
    id: number;
    user_id: number;
    email: string;
    role: string;
    joined_at: string;
}

interface OrgRow extends Org {
    myRole: string | null;
    members: Member[] | null;
    membersError: string | null;
}

interface AuditEntry {
    id: number;
    user_id: number | null;
    user_email: string | null;
    action: string;
    resource_type: string;
    resource_id: number | null;
    details: unknown;
    // The entry also carries the member's truncated network address. It is
    // not shown: teammates do not need each other's network.
    ip_address: string | null;
    created_at: string;
}

type Dialog =
    | { kind: 'leave' }
    | { kind: 'remove'; member: Member }
    | { kind: 'transfer' }
    | { kind: 'delete' };

function manages(role: string | null): boolean {
    return role === 'owner' || role === 'admin';
}

function roleLabel(role: string | null): string {
    switch (role) {
        case 'owner': return 'Owner';
        case 'admin': return 'Admin';
        case 'editor': return 'Editor';
        case 'viewer': return 'Viewer';
        default: return 'Unknown';
    }
}

function suggestSlug(name: string): string {
    return name
        .toLowerCase()
        .replace(/[^a-z0-9]+/g, '-')
        .replace(/^-+|-+$/g, '')
        .slice(0, 64);
}

function formatWhen(iso: string): string {
    const date = new Date(iso);
    if (Number.isNaN(date.getTime())) return iso;
    return date.toLocaleString(undefined, {
        year: 'numeric',
        month: 'short',
        day: 'numeric',
        hour: '2-digit',
        minute: '2-digit',
    });
}

function auditSummary(entry: AuditEntry): string {
    const who = entry.user_email ?? 'Someone';
    switch (entry.action) {
        case 'create': return `${who} created the team`;
        case 'update': return `${who} updated the team`;
        case 'invite': return `${who} invited a member`;
        case 'update_role': return `${who} changed a role`;
        case 'remove': return `${who} removed a member`;
        case 'leave': return `${who} left the team`;
        case 'transfer_ownership': return `${who} transferred ownership`;
        default: return `${who} ${entry.action.replaceAll('_', ' ')}`;
    }
}

function auditDetail(details: unknown): string | null {
    if (!details || typeof details !== 'object' || Array.isArray(details)) return null;
    const record = details as Record<string, unknown>;
    const parts: string[] = [];
    if (typeof record.email === 'string') parts.push(record.email);
    if (typeof record.old_role === 'string' && typeof record.new_role === 'string') {
        parts.push(`${record.old_role} → ${record.new_role}`);
    } else if (typeof record.role === 'string') {
        parts.push(record.role);
    }
    return parts.length > 0 ? parts.join(' · ') : null;
}

const inputClass = 'w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint';
const labelClass = 'block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5';
const primaryBtn = 'inline-flex items-center justify-center gap-1.5 rounded-lg bg-primary-600 px-4 py-2 text-sm font-semibold text-white transition-colors hover:bg-primary-700 disabled:opacity-50';
const quietBtn = 'inline-flex items-center justify-center gap-1.5 rounded-lg border border-line2 px-4 py-2 text-sm font-medium text-muted transition-colors hover:border-ink/30 hover:text-ink disabled:opacity-50';
const dangerBtn = 'inline-flex items-center justify-center gap-1.5 rounded-lg border border-danger/40 bg-danger/10 px-4 py-2 text-sm font-semibold text-danger transition-colors hover:bg-danger/20 disabled:opacity-50';

function ConfirmDialog({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);

    useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (event.key === 'Escape') onClose();
        };
        document.addEventListener('keydown', onKey);
        return () => document.removeEventListener('keydown', onKey);
    }, [onClose]);

    return createPortal(
        <div className="fixed inset-0 z-[60] flex items-end justify-center bg-ink/40 p-4 sm:items-center" onClick={onClose}>
            <div
                ref={dialogRef}
                role="dialog"
                aria-modal="true"
                aria-labelledby="org-dialog-title"
                className="w-full max-w-md rounded-2xl border border-line2 bg-surface p-6 shadow-lift"
                onClick={(event) => event.stopPropagation()}
            >
                <h2 id="org-dialog-title" className="font-display text-lg font-bold text-ink tracking-tight">{title}</h2>
                <div className="mt-4">{children}</div>
            </div>
        </div>,
        document.body,
    );
}

function ActivityLog({ orgId }: { orgId: number }) {
    const [offset, setOffset] = useState(0);
    const [entries, setEntries] = useState<AuditEntry[] | null>(null);
    const [error, setError] = useState('');
    const [loading, setLoading] = useState(true);
    const [hasMore, setHasMore] = useState(false);

    useEffect(() => {
        let cancelled = false;
        (async () => {
            try {
                const res = await authFetch(API_ENDPOINTS.orgAudit(orgId, { limit: AUDIT_PAGE, offset }));
                if (cancelled) return;
                if (!res.ok) {
                    setError(await responseError(res, 'Could not load activity'));
                    setEntries([]);
                    setHasMore(false);
                    return;
                }
                const rows = await res.json() as AuditEntry[];
                if (cancelled) return;
                setEntries(rows);
                setHasMore(rows.length === AUDIT_PAGE);
                setError('');
            } catch (err) {
                if (!cancelled) {
                    logger.error('Failed to load organization audit log', err);
                    setError(failureMessage(err, 'Could not load activity'));
                    setEntries([]);
                    setHasMore(false);
                }
            } finally {
                if (!cancelled) setLoading(false);
            }
        })();
        return () => { cancelled = true; };
    }, [orgId, offset]);

    return (
        <section className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden">
            <div className="border-b border-line p-6">
                <h2 className="font-display text-lg font-bold text-ink tracking-tight">Activity</h2>
                <p className="text-sm text-muted">Changes to this team, newest first</p>
            </div>
            <div className="p-6">
                {error && (
                    <p role="alert" className="mb-4 text-sm text-danger">{error}</p>
                )}
                {loading && entries === null ? (
                    <Loader2 className="h-5 w-5 animate-spin text-primary-600" aria-label="Loading activity" />
                ) : entries && entries.length > 0 ? (
                    <ul aria-label="Activity" className="divide-y divide-line rounded-lg border border-line">
                        {entries.map((entry) => {
                            const detail = auditDetail(entry.details);
                            return (
                                <li key={entry.id} className="px-4 py-3">
                                    <p className="text-sm text-ink">{auditSummary(entry)}</p>
                                    <p className="mt-0.5 font-mono text-xs text-faint break-all">
                                        {formatWhen(entry.created_at)}
                                        {detail ? ` · ${detail}` : ''}
                                    </p>
                                </li>
                            );
                        })}
                    </ul>
                ) : (
                    <p className="text-sm text-muted">
                        {offset > 0 ? 'No older activity.' : 'No activity yet.'}
                    </p>
                )}
                {(offset > 0 || hasMore) && (
                    <div className="mt-4 flex flex-wrap gap-2">
                        {offset > 0 && (
                            <button
                                type="button"
                                className={quietBtn}
                                disabled={loading}
                                onClick={() => {
                                    setLoading(true);
                                    setOffset((current) => Math.max(0, current - AUDIT_PAGE));
                                }}
                            >
                                Newer
                            </button>
                        )}
                        {hasMore && (
                            <button
                                type="button"
                                className={quietBtn}
                                disabled={loading}
                                onClick={() => {
                                    setLoading(true);
                                    setOffset((current) => current + AUDIT_PAGE);
                                }}
                            >
                                Older
                            </button>
                        )}
                    </div>
                )}
            </div>
        </section>
    );
}

async function membershipOf(org: Org, myId: number): Promise<OrgRow> {
    try {
        const res = await authFetch(API_ENDPOINTS.orgMembers(org.id));
        if (!res.ok) {
            return {
                ...org,
                myRole: null,
                members: null,
                membersError: await responseError(res, 'Could not load members'),
            };
        }
        const members = await res.json() as Member[];
        const mine = members.find((member) => member.user_id === myId);
        return { ...org, myRole: mine?.role ?? null, members, membersError: null };
    } catch (err) {
        if (err instanceof SessionExpiredError) throw err;
        return {
            ...org,
            myRole: null,
            members: null,
            membersError: failureMessage(err, 'Could not load members'),
        };
    }
}

export default function Organizations() {
    const navigate = useNavigate();
    const [loading, setLoading] = useState(true);
    const [me, setMe] = useState<Profile | null>(null);
    const [orgs, setOrgs] = useState<OrgRow[]>([]);
    const [selectedId, setSelectedId] = useState<number | null>(null);
    const [loadError, setLoadError] = useState('');
    const [notice, setNotice] = useState<{ tone: 'error' | 'ok'; text: string } | null>(null);
    const [teamName, setTeamName] = useState('');
    const [teamSlug, setTeamSlug] = useState('');
    const [slugEdited, setSlugEdited] = useState(false);
    const [creating, setCreating] = useState(false);
    const [renameName, setRenameName] = useState('');
    const [renameSlug, setRenameSlug] = useState('');
    const [savingRename, setSavingRename] = useState(false);
    const [inviteEmail, setInviteEmail] = useState('');
    const [inviteRole, setInviteRole] = useState<(typeof ASSIGNABLE_ROLES)[number]>('viewer');
    const [inviting, setInviting] = useState(false);
    const [pendingMemberId, setPendingMemberId] = useState<number | null>(null);
    const [dialog, setDialog] = useState<Dialog | null>(null);
    const [confirmName, setConfirmName] = useState('');
    const [transferUserId, setTransferUserId] = useState('');
    const [dialogBusy, setDialogBusy] = useState(false);
    const [auditGeneration, setAuditGeneration] = useState(0);

    const orgsRef = useRef<OrgRow[]>([]);
    const loadSeq = useRef(0);
    const pendingSelect = useRef<number | null | undefined>(undefined);

    const selected = orgs.find((org) => org.id === selectedId) ?? null;

    async function loadTeams(): Promise<boolean> {
        const seq = ++loadSeq.current;
        if (orgsRef.current.length === 0) setLoading(true);
        try {
            const meRes = await authFetch(API_ENDPOINTS.userProfile);
            if (seq !== loadSeq.current) return false;
            if (!meRes.ok) {
                setLoadError(await responseError(meRes, 'Could not load your account'));
                return false;
            }
            const profile = await meRes.json() as Profile;
            const orgsRes = await authFetch(API_ENDPOINTS.orgs);
            if (seq !== loadSeq.current) return false;
            if (!orgsRes.ok) {
                setMe(profile);
                setLoadError(await responseError(orgsRes, 'Could not load teams'));
                return false;
            }
            const list = await orgsRes.json() as Org[];
            const rows = await Promise.all(list.map((org) => membershipOf(org, profile.id)));
            if (seq !== loadSeq.current) return false;
            orgsRef.current = rows;
            setMe(profile);
            setOrgs(rows);
            setLoadError('');
            const wanted = pendingSelect.current;
            pendingSelect.current = undefined;
            setSelectedId((current) => {
                if (wanted !== undefined) return wanted;
                return current != null && rows.some((org) => org.id === current) ? current : null;
            });
            return true;
        } catch (err) {
            if (seq !== loadSeq.current) return false;
            logger.error('Failed to load teams', err);
            setLoadError(failureMessage(err, 'Could not load teams'));
            return false;
        } finally {
            if (seq === loadSeq.current) setLoading(false);
        }
    }

    useEffect(() => {
        const token = safeLocalStorage.getItem('token');
        if (!token) {
            navigate('/login');
            return;
        }
        void loadTeams();
        // Initial load only. Later refreshes call loadTeams directly.
    }, [navigate]);

    useEffect(() => {
        if (!selected) return;
        setRenameName(selected.name);
        setRenameSlug(selected.slug);
        // Keep the form on the saved name and slug. Membership reloads must not
        // wipe an edit, so this follows those two fields only.
        // eslint-disable-next-line react-hooks/exhaustive-deps
    }, [selected?.id, selected?.name, selected?.slug]);

    function selectOrg(id: number) {
        setSelectedId(id);
        setDialog(null);
        setNotice(null);
    }

    function bumpAudit() {
        setAuditGeneration((current) => current + 1);
    }

    async function handleCreate(event: FormEvent) {
        event.preventDefault();
        setNotice(null);
        setCreating(true);
        try {
            const res = await authFetch(API_ENDPOINTS.orgs, {
                method: 'POST',
                body: JSON.stringify({ name: teamName.trim(), slug: teamSlug.trim() }),
            });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not create the team') });
                return;
            }
            const created = await res.json() as Org;
            pendingSelect.current = created.id;
            const ok = await loadTeams();
            if (ok) {
                setTeamName('');
                setTeamSlug('');
                setSlugEdited(false);
                setNotice({ tone: 'ok', text: 'Team created' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not create the team') });
        } finally {
            setCreating(false);
        }
    }

    async function handleRename(event: FormEvent) {
        event.preventDefault();
        if (!selected) return;
        setNotice(null);
        setSavingRename(true);
        try {
            const res = await authFetch(API_ENDPOINTS.org(selected.id), {
                method: 'PUT',
                body: JSON.stringify({ name: renameName.trim(), slug: renameSlug.trim() }),
            });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not update the team') });
                return;
            }
            const ok = await loadTeams();
            if (ok) {
                setNotice({ tone: 'ok', text: 'Team updated' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not update the team') });
        } finally {
            setSavingRename(false);
        }
    }

    async function handleInvite(event: FormEvent) {
        event.preventDefault();
        if (!selected) return;
        setNotice(null);
        setInviting(true);
        try {
            const res = await authFetch(API_ENDPOINTS.orgMembers(selected.id), {
                method: 'POST',
                body: JSON.stringify({ email: inviteEmail.trim(), role: inviteRole }),
            });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not add member') });
                return;
            }
            setInviteEmail('');
            const ok = await loadTeams();
            if (ok) {
                setNotice({ tone: 'ok', text: 'Member added' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not add member') });
        } finally {
            setInviting(false);
        }
    }

    async function handleRoleChange(member: Member, role: string) {
        if (!selected || role === member.role) return;
        setNotice(null);
        setPendingMemberId(member.id);
        try {
            const res = await authFetch(API_ENDPOINTS.orgMember(selected.id, member.id), {
                method: 'PUT',
                body: JSON.stringify({ role }),
            });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not change the role') });
                return;
            }
            const ok = await loadTeams();
            if (ok) {
                setNotice({ tone: 'ok', text: 'Role updated' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not change the role') });
        } finally {
            setPendingMemberId(null);
        }
    }

    async function handleRemove(member: Member) {
        if (!selected) return;
        setDialogBusy(true);
        setNotice(null);
        try {
            const res = await authFetch(API_ENDPOINTS.orgMember(selected.id, member.id), { method: 'DELETE' });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not remove member') });
                return;
            }
            setDialog(null);
            const ok = await loadTeams();
            if (ok) {
                setNotice({ tone: 'ok', text: 'Member removed' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not remove member') });
        } finally {
            setDialogBusy(false);
        }
    }

    async function handleLeave() {
        if (!selected) return;
        setDialogBusy(true);
        setNotice(null);
        try {
            const res = await authFetch(API_ENDPOINTS.orgLeave(selected.id), { method: 'POST' });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not leave the team') });
                return;
            }
            setDialog(null);
            pendingSelect.current = null;
            const ok = await loadTeams();
            if (ok) setNotice({ tone: 'ok', text: 'You left the team' });
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not leave the team') });
        } finally {
            setDialogBusy(false);
        }
    }

    async function handleTransfer() {
        if (!selected) return;
        const newOwnerUserId = Number(transferUserId);
        if (!Number.isInteger(newOwnerUserId)) return;
        setDialogBusy(true);
        setNotice(null);
        try {
            const res = await authFetch(API_ENDPOINTS.orgTransfer(selected.id), {
                method: 'POST',
                body: JSON.stringify({ new_owner_user_id: newOwnerUserId }),
            });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not transfer ownership') });
                return;
            }
            setDialog(null);
            setTransferUserId('');
            const ok = await loadTeams();
            if (ok) {
                setNotice({ tone: 'ok', text: 'Ownership transferred' });
                bumpAudit();
            }
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not transfer ownership') });
        } finally {
            setDialogBusy(false);
        }
    }

    async function handleDelete() {
        if (!selected || confirmName !== selected.name) return;
        setDialogBusy(true);
        setNotice(null);
        try {
            const res = await authFetch(API_ENDPOINTS.org(selected.id), { method: 'DELETE' });
            if (!res.ok) {
                setNotice({ tone: 'error', text: await responseError(res, 'Could not delete the team') });
                return;
            }
            setDialog(null);
            setConfirmName('');
            pendingSelect.current = null;
            const ok = await loadTeams();
            if (ok) setNotice({ tone: 'ok', text: 'Team deleted' });
        } catch (err) {
            setNotice({ tone: 'error', text: failureMessage(err, 'Could not delete the team') });
        } finally {
            setDialogBusy(false);
        }
    }

    function closeDialog() {
        if (dialogBusy) return;
        setDialog(null);
        setConfirmName('');
        setTransferUserId('');
    }

    const canManage = manages(selected?.myRole ?? null);
    const isOwner = selected?.myRole === 'owner';
    const canLeave = selected?.myRole != null && selected.myRole !== 'owner';
    const otherMembers = selected?.members?.filter((member) => member.user_id !== me?.id) ?? [];

    return (
        <div className="mx-auto max-w-5xl px-4 py-10 sm:px-6 lg:px-8">
            <SEO title="Teams" description="Create a team and manage its members" noIndex />
            <h1 className="font-display text-3xl font-extrabold tracking-tight text-ink sm:text-4xl">Teams</h1>
            <p className="mt-2 mb-8 max-w-2xl text-sm text-muted">
                Share links with other people. Your role decides what you can change.
            </p>

            {loadError && (
                <div role="alert" className="mb-6 flex flex-col gap-3 rounded-xl border border-danger/30 bg-danger/5 px-4 py-3 text-sm text-danger sm:flex-row sm:items-center">
                    <div className="flex min-w-0 items-center gap-2">
                        <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                        <span className="break-words">{loadError}</span>
                    </div>
                    <button
                        type="button"
                        onClick={() => { void loadTeams(); }}
                        className="shrink-0 rounded-lg border border-danger/30 bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink/30 sm:ml-auto"
                    >
                        Try again
                    </button>
                </div>
            )}

            {notice && (
                <div
                    role={notice.tone === 'error' ? 'alert' : 'status'}
                    className={`mb-6 flex items-center gap-2 rounded-xl border px-4 py-3 text-sm ${
                        notice.tone === 'error'
                            ? 'border-danger/30 bg-danger/5 text-danger'
                            : 'border-success/30 bg-success/5 text-success'
                    }`}
                >
                    {notice.tone === 'error'
                        ? <AlertTriangle className="h-4 w-4 flex-shrink-0" />
                        : <Check className="h-4 w-4 flex-shrink-0" />}
                    <span className="min-w-0 break-words">{notice.text}</span>
                    <button type="button" onClick={() => setNotice(null)} aria-label="Dismiss" className="ml-auto">
                        <X className="h-4 w-4" />
                    </button>
                </div>
            )}

            {loading && orgs.length === 0 ? (
                <div className="flex min-h-[40vh] items-center justify-center">
                    <Loader2 className="h-8 w-8 animate-spin text-primary-600" aria-label="Loading teams" />
                </div>
            ) : me && (
                <div className="space-y-6">
                    <section className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden">
                        <div className="border-b border-line p-6">
                            <h2 className="font-display text-lg font-bold tracking-tight text-ink">New team</h2>
                            <p className="text-sm text-muted">You become the owner</p>
                        </div>
                        <form onSubmit={handleCreate} className="grid grid-cols-1 gap-4 p-6 sm:grid-cols-2">
                            <div className="sm:col-span-1">
                                <label htmlFor="new-team-name" className={labelClass}>Team name</label>
                                <input
                                    id="new-team-name"
                                    value={teamName}
                                    onChange={(event) => {
                                        const next = event.target.value;
                                        setTeamName(next);
                                        if (!slugEdited) setTeamSlug(suggestSlug(next));
                                    }}
                                    maxLength={100}
                                    required
                                    className={inputClass}
                                    placeholder="Acme"
                                />
                            </div>
                            <div>
                                <label htmlFor="new-team-slug" className={labelClass}>Team slug</label>
                                <input
                                    id="new-team-slug"
                                    value={teamSlug}
                                    onChange={(event) => {
                                        setSlugEdited(true);
                                        setTeamSlug(event.target.value.toLowerCase());
                                    }}
                                    maxLength={64}
                                    required
                                    spellCheck={false}
                                    autoCapitalize="none"
                                    autoCorrect="off"
                                    className={inputClass}
                                    placeholder="acme"
                                />
                            </div>
                            <div className="sm:col-span-2">
                                <button type="submit" className={primaryBtn} disabled={creating}>
                                    {creating ? <Loader2 className="h-4 w-4 animate-spin" /> : <Plus className="h-4 w-4" />}
                                    Create team
                                </button>
                            </div>
                        </form>
                    </section>

                    <div className="flex flex-wrap items-baseline justify-between gap-2">
                        <h2 className="font-mono text-xs uppercase tracking-[0.14em] text-faint">Your teams</h2>
                        <p className="text-sm text-muted">{orgs.length} {pluralize(orgs.length, 'team')}</p>
                    </div>

                    {orgs.length === 0 ? (
                        <p className="text-sm text-muted">You are not on a team yet.</p>
                    ) : (
                        <div className="grid grid-cols-1 gap-6 lg:grid-cols-[minmax(0,18rem)_minmax(0,1fr)]">
                            <ul className="min-w-0 space-y-2">
                                {orgs.map((org) => {
                                    const active = org.id === selectedId;
                                    return (
                                        <li key={org.id}>
                                            <button
                                                type="button"
                                                onClick={() => selectOrg(org.id)}
                                                aria-current={active ? 'true' : undefined}
                                                className={`w-full min-w-0 rounded-xl border px-4 py-3 text-left transition-colors ${
                                                    active
                                                        ? 'border-primary-300 bg-primary-50/60'
                                                        : 'border-line2 bg-surface hover:border-ink/30'
                                                }`}
                                            >
                                                <span className="block truncate font-medium text-ink">{org.name}</span>
                                                <span className="mt-1 flex flex-wrap items-center gap-x-2 gap-y-1 font-mono text-[10px] uppercase tracking-[0.14em] text-faint">
                                                    <span>{roleLabel(org.myRole)}</span>
                                                    <span>{org.member_count} {pluralize(org.member_count, 'member')}</span>
                                                    <span>{org.link_count} {pluralize(org.link_count, 'link')}</span>
                                                </span>
                                            </button>
                                        </li>
                                    );
                                })}
                            </ul>

                            <div className="min-w-0">
                                {!selected ? (
                                    <p className="text-sm text-muted">Select a team to see its members.</p>
                                ) : (
                                    <div className="space-y-6">
                                        <section className="rounded-2xl border border-line2 bg-surface p-6 shadow-subtle">
                                            <div className="flex flex-wrap items-start justify-between gap-3">
                                                <div className="min-w-0">
                                                    <div className="flex items-center gap-2">
                                                        <Users className="h-5 w-5 flex-shrink-0 text-muted" />
                                                        <h2 className="truncate font-display text-lg font-bold tracking-tight text-ink">{selected.name}</h2>
                                                    </div>
                                                    <p className="mt-1 break-all font-mono text-xs text-faint">{selected.slug}</p>
                                                    <p className="mt-2 font-mono text-[10px] uppercase tracking-[0.14em] text-faint">
                                                        {roleLabel(selected.myRole)}
                                                        {' · '}
                                                        {selected.member_count} {pluralize(selected.member_count, 'member')}
                                                        {' · '}
                                                        {selected.link_count} {pluralize(selected.link_count, 'link')}
                                                    </p>
                                                </div>
                                                <div className="flex flex-wrap gap-2">
                                                    <Link to={`/dashboard?org=${selected.id}`} className={quietBtn}>View links</Link>
                                                    {canLeave && (
                                                        <button type="button" className={quietBtn} onClick={() => setDialog({ kind: 'leave' })}>
                                                            Leave team
                                                        </button>
                                                    )}
                                                </div>
                                            </div>
                                        </section>

                                        {canManage && (
                                            <section className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden">
                                                <div className="border-b border-line p-6">
                                                    <h2 className="font-display text-lg font-bold tracking-tight text-ink">Details</h2>
                                                </div>
                                                <form onSubmit={handleRename} className="grid grid-cols-1 gap-4 p-6 sm:grid-cols-2">
                                                    <div>
                                                        <label htmlFor="rename-team-name" className={labelClass}>Name</label>
                                                        <input
                                                            id="rename-team-name"
                                                            value={renameName}
                                                            onChange={(event) => setRenameName(event.target.value)}
                                                            maxLength={100}
                                                            required
                                                            className={inputClass}
                                                        />
                                                    </div>
                                                    <div>
                                                        <label htmlFor="rename-team-slug" className={labelClass}>Slug</label>
                                                        <input
                                                            id="rename-team-slug"
                                                            value={renameSlug}
                                                            onChange={(event) => setRenameSlug(event.target.value.toLowerCase())}
                                                            maxLength={64}
                                                            required
                                                            spellCheck={false}
                                                            autoCapitalize="none"
                                                            className={inputClass}
                                                        />
                                                    </div>
                                                    <div className="sm:col-span-2">
                                                        <button type="submit" className={primaryBtn} disabled={savingRename}>
                                                            {savingRename ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Save'}
                                                        </button>
                                                    </div>
                                                </form>
                                            </section>
                                        )}

                                        <section className="rounded-2xl border border-line2 bg-surface shadow-subtle overflow-hidden">
                                            <div className="border-b border-line p-6">
                                                <h2 className="font-display text-lg font-bold tracking-tight text-ink">Members</h2>
                                            </div>
                                            <div className="p-6">
                                                {selected.membersError && (
                                                    <div role="alert" className="mb-4 flex flex-col gap-3 text-sm text-danger sm:flex-row sm:items-center">
                                                        <span className="break-words">{selected.membersError}</span>
                                                        <button type="button" className={quietBtn} onClick={() => { void loadTeams(); }}>
                                                            Try again
                                                        </button>
                                                    </div>
                                                )}
                                                {selected.members && selected.myRole == null && (
                                                    <p className="mb-4 text-sm text-muted">You are not listed as a member of this team.</p>
                                                )}
                                                {selected.members && selected.members.length === 0 && (
                                                    <p className="text-sm text-muted">No members yet.</p>
                                                )}
                                                {selected.members && selected.members.length > 0 && (
                                                    <ul aria-label="Members" className="divide-y divide-line rounded-lg border border-line">
                                                        {selected.members.map((member) => {
                                                            const isMe = member.user_id === me.id;
                                                            const canChange = canManage && member.role !== 'owner';
                                                            const canRemove = canManage && member.role !== 'owner' && !isMe;
                                                            return (
                                                                <li key={member.id} className="flex flex-col gap-3 px-4 py-3 sm:flex-row sm:items-center sm:justify-between">
                                                                    <div className="min-w-0">
                                                                        <p className="break-all text-sm font-medium text-ink">
                                                                            {member.email}{isMe ? ' (you)' : ''}
                                                                        </p>
                                                                        <p className="font-mono text-xs text-faint">Joined {formatWhen(member.joined_at)}</p>
                                                                    </div>
                                                                    <div className="flex flex-wrap items-center gap-2">
                                                                        {canChange ? (
                                                                            <select
                                                                                aria-label={`Role for ${member.email}`}
                                                                                value={ASSIGNABLE_ROLES.includes(member.role as typeof ASSIGNABLE_ROLES[number]) ? member.role : 'viewer'}
                                                                                disabled={pendingMemberId === member.id}
                                                                                onChange={(event) => { void handleRoleChange(member, event.target.value); }}
                                                                                className="max-w-full rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none focus:border-primary-500"
                                                                            >
                                                                                {ASSIGNABLE_ROLES.map((role) => (
                                                                                    <option key={role} value={role}>{roleLabel(role)}</option>
                                                                                ))}
                                                                            </select>
                                                                        ) : (
                                                                            <span className="font-mono text-[10px] uppercase tracking-[0.14em] text-faint">
                                                                                {roleLabel(member.role)}
                                                                            </span>
                                                                        )}
                                                                        {canRemove && (
                                                                            <button
                                                                                type="button"
                                                                                className="text-sm text-danger hover:underline"
                                                                                aria-label={`Remove ${member.email}`}
                                                                                onClick={() => setDialog({ kind: 'remove', member })}
                                                                            >
                                                                                Remove
                                                                            </button>
                                                                        )}
                                                                    </div>
                                                                </li>
                                                            );
                                                        })}
                                                    </ul>
                                                )}

                                                {canManage && (
                                                    <form onSubmit={handleInvite} className="mt-4 grid grid-cols-1 gap-3 sm:grid-cols-[minmax(0,1fr)_9rem_auto] sm:items-end">
                                                        <div className="min-w-0">
                                                            <label htmlFor="member-email" className={labelClass}>Member email</label>
                                                            <input
                                                                id="member-email"
                                                                type="email"
                                                                value={inviteEmail}
                                                                onChange={(event) => setInviteEmail(event.target.value)}
                                                                required
                                                                autoComplete="email"
                                                                className={inputClass}
                                                                placeholder="name@example.com"
                                                            />
                                                        </div>
                                                        <div>
                                                            <label htmlFor="member-role" className={labelClass}>Member role</label>
                                                            <select
                                                                id="member-role"
                                                                value={inviteRole}
                                                                onChange={(event) => setInviteRole(event.target.value as typeof inviteRole)}
                                                                className="w-full rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none focus:border-primary-500"
                                                            >
                                                                {ASSIGNABLE_ROLES.map((role) => (
                                                                    <option key={role} value={role}>{roleLabel(role)}</option>
                                                                ))}
                                                            </select>
                                                        </div>
                                                        <button type="submit" className={primaryBtn} disabled={inviting}>
                                                            {inviting ? <Loader2 className="h-4 w-4 animate-spin" /> : 'Add member'}
                                                        </button>
                                                    </form>
                                                )}
                                            </div>
                                        </section>

                                        {canManage && (
                                            <ActivityLog key={`${selected.id}:${auditGeneration}`} orgId={selected.id} />
                                        )}

                                        {isOwner && (
                                            <section className="rounded-2xl border border-danger/30 bg-surface shadow-subtle overflow-hidden">
                                                <div className="border-b border-danger/20 bg-danger/5 p-6">
                                                    <h2 className="font-display text-lg font-bold tracking-tight text-danger">Danger zone</h2>
                                                    <p className="text-sm text-danger">Irreversible actions</p>
                                                </div>
                                                <div className="flex flex-col gap-3 p-6 sm:flex-row sm:flex-wrap">
                                                    <button
                                                        type="button"
                                                        className={quietBtn}
                                                        onClick={() => {
                                                            setTransferUserId(otherMembers[0] ? String(otherMembers[0].user_id) : '');
                                                            setDialog({ kind: 'transfer' });
                                                        }}
                                                    >
                                                        Transfer ownership
                                                    </button>
                                                    <button
                                                        type="button"
                                                        className={dangerBtn}
                                                        onClick={() => {
                                                            setConfirmName('');
                                                            setDialog({ kind: 'delete' });
                                                        }}
                                                    >
                                                        Delete team
                                                    </button>
                                                </div>
                                            </section>
                                        )}
                                    </div>
                                )}
                            </div>
                        </div>
                    )}
                </div>
            )}

            {dialog && selected && (
                <ConfirmDialog title={
                    dialog.kind === 'leave' ? 'Leave team'
                        : dialog.kind === 'remove' ? 'Remove member'
                            : dialog.kind === 'transfer' ? 'Transfer ownership'
                                : 'Delete team'
                } onClose={closeDialog}>
                    {dialog.kind === 'leave' && (
                        <form onSubmit={(event) => { event.preventDefault(); void handleLeave(); }} className="space-y-4">
                            <p className="text-sm text-muted">
                                You will lose access to this team's links. Links you created stay with the team.
                            </p>
                            <div className="flex flex-wrap gap-2">
                                <button type="button" className={quietBtn} onClick={closeDialog}>Cancel</button>
                                <button type="submit" className={dangerBtn} disabled={dialogBusy}>Leave team</button>
                            </div>
                        </form>
                    )}
                    {dialog.kind === 'remove' && (
                        <form onSubmit={(event) => { event.preventDefault(); void handleRemove(dialog.member); }} className="space-y-4">
                            <p className="break-all text-sm text-muted">
                                Remove {dialog.member.email} from {selected.name}? They lose access. Links they created stay with the team.
                            </p>
                            <div className="flex flex-wrap gap-2">
                                <button type="button" className={quietBtn} onClick={closeDialog}>Cancel</button>
                                <button type="submit" className={dangerBtn} disabled={dialogBusy}>Remove member</button>
                            </div>
                        </form>
                    )}
                    {dialog.kind === 'transfer' && (
                        <form onSubmit={(event) => { event.preventDefault(); void handleTransfer(); }} className="space-y-4">
                            {otherMembers.length === 0 ? (
                                <p className="text-sm text-muted">Add a member before transferring ownership.</p>
                            ) : (
                                <>
                                    <p className="text-sm text-muted">They become the owner. You stay on the team as an admin.</p>
                                    <div>
                                        <label htmlFor="new-owner" className={labelClass}>New owner</label>
                                        <select
                                            id="new-owner"
                                            value={transferUserId}
                                            onChange={(event) => setTransferUserId(event.target.value)}
                                            className="w-full rounded-lg border border-line2 bg-surface px-3 py-2 text-sm text-ink outline-none focus:border-primary-500"
                                        >
                                            {otherMembers.map((member) => (
                                                <option key={member.user_id} value={member.user_id}>{member.email}</option>
                                            ))}
                                        </select>
                                    </div>
                                </>
                            )}
                            <div className="flex flex-wrap gap-2">
                                <button type="button" className={quietBtn} onClick={closeDialog}>Cancel</button>
                                {otherMembers.length > 0 && (
                                    <button type="submit" className={dangerBtn} disabled={dialogBusy || transferUserId === ''}>
                                        Transfer ownership
                                    </button>
                                )}
                            </div>
                        </form>
                    )}
                    {dialog.kind === 'delete' && (
                        <form onSubmit={(event) => { event.preventDefault(); void handleDelete(); }} className="space-y-4">
                            <p className="text-sm text-danger">
                                This permanently deletes the team and every link in it. Type {selected.name} to confirm.
                            </p>
                            <div>
                                <label htmlFor="confirm-team-name" className={labelClass}>Type the team name</label>
                                <input
                                    id="confirm-team-name"
                                    value={confirmName}
                                    onChange={(event) => setConfirmName(event.target.value)}
                                    className={inputClass}
                                    autoComplete="off"
                                />
                            </div>
                            <div className="flex flex-wrap gap-2">
                                <button type="button" className={quietBtn} onClick={closeDialog}>Cancel</button>
                                <button type="submit" className={dangerBtn} disabled={dialogBusy || confirmName !== selected.name}>
                                    Delete team
                                </button>
                            </div>
                        </form>
                    )}
                </ConfirmDialog>
            )}
        </div>
    );
}
