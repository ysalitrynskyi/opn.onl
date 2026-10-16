import type { DestinationStatus } from './linkFields';

const buttonClass = 'inline-flex shrink-0 items-center justify-center rounded-lg border border-line2 bg-surface px-4 text-sm font-medium text-ink transition-colors hover:border-ink/30 disabled:opacity-70';

export function DestinationCheckButton({
    pending,
    disabled,
    onClick,
    className = 'py-2.5 w-full sm:w-auto',
}: {
    pending: boolean;
    disabled: boolean;
    onClick: () => void;
    className?: string;
}) {
    return (
        <button
            type="button"
            onClick={onClick}
            disabled={disabled || pending}
            aria-busy={pending}
            className={`${buttonClass} ${className}`}
        >
            Check destination
        </button>
    );
}

export function DestinationStatus({ status }: { status: DestinationStatus | null }) {
    if (!status) return null;
    return (
        <p role="status" className={`text-xs ${status.ok ? 'text-success' : 'text-danger'}`}>
            {status.text}
        </p>
    );
}
