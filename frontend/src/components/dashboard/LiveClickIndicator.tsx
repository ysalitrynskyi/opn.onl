import type { LiveStatus } from '../../hooks/useLiveClicks';

const STATUS_LABEL: Record<LiveStatus, string> = {
    connected: 'Live',
    reconnecting: 'Reconnecting',
    off: 'Off',
};

/**
 * Header chip: connected, waiting to reconnect, or not streaming (tab hidden
 * or signed out). A status takes its accessible name from the author, not from
 * its text, so the visible word is repeated as `aria-label`.
 */
export default function LiveClickIndicator({ status }: { status: LiveStatus }) {
    const label = STATUS_LABEL[status];
    const dot = status === 'connected'
        ? 'bg-success'
        : status === 'reconnecting'
            ? 'bg-warning animate-pulse'
            : 'bg-faint';
    return (
        <span role="status" aria-label={label} className="inline-flex items-center gap-1.5">
            <span aria-hidden="true" className={`h-1.5 w-1.5 shrink-0 rounded-full ${dot}`} />
            {label}
        </span>
    );
}
