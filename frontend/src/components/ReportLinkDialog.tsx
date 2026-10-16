import { useEffect, useRef, useState } from 'react';
import { X } from 'lucide-react';
import { API_ENDPOINTS } from '../config/api';
import { useFocusTrap } from '../hooks/useFocusTrap';

const REASONS = [
    ['phishing', 'Phishing'],
    ['malware', 'Malware'],
    ['spam', 'Spam'],
    ['illegal', 'Illegal'],
    ['other', 'Other'],
] as const;

const MAX_DETAILS = 2000;

interface ReportLinkDialogProps {
    code: string;
    onClose: () => void;
}

/**
 * Public "report this link" form. Sends no Authorization header: a stale
 * session must not turn a report into a logout, and the report stores no
 * account id.
 */
export default function ReportLinkDialog({ code, onClose }: ReportLinkDialogProps) {
    const dialogRef = useRef<HTMLDivElement>(null);
    useFocusTrap(dialogRef);

    const [reason, setReason] = useState('');
    const [details, setDetails] = useState('');
    const [email, setEmail] = useState('');
    const [sending, setSending] = useState(false);
    const [error, setError] = useState('');
    const [done, setDone] = useState(false);
    const doneRef = useRef<HTMLParagraphElement>(null);

    useEffect(() => {
        if (done) doneRef.current?.focus();
    }, [done]);

    const submit = async (event: React.FormEvent) => {
        event.preventDefault();
        if (!reason || sending) return;
        setSending(true);
        setError('');
        const body: { code: string; reason: string; details?: string; email?: string } = {
            code,
            reason,
        };
        const note = details.trim();
        const address = email.trim();
        if (note) body.details = note;
        if (address) body.email = address;
        try {
            const res = await fetch(API_ENDPOINTS.report, {
                method: 'POST',
                headers: { 'Content-Type': 'application/json' },
                body: JSON.stringify(body),
            });
            if (res.ok) {
                setDone(true);
                return;
            }
            const payload = (await res.json().catch(() => null)) as {
                error?: string;
                message?: string;
                retry_after?: number;
            } | null;
            if (res.status === 404) {
                setError('This link could not be found.');
            } else if (res.status === 429) {
                setError(
                    typeof payload?.retry_after === 'number'
                        ? `Too many reports. Please try again in ${payload.retry_after} seconds.`
                        : payload?.message || payload?.error || 'Too many reports. Please try again later.',
                );
            } else if (res.status === 400) {
                setError(payload?.error || 'Could not send the report. Please try again.');
            } else {
                setError(payload?.error || payload?.message || 'Could not send the report. Please try again.');
            }
        } catch {
            setError('Could not send the report. Please try again.');
        } finally {
            setSending(false);
        }
    };

    return (
        <div
            className="fixed inset-0 z-50 flex items-center justify-center bg-black/50 p-4"
            onClick={onClose}
        >
            <div
                ref={dialogRef}
                role="dialog"
                aria-modal="true"
                aria-labelledby="report-link-title"
                className="w-full max-w-md rounded-2xl bg-white p-6 shadow-xl"
                onClick={(event) => event.stopPropagation()}
                onKeyDown={(event) => {
                    if (event.key === 'Escape') onClose();
                }}
            >
                <div className="mb-4 flex items-start justify-between gap-3">
                    <h2 id="report-link-title" className="text-lg font-semibold text-slate-900">
                        Report this link
                    </h2>
                    <button
                        type="button"
                        onClick={onClose}
                        aria-label="Close"
                        className="rounded-lg p-1 text-slate-500 hover:bg-slate-100 focus:outline-none focus:ring-2 focus:ring-primary-500"
                    >
                        <X className="h-4 w-4" />
                    </button>
                </div>

                {done ? (
                    <p
                        ref={doneRef}
                        role="status"
                        tabIndex={0}
                        className="rounded text-sm text-slate-700 focus:outline-none focus:ring-2 focus:ring-primary-500"
                    >
                        Thanks, an administrator will review this link
                    </p>
                ) : (
                    <form onSubmit={submit} className="space-y-4">
                        <fieldset>
                            <legend className="mb-2 text-sm font-medium text-slate-700">Reason</legend>
                            <div className="space-y-2">
                                {REASONS.map(([value, label]) => (
                                    <label key={value} className="flex items-center gap-2 text-sm text-slate-700">
                                        <input
                                            type="radio"
                                            name="reason"
                                            value={value}
                                            checked={reason === value}
                                            onChange={() => setReason(value)}
                                            required
                                            className="text-primary-600 focus:ring-primary-500"
                                        />
                                        {label}
                                    </label>
                                ))}
                            </div>
                        </fieldset>

                        <div>
                            <label htmlFor="report-details" className="mb-1 block text-sm font-medium text-slate-700">
                                Details
                            </label>
                            <textarea
                                id="report-details"
                                value={details}
                                onChange={(event) => setDetails(event.target.value)}
                                maxLength={MAX_DETAILS}
                                rows={4}
                                aria-describedby="report-details-count"
                                className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                            />
                            <p id="report-details-count" className="mt-1 text-right text-xs text-slate-500">
                                {details.length}/{MAX_DETAILS}
                            </p>
                        </div>

                        <div>
                            <label htmlFor="report-email" className="mb-1 block text-sm font-medium text-slate-700">
                                Email (optional)
                            </label>
                            <input
                                id="report-email"
                                type="email"
                                value={email}
                                onChange={(event) => setEmail(event.target.value)}
                                maxLength={254}
                                autoComplete="email"
                                className="w-full rounded-lg border border-slate-300 px-3 py-2 text-sm focus:outline-none focus:ring-2 focus:ring-primary-500"
                            />
                        </div>

                        {error && (
                            <p role="alert" className="text-sm text-red-700">
                                {error}
                            </p>
                        )}

                        <button
                            type="submit"
                            disabled={sending || !reason}
                            className="w-full rounded-xl bg-primary-600 px-4 py-2.5 text-sm font-medium text-white hover:bg-primary-700 focus:outline-none focus:ring-2 focus:ring-primary-500 disabled:cursor-not-allowed disabled:bg-slate-300"
                        >
                            {sending ? 'Sending…' : 'Submit report'}
                        </button>
                    </form>
                )}
            </div>
        </div>
    );
}
