import { useEffect, useRef, useState, type ReactNode } from 'react';
import { CalendarClock, Clock, FolderInput, Trash2, X } from 'lucide-react';
import { motion } from 'framer-motion';
import { useFocusTrap } from '../../hooks/useFocusTrap';
import { pluralize } from '../../utils/plural';
import { localIsoDate } from '../../utils/localIsoDate';
import { endOfLocalDayIso } from './bulkActions';
import type { CatalogItem } from './linkScope';

interface BulkActionBarProps {
    count: number;
    busy: boolean;
    /** Resolves true when the selection was cleared and focus should leave the bar. */
    onDelete: () => Promise<boolean>;
    onSetExpiry: (expiresAt: string) => Promise<boolean>;
    onRemoveExpiry: () => Promise<boolean>;
    /** `null` clears the folder (`remove_folder`). A number files the links there. */
    onMoveToFolder: (folderId: number | null) => Promise<boolean>;
    folders?: CatalogItem[];
}

type BulkDialog = 'delete' | 'expiry' | 'move';

const buttonClass =
    'inline-flex items-center gap-1.5 rounded-lg border border-line2 bg-surface px-3 py-1.5 text-sm font-medium text-ink transition-colors hover:border-ink/30 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 disabled:cursor-wait disabled:opacity-50';

/**
 * Selection actions. Fixed to the bottom of the screen on a narrow viewport,
 * in normal flow on a wide one. Dialogs live here so closing one returns
 * focus to the button that opened it.
 */
export default function BulkActionBar({
    count,
    busy,
    onDelete,
    onSetExpiry,
    onRemoveExpiry,
    onMoveToFolder,
    folders = [],
}: BulkActionBarProps) {
    const [dialog, setDialog] = useState<BulkDialog | null>(null);
    const alive = useRef(true);
    useEffect(() => {
        alive.current = true;
        return () => {
            alive.current = false;
        };
    }, []);

    const run = async (task: () => Promise<boolean>) => {
        await task();
        // The bar unmounts when the selection is cleared, and the dialog goes
        // with it. Setting state then would update a component that is gone.
        if (alive.current) setDialog(null);
    };

    return (
        <>
            <div
                role="toolbar"
                aria-label="Bulk actions"
                aria-busy={busy}
                className="fixed inset-x-0 bottom-0 z-40 flex flex-wrap items-center gap-2 border-t border-line bg-surface px-4 py-3 shadow-lift sm:static sm:z-auto sm:mb-4 sm:rounded-xl sm:border sm:px-4 sm:shadow-none"
            >
                <span className="mr-1 text-sm font-medium text-ink" aria-live="polite">
                    {count.toLocaleString()} {pluralize(count, 'link')} selected
                </span>
                <button
                    type="button"
                    onClick={() => setDialog('delete')}
                    disabled={busy}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-danger px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-danger/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-danger disabled:cursor-wait disabled:opacity-50"
                >
                    <Trash2 className="h-4 w-4" />
                    Delete
                </button>
                <button type="button" onClick={() => setDialog('expiry')} disabled={busy} className={buttonClass}>
                    <CalendarClock className="h-4 w-4 text-muted" />
                    Set expiry
                </button>
                <button type="button" onClick={() => { void run(onRemoveExpiry); }} disabled={busy} className={buttonClass}>
                    <Clock className="h-4 w-4 text-muted" />
                    Remove expiry
                </button>
                <button type="button" onClick={() => setDialog('move')} disabled={busy} className={buttonClass}>
                    <FolderInput className="h-4 w-4 text-muted" />
                    Move to folder
                </button>
            </div>
            {dialog === 'delete' && (
                <ConfirmDeleteDialog
                    count={count}
                    busy={busy}
                    onCancel={() => setDialog(null)}
                    onConfirm={() => { void run(onDelete); }}
                />
            )}
            {dialog === 'expiry' && (
                <SetExpiryDialog
                    count={count}
                    busy={busy}
                    onCancel={() => setDialog(null)}
                    onConfirm={(ymd) => { void run(() => onSetExpiry(endOfLocalDayIso(ymd))); }}
                />
            )}
            {dialog === 'move' && (
                <MoveFolderDialog
                    folders={folders}
                    busy={busy}
                    onCancel={() => setDialog(null)}
                    onConfirm={(folderId) => { void run(() => onMoveToFolder(folderId)); }}
                />
            )}
        </>
    );
}

interface SelectAllCheckboxProps {
    checked: boolean;
    indeterminate: boolean;
    onChange: () => void;
}

/** The box and the words beside it, which are its label: either one toggles the page. */
export function SelectAllCheckbox({ checked, indeterminate, onChange }: SelectAllCheckboxProps) {
    const ref = useRef<HTMLInputElement>(null);
    useEffect(() => {
        if (ref.current) ref.current.indeterminate = indeterminate;
    }, [indeterminate]);

    return (
        <>
            <input
                ref={ref}
                id="link-select-all"
                type="checkbox"
                checked={checked}
                onChange={onChange}
                className="h-4 w-4 rounded border-line2 text-primary-600 focus:ring-primary-500"
            />
            <label htmlFor="link-select-all" className="cursor-pointer text-sm text-muted">
                Select all on this page
            </label>
        </>
    );
}

function DialogFrame({
    titleId,
    title,
    busy,
    onClose,
    children,
}: {
    titleId: string;
    title: string;
    busy: boolean;
    onClose: () => void;
    children: ReactNode;
}) {
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);

    useEffect(() => {
        const onKeyDown = (event: KeyboardEvent) => {
            if (event.key !== 'Escape' || busy) return;
            // The page shortcut also listens for Escape, on window, and would
            // clear the search behind the dialog.
            event.preventDefault();
            event.stopPropagation();
            onClose();
        };
        document.addEventListener('keydown', onKeyDown);
        return () => document.removeEventListener('keydown', onKeyDown);
    }, [busy, onClose]);

    return (
        <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="fixed inset-0 z-50 flex items-center justify-center bg-ink/40 p-4 backdrop-blur-sm"
            onClick={busy ? undefined : onClose}
        >
            <motion.div
                ref={dialogRef}
                role="dialog"
                aria-modal="true"
                aria-labelledby={titleId}
                initial={{ scale: 0.97, opacity: 0, y: 8 }}
                animate={{ scale: 1, opacity: 1, y: 0 }}
                exit={{ scale: 0.97, opacity: 0, y: 8 }}
                transition={{ duration: 0.18, ease: [0.16, 1, 0.3, 1] }}
                className="w-full max-w-md rounded-2xl border border-line2 bg-surface p-6 shadow-lift"
                onClick={(event) => event.stopPropagation()}
            >
                <div className="mb-4 flex items-center justify-between gap-3">
                    <h3 id={titleId} className="font-display text-xl font-bold tracking-tight text-ink">{title}</h3>
                    <button
                        type="button"
                        onClick={onClose}
                        disabled={busy}
                        aria-label="Close"
                        className="text-faint transition-colors hover:text-ink disabled:opacity-50"
                    >
                        <X className="h-5 w-5" />
                    </button>
                </div>
                {children}
            </motion.div>
        </motion.div>
    );
}

function ConfirmDeleteDialog({
    count,
    busy,
    onCancel,
    onConfirm,
}: {
    count: number;
    busy: boolean;
    onCancel: () => void;
    onConfirm: () => void;
}) {
    const noun = pluralize(count, 'link');
    return (
        <DialogFrame titleId="bulk-delete-title" title={`Delete ${count.toLocaleString()} ${noun}?`} busy={busy} onClose={onCancel}>
            <p className="text-sm text-muted">They stop redirecting immediately.</p>
            <div className="mt-6 flex justify-end gap-2">
                <button type="button" onClick={onCancel} disabled={busy} className={buttonClass}>Cancel</button>
                <button
                    type="button"
                    onClick={onConfirm}
                    disabled={busy}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-danger px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-danger/90 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-danger disabled:cursor-wait disabled:opacity-50"
                >
                    Delete
                </button>
            </div>
        </DialogFrame>
    );
}

function SetExpiryDialog({
    count,
    busy,
    onCancel,
    onConfirm,
}: {
    count: number;
    busy: boolean;
    onCancel: () => void;
    onConfirm: (ymd: string) => void;
}) {
    const [ymd, setYmd] = useState('');
    const noun = pluralize(count, 'link');
    return (
        <DialogFrame titleId="bulk-expiry-title" title="Set expiry" busy={busy} onClose={onCancel}>
            <p className="mb-4 text-sm text-muted">
                {count.toLocaleString()} {noun} stop redirecting at the end of this day.
            </p>
            <label htmlFor="bulk-expires" className="mb-1.5 block font-mono text-xs uppercase tracking-[0.14em] text-faint">
                Expiration
            </label>
            <input
                id="bulk-expires"
                type="date"
                value={ymd}
                min={localIsoDate()}
                onChange={(event) => setYmd(event.target.value)}
                className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
            />
            <div className="mt-6 flex justify-end gap-2">
                <button type="button" onClick={onCancel} disabled={busy} className={buttonClass}>Cancel</button>
                <button
                    type="button"
                    onClick={() => onConfirm(ymd)}
                    disabled={busy || !ymd}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-primary-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 disabled:cursor-not-allowed disabled:opacity-50"
                >
                    Set expiry
                </button>
            </div>
        </DialogFrame>
    );
}

function MoveFolderDialog({
    folders,
    busy,
    onCancel,
    onConfirm,
}: {
    folders: CatalogItem[];
    busy: boolean;
    onCancel: () => void;
    onConfirm: (folderId: number | null) => void;
}) {
    const [choice, setChoice] = useState('');
    return (
        <DialogFrame titleId="bulk-move-title" title="Move to folder" busy={busy} onClose={onCancel}>
            <label htmlFor="bulk-folder" className="mb-1.5 block font-mono text-xs uppercase tracking-[0.14em] text-faint">
                Folder
            </label>
            <select
                id="bulk-folder"
                value={choice}
                onChange={(event) => setChoice(event.target.value)}
                className="w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500"
            >
                <option value="">No folder</option>
                {folders.map(folder => (
                    <option key={folder.id} value={String(folder.id)}>{folder.name}</option>
                ))}
            </select>
            <div className="mt-6 flex justify-end gap-2">
                <button type="button" onClick={onCancel} disabled={busy} className={buttonClass}>Cancel</button>
                <button
                    type="button"
                    onClick={() => onConfirm(choice ? Number(choice) : null)}
                    disabled={busy}
                    className="inline-flex items-center gap-1.5 rounded-lg bg-primary-600 px-3 py-1.5 text-sm font-medium text-white transition-colors hover:bg-primary-700 focus-visible:outline focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-primary-500 disabled:cursor-wait disabled:opacity-50"
                >
                    Move
                </button>
            </div>
        </DialogFrame>
    );
}
