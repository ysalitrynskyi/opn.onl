import { useEffect, useRef, useState, type FormEvent } from 'react';
import { SlidersHorizontal } from 'lucide-react';
import { API_ENDPOINTS, authFetch, responseError } from '../../config/api';
import { pluralize } from '../../utils/plural';
import { swatchColor, type CatalogItem } from './linkScope';

interface OrganizeMenuProps {
    orgId: number | null;
    folders: CatalogItem[];
    tags: CatalogItem[];
    onChanged: (kind: 'folder' | 'tag') => void;
    onFolderDeleted: (id: number) => void;
    onTagDeleted: (id: number) => void;
}

const MAX_NAME = 100;
const DEFAULT_COLOR = '#2563eb';

const fieldClass = 'min-w-0 flex-1 rounded-lg border border-line2 bg-surface px-3 py-1.5 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint';
const buttonClass = 'inline-flex items-center gap-1.5 rounded-lg border border-line2 bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink/30 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 disabled:cursor-wait disabled:opacity-50';

type Kind = 'folder' | 'tag';

/**
 * Create, rename and delete the folders and tags of the current context.
 * Deleting a folder unfiles its links (the foreign key sets folder_id null).
 * Deleting a tag takes it off its links; the links stay.
 */
export default function OrganizeMenu({
    orgId,
    folders,
    tags,
    onChanged,
    onFolderDeleted,
    onTagDeleted,
}: OrganizeMenuProps) {
    const [open, setOpen] = useState(false);
    const rootRef = useRef<HTMLDivElement>(null);

    useEffect(() => {
        if (!open) return;
        const onPointer = (event: MouseEvent) => {
            if (!rootRef.current?.contains(event.target as Node)) setOpen(false);
        };
        // The page shortcut listens on window. Stopping the event here, on
        // document, closes the panel and leaves the search box alone. A real
        // modal is further in front, so Escape still dismisses that first.
        const onKey = (event: KeyboardEvent) => {
            if (event.key !== 'Escape') return;
            if (document.querySelector('[role="dialog"][aria-modal="true"]')) return;
            event.stopPropagation();
            setOpen(false);
        };
        document.addEventListener('mousedown', onPointer);
        document.addEventListener('keydown', onKey);
        return () => {
            document.removeEventListener('mousedown', onPointer);
            document.removeEventListener('keydown', onKey);
        };
    }, [open]);

    return (
        <div className="relative" ref={rootRef}>
            <button
                type="button"
                aria-expanded={open}
                aria-controls="organize-panel"
                onClick={() => setOpen(value => !value)}
                className={buttonClass}
            >
                <SlidersHorizontal className="h-4 w-4 text-muted" />
                Manage
            </button>
            {open && (
                <div
                    id="organize-panel"
                    role="region"
                    aria-label="Manage folders and tags"
                    className="absolute right-0 z-30 mt-2 w-[min(100vw-2rem,22rem)] rounded-xl border border-line2 bg-surface p-4 shadow-lift"
                >
                    <ResourceSection
                        kind="folder"
                        orgId={orgId}
                        items={folders}
                        onChanged={() => onChanged('folder')}
                        onDeleted={onFolderDeleted}
                    />
                    <ResourceSection
                        kind="tag"
                        orgId={orgId}
                        items={tags}
                        onChanged={() => onChanged('tag')}
                        onDeleted={onTagDeleted}
                    />
                </div>
            )}
        </div>
    );
}

function ResourceSection({
    kind,
    orgId,
    items,
    onChanged,
    onDeleted,
}: {
    kind: Kind;
    orgId: number | null;
    items: CatalogItem[];
    onChanged: () => void;
    onDeleted: (id: number) => void;
}) {
    const noun = kind === 'folder' ? 'folder' : 'tag';
    const Noun = kind === 'folder' ? 'Folder' : 'Tag';
    const [name, setName] = useState('');
    const [color, setColor] = useState(DEFAULT_COLOR);
    const [editingId, setEditingId] = useState<number | null>(null);
    const [editName, setEditName] = useState('');
    const [editColor, setEditColor] = useState(DEFAULT_COLOR);
    const [pendingDelete, setPendingDelete] = useState<CatalogItem | null>(null);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState('');

    const collectionUrl = kind === 'folder' ? API_ENDPOINTS.folders : API_ENDPOINTS.tags;
    const itemUrl = kind === 'folder' ? API_ENDPOINTS.folder : API_ENDPOINTS.tag;

    const create = async (event?: FormEvent) => {
        event?.preventDefault();
        const trimmed = name.trim();
        if (!trimmed || busy) return;
        setBusy(true);
        setError('');
        const body: Record<string, unknown> = { name: trimmed };
        const hex = swatchColor(color);
        if (hex) body.color = hex;
        if (orgId != null) body.org_id = orgId;
        try {
            const res = await authFetch(collectionUrl, { method: 'POST', body: JSON.stringify(body) });
            if (!res.ok) {
                setError(await responseError(res, `Failed to add ${noun}`));
                return;
            }
            setName('');
            onChanged();
        } catch {
            setError(`Failed to add ${noun}`);
        } finally {
            setBusy(false);
        }
    };

    const save = async (id: number) => {
        const trimmed = editName.trim();
        if (!trimmed || busy) return;
        setBusy(true);
        setError('');
        const body: Record<string, unknown> = { name: trimmed };
        const hex = swatchColor(editColor);
        if (hex) body.color = hex;
        try {
            const res = await authFetch(itemUrl(id), { method: 'PUT', body: JSON.stringify(body) });
            if (!res.ok) {
                setError(await responseError(res, `Failed to rename ${noun}`));
                return;
            }
            setEditingId(null);
            onChanged();
        } catch {
            setError(`Failed to rename ${noun}`);
        } finally {
            setBusy(false);
        }
    };

    const remove = async (item: CatalogItem) => {
        if (busy) return;
        setBusy(true);
        setError('');
        try {
            const res = await authFetch(itemUrl(item.id), { method: 'DELETE' });
            if (!res.ok) {
                setError(await responseError(res, `Failed to delete ${noun}`));
                return;
            }
            setPendingDelete(null);
            onDeleted(item.id);
        } catch {
            setError(`Failed to delete ${noun}`);
        } finally {
            setBusy(false);
        }
    };

    const confirmText = (item: CatalogItem) => {
        const n = item.link_count;
        const count = `${n.toLocaleString()} ${pluralize(n, 'link')}`;
        if (kind === 'folder') {
            return n === 1
                ? `Delete ${item.name}? ${count} stays, but it leaves this folder.`
                : `Delete ${item.name}? ${count} stay, but they leave this folder.`;
        }
        return n === 1
            ? `Delete ${item.name}? It comes off ${count}. The link stays.`
            : `Delete ${item.name}? It comes off ${count}. The links stay.`;
    };

    return (
        <section className="mb-4 last:mb-0">
            <h3 className="font-mono text-xs uppercase tracking-[0.14em] text-faint mb-2">{Noun}s</h3>
            {items.length === 0 && <p className="mb-2 text-sm text-muted">No {noun}s yet</p>}
            <ul className="mb-2 space-y-1.5">
                {items.map(item => (
                    <li key={item.id} className="rounded-lg border border-line px-2 py-1.5">
                        {pendingDelete?.id === item.id ? (
                            <div>
                                <p className="text-sm text-ink">{confirmText(item)}</p>
                                <div className="mt-2 flex justify-end gap-2">
                                    <button type="button" onClick={() => setPendingDelete(null)} disabled={busy} className={buttonClass}>
                                        Cancel
                                    </button>
                                    <button
                                        type="button"
                                        onClick={() => { void remove(item); }}
                                        disabled={busy}
                                        className="inline-flex items-center rounded-lg bg-danger px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-danger/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-danger disabled:cursor-wait disabled:opacity-50"
                                    >
                                        Delete {noun}
                                    </button>
                                </div>
                            </div>
                        ) : editingId === item.id ? (
                            <form
                                onSubmit={(event) => {
                                    event.preventDefault();
                                    void save(item.id);
                                }}
                                className="flex flex-wrap items-center gap-1.5"
                            >
                                <label className="sr-only" htmlFor={`${kind}-color-${item.id}`}>{Noun} color</label>
                                <input
                                    id={`${kind}-color-${item.id}`}
                                    type="color"
                                    value={editColor}
                                    onChange={(event) => setEditColor(event.target.value)}
                                    aria-label={`${Noun} color`}
                                    className="h-8 w-8 cursor-pointer rounded border border-line2 bg-surface p-0.5"
                                />
                                <label className="sr-only" htmlFor={`${kind}-name-${item.id}`}>{Noun} name</label>
                                <input
                                    id={`${kind}-name-${item.id}`}
                                    value={editName}
                                    onChange={(event) => setEditName(event.target.value)}
                                    maxLength={MAX_NAME}
                                    aria-label={`${Noun} name`}
                                    className={fieldClass}
                                />
                                <button type="submit" disabled={busy || !editName.trim()} className={buttonClass}>Save {noun}</button>
                                <button type="button" onClick={() => setEditingId(null)} disabled={busy} className={buttonClass}>Cancel</button>
                            </form>
                        ) : (
                            <div className="flex items-center gap-2">
                                <span
                                    className="h-2.5 w-2.5 shrink-0 rounded-full border border-line"
                                    style={{ backgroundColor: swatchColor(item.color) }}
                                    aria-hidden="true"
                                />
                                <span className="min-w-0 flex-1 truncate text-sm text-ink">{item.name}</span>
                                <button
                                    type="button"
                                    aria-label={`Rename ${noun} ${item.name}`}
                                    onClick={() => {
                                        setPendingDelete(null);
                                        setEditingId(item.id);
                                        setEditName(item.name);
                                        setEditColor(swatchColor(item.color) ?? DEFAULT_COLOR);
                                    }}
                                    disabled={busy}
                                    className="text-sm text-muted underline-offset-2 hover:text-ink hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 disabled:opacity-50"
                                >
                                    Rename
                                </button>
                                <button
                                    type="button"
                                    aria-label={`Delete ${noun} ${item.name}`}
                                    onClick={() => {
                                        setEditingId(null);
                                        setPendingDelete(item);
                                    }}
                                    disabled={busy}
                                    className="text-sm text-danger underline-offset-2 hover:underline focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-danger disabled:opacity-50"
                                >
                                    Delete
                                </button>
                            </div>
                        )}
                    </li>
                ))}
            </ul>
            <form onSubmit={(event) => { void create(event); }} className="flex flex-wrap items-center gap-1.5">
                <label className="sr-only" htmlFor={`new-${kind}-color`}>New {noun} color</label>
                <input
                    id={`new-${kind}-color`}
                    type="color"
                    value={color}
                    onChange={(event) => setColor(event.target.value)}
                    aria-label={`New ${noun} color`}
                    className="h-8 w-8 cursor-pointer rounded border border-line2 bg-surface p-0.5"
                />
                <label className="sr-only" htmlFor={`new-${kind}-name`}>New {noun} name</label>
                <input
                    id={`new-${kind}-name`}
                    value={name}
                    onChange={(event) => setName(event.target.value)}
                    maxLength={MAX_NAME}
                    placeholder={`${Noun} name`}
                    aria-label={`New ${noun} name`}
                    className={fieldClass}
                />
                <button type="submit" disabled={busy || !name.trim()} className={buttonClass}>Add {noun}</button>
            </form>
            {error && <p role="alert" className="mt-2 text-sm text-danger">{error}</p>}
        </section>
    );
}
