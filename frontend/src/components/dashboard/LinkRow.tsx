import { useState, type Ref } from 'react';
import { Link } from 'react-router-dom';
import {
    Copy, Trash2, BarChart2, QrCode, Lock, Clock, Edit2, Check,
    Link2, Share2, Pin, CopyPlus, Eye,
} from 'lucide-react';
import { motion } from 'framer-motion';
import { shortLinkBase } from '../../config/api';
import Sparkline from '../Sparkline';
import MiniStats from './MiniStats';
import { pluralize } from '../../utils/plural';
import { swatchColor } from './linkScope';
import { clickCapReached, clickLimitLabel, formatBadgeDay, inactiveBadge } from './linkStatus';
import type { LinkData, SparklineSeries } from './types';

interface LinkRowProps {
    link: LinkData;
    linkNumber: number;
    sparkline?: SparklineSeries;
    copied: boolean;
    sourceCopied: boolean;
    cloning: boolean;
    selected: boolean;
    /** Non-zero while a live click should flash the count. A new value restarts it. */
    clickFlash?: number;
    onToggleSelect: (id: number) => void;
    onCopy: (link: LinkData) => void;
    onCopySource: (link: LinkData) => void;
    onPreview: (link: LinkData) => void;
    onPin: (link: LinkData) => void;
    onClone: (link: LinkData) => void;
    onShare: (link: LinkData) => void;
    onQr: (link: LinkData) => void;
    onEdit: (link: LinkData) => void;
    onDelete: (id: number) => void;
    /** Folder name for `link.folder_id`, when the current list includes that folder. */
    folder?: { name: string; color?: string | null } | null;
    // AnimatePresence mode="popLayout" measures the row by cloning it with a ref.
    ref?: Ref<HTMLDivElement>;
}

export default function LinkRow({
    link,
    linkNumber,
    sparkline,
    copied,
    sourceCopied,
    cloning,
    selected,
    clickFlash = 0,
    onToggleSelect,
    onCopy,
    onCopySource,
    onPreview,
    onPin,
    onClone,
    onShare,
    onQr,
    onEdit,
    onDelete,
    folder = null,
    ref,
}: LinkRowProps) {
    const mainUrl = `${shortLinkBase()}/${link.code}`;
    const apiUrl = link.api_url || link.short_url; // Use api_url from backend
    // Same clock snapshot as MiniStats: the badge is fixed for this visit.
    const [now] = useState(() => Date.now());
    const reason = inactiveBadge(link, now);
    const startsInFuture = !!link.starts_at && !Number.isNaN(Date.parse(link.starts_at)) && now < Date.parse(link.starts_at);
    const capReached = clickCapReached(link);

    const neutralBadge = 'inline-flex items-center gap-1 rounded border border-line px-2 py-0.5 text-xs font-medium text-muted';
    const dangerBadge = 'inline-flex items-center gap-1 rounded border border-danger/30 bg-danger/5 px-2 py-0.5 text-xs font-medium text-danger';

    return (
        <motion.div
            ref={ref}
            layout
            initial={{ opacity: 0, y: 16 }}
            animate={{ opacity: 1, y: 0 }}
            exit={{ opacity: 0, scale: 0.98 }}
            className={`group py-5 transition-colors hover:bg-primary-50/40 ${selected ? 'bg-primary-50/50' : ''}`}
        >
            <div className="flex items-start gap-3">
                <input
                    type="checkbox"
                    checked={selected}
                    onChange={() => onToggleSelect(link.id)}
                    aria-label={`Select link ${link.code}`}
                    className="mt-1.5 h-4 w-4 shrink-0 rounded border-line2 text-primary-600 focus:ring-primary-500"
                />
                <div className="flex min-w-0 flex-1 flex-col sm:flex-row sm:items-start justify-between gap-4">
                    <div className="space-y-1 min-w-0 flex-1">
                        {/* Link number and title */}
                        <div className="flex items-center gap-2 mb-1">
                            <span className="font-mono text-xs font-semibold text-faint">
                                #{linkNumber}
                            </span>
                            {link.is_pinned && (
                                <Pin className="h-3 w-3 text-primary-600 fill-current" aria-label="Pinned" />
                            )}
                            {link.title && (
                                <span className="text-sm font-medium text-ink truncate">
                                    {link.title}
                                </span>
                            )}
                        </div>

                        {/* Primary short URL → Destination */}
                        <div className="flex items-center gap-2 flex-wrap">
                            <a
                                href={mainUrl}
                                target="_blank"
                                rel="noreferrer"
                                className="font-mono text-lg font-bold text-primary-600 hover:underline"
                            >
                                {mainUrl.replace(/^https?:\/\//, '')}
                            </a>
                            <button
                                onClick={() => onCopy(link)}
                                className="rounded p-1 text-faint transition-colors hover:bg-primary-50 hover:text-primary-600"
                                title="Copy short URL"
                                aria-label={`Copy short URL for ${link.code}`}
                            >
                                {copied ? (
                                    <Check className="h-4 w-4 text-success" />
                                ) : (
                                    <Copy className="h-4 w-4" />
                                )}
                            </button>
                            <span className="text-faint" aria-hidden="true">→</span>
                            <a
                                href={link.original_url}
                                target="_blank"
                                rel="noreferrer"
                                className="truncate max-w-[300px] font-mono text-sm text-muted transition-colors hover:text-ink hover:underline"
                                title={link.original_url}
                            >
                                {link.original_url.replace(/^https?:\/\//, '').substring(0, 50)}{link.original_url.length > 60 ? '...' : ''}
                            </a>
                            <button
                                onClick={() => onCopySource(link)}
                                className="rounded p-1 text-faint transition-colors hover:bg-line hover:text-ink"
                                title="Copy source URL"
                                aria-label={`Copy source URL for ${link.code}`}
                            >
                                {sourceCopied ? (
                                    <Check className="h-4 w-4 text-success" />
                                ) : (
                                    <Link2 className="h-4 w-4" />
                                )}
                            </button>
                            {link.has_password && (
                                <span className={neutralBadge}>
                                    <Lock className="h-3 w-3" />
                                    Protected
                                </span>
                            )}
                            {link.expires_at && (
                                <span className={neutralBadge}>
                                    <Clock className="h-3 w-3" />
                                    {new Date(link.expires_at).toLocaleDateString()}
                                </span>
                            )}
                            {link.max_clicks != null && (
                                <span className={`${capReached ? dangerBadge : neutralBadge} tabular-nums`}>
                                    {clickLimitLabel(link.click_count, link.max_clicks)}
                                </span>
                            )}
                            {startsInFuture && link.starts_at && (
                                <span className={neutralBadge}>
                                    <Clock className="h-3 w-3" />
                                    Starts {formatBadgeDay(link.starts_at)}
                                </span>
                            )}
                            {reason ? (
                                <span className={dangerBadge} title={reason.reason}>
                                    {reason.label}
                                </span>
                            ) : !link.is_active && (
                                <span className={dangerBadge}>
                                    Inactive
                                </span>
                            )}
                        </div>
                        {/* API URL */}
                        <p className="font-mono text-xs text-faint">
                            API: <a href={apiUrl} target="_blank" rel="noreferrer" className="underline decoration-line2 underline-offset-2 hover:text-muted hover:decoration-current">{apiUrl}</a>
                        </p>
                        {(folder || (link.tags?.length ?? 0) > 0) && (
                            <div className="mt-2 flex flex-wrap items-center gap-1.5">
                                {folder && <LabelChip name={folder.name} color={folder.color} />}
                                {(link.tags ?? []).slice(0, VISIBLE_TAGS).map(tag => (
                                    <LabelChip key={tag.id} name={tag.name} color={tag.color} />
                                ))}
                                {(link.tags?.length ?? 0) > VISIBLE_TAGS && (
                                    <span
                                        className="text-xs text-faint"
                                        title={(link.tags ?? []).slice(VISIBLE_TAGS).map(tag => tag.name).join(', ')}
                                    >
                                        +{(link.tags?.length ?? 0) - VISIBLE_TAGS}
                                    </span>
                                )}
                            </div>
                        )}
                        {/* Mini stats */}
                        <MiniStats link={link} />
                    </div>

                    <div className="flex items-center gap-2 sm:gap-3">
                        {/* Sparkline Chart */}
                        {sparkline && (
                            <div className="hidden sm:block">
                                <Sparkline
                                    data={sparkline.data}
                                    labels={sparkline.labels}
                                    width={70}
                                    height={24}
                                />
                            </div>
                        )}
                        <Link
                            to={`/analytics/${link.id}`}
                            aria-label={`${link.click_count.toLocaleString()} ${pluralize(link.click_count, 'click')}, analytics for ${link.code}`}
                            className="inline-flex items-center gap-1.5 text-sm font-medium text-muted transition-colors hover:text-primary-600"
                        >
                            <BarChart2 className="h-4 w-4" />
                            <span
                                key={clickFlash}
                                className={`font-mono font-bold tabular-nums${clickFlash ? ' text-primary-600 animate-live-click' : ''}`}
                            >
                                {link.click_count.toLocaleString()}
                            </span>
                        </Link>
                        <div className="flex items-center gap-0.5">
                            <button
                                onClick={() => onPreview(link)}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                title="Preview destination"
                                aria-label={`Preview destination for ${link.code}`}
                            >
                                <Eye className="h-4 w-4" />
                            </button>
                            <button
                                onClick={() => onPin(link)}
                                className={`rounded-md p-2 transition-colors hover:bg-line ${link.is_pinned ? 'text-primary-600' : 'text-faint hover:text-ink'}`}
                                title={link.is_pinned ? 'Unpin' : 'Pin'}
                                aria-label={`${link.is_pinned ? 'Unpin' : 'Pin'} link ${link.code}`}
                            >
                                <Pin className={`h-4 w-4 ${link.is_pinned ? 'fill-current' : ''}`} />
                            </button>
                            <button
                                onClick={() => onClone(link)}
                                disabled={cloning}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink disabled:cursor-wait disabled:opacity-50"
                                title="Clone"
                                aria-label={`Clone link ${link.code}`}
                            >
                                <CopyPlus className="h-4 w-4" />
                            </button>
                            <button
                                onClick={() => onShare(link)}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                title="Share"
                                aria-label={`Share link ${link.code}`}
                            >
                                <Share2 className="h-4 w-4" />
                            </button>
                            <button
                                onClick={() => onQr(link)}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                title="QR Code"
                                aria-label={`Show QR code for ${link.code}`}
                            >
                                <QrCode className="h-4 w-4" />
                            </button>
                            <button
                                onClick={() => onEdit(link)}
                                data-edit-link={link.id}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-line hover:text-ink"
                                title="Edit"
                                aria-label={`Edit link ${link.code}`}
                            >
                                <Edit2 className="h-4 w-4" />
                            </button>
                            <button
                                onClick={() => onDelete(link.id)}
                                className="rounded-md p-2 text-faint transition-colors hover:bg-danger/5 hover:text-danger"
                                title="Delete"
                                aria-label={`Delete link ${link.code}`}
                            >
                                <Trash2 className="h-4 w-4" />
                            </button>
                        </div>
                    </div>
                </div>
            </div>
        </motion.div>
    );
}

// A row shows two tags and a count of the rest, so a heavily tagged link
// keeps the same height as any other.
const VISIBLE_TAGS = 2;

function LabelChip({ name, color }: { name: string; color?: string | null }) {
    const swatch = swatchColor(color);
    return (
        <span className="inline-flex items-center gap-1 rounded border border-line px-1.5 py-0.5 text-xs text-muted">
            {swatch && <span className="h-2 w-2 rounded-full" style={{ backgroundColor: swatch }} aria-hidden="true" />}
            {name}
        </span>
    );
}
