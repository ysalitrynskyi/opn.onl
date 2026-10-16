import { useRef, useState } from 'react';
import { ChevronDown } from 'lucide-react';
import { API_ENDPOINTS, authFetch, responseError, type BuildUtmResponse } from '../../config/api';

const FIELDS = [
    ['utm_source', 'Source'],
    ['utm_medium', 'Medium'],
    ['utm_campaign', 'Campaign'],
    ['utm_term', 'Term'],
    ['utm_content', 'Content'],
] as const;

const inputClass = 'w-full rounded-lg border border-line2 bg-surface px-4 py-2 text-sm text-ink outline-none transition-colors focus:border-primary-500 placeholder:text-faint';

/**
 * Writes `url_with_utm` from POST /links/build-utm into the destination.
 * The server decides encoding and how existing query parameters are kept.
 * `onApply` gets the destination the answer was built from, so the caller can
 * drop an answer that arrives after the destination changed.
 */
export default function UtmBuilder({
    url,
    onApply,
}: {
    url: string;
    onApply: (next: string, builtFrom: string) => void;
}) {
    const [open, setOpen] = useState(false);
    const [values, setValues] = useState<Record<string, string>>({});
    const [pending, setPending] = useState(false);
    const [error, setError] = useState('');
    const pendingRef = useRef(false);
    const urlRef = useRef(url);
    urlRef.current = url;

    const setField = (key: string, value: string) => {
        setValues(prev => ({ ...prev, [key]: value }));
        setError('');
    };

    const hasParam = FIELDS.some(([key]) => (values[key] ?? '').trim() !== '');
    const canApply = Boolean(url.trim()) && hasParam;

    const apply = async () => {
        if (!canApply || pendingRef.current) return;
        pendingRef.current = true;
        setPending(true);
        setError('');
        const requested = url.trim();
        const body: Record<string, string> = { url: requested };
        for (const [key] of FIELDS) {
            const value = (values[key] ?? '').trim();
            if (value) body[key] = value;
        }
        try {
            const res = await authFetch(API_ENDPOINTS.buildUtm, {
                method: 'POST',
                body: JSON.stringify(body),
            });
            const failure = res.ok ? '' : await responseError(res, 'Could not add UTM parameters');
            const data = res.ok ? await res.json() as BuildUtmResponse : null;
            // The destination was edited while this was in flight. The answer
            // is about the old one; the parameters stay for the new one.
            if (urlRef.current.trim() !== requested) return;
            if (failure) {
                setError(failure);
                return;
            }
            if (!data?.url_with_utm) {
                setError('Could not add UTM parameters');
                return;
            }
            onApply(data.url_with_utm, requested);
            setValues({});
        } catch {
            setError('Could not add UTM parameters. Try again.');
        } finally {
            pendingRef.current = false;
            setPending(false);
        }
    };

    return (
        <div>
            <button
                type="button"
                aria-expanded={open}
                onClick={() => setOpen(v => !v)}
                className="inline-flex items-center gap-1 text-sm text-muted transition-colors hover:text-ink"
            >
                <ChevronDown className={`h-4 w-4 transition-transform ${open ? 'rotate-180' : ''}`} />
                Add UTM parameters
            </button>
            {open && (
                <div className="mt-3 space-y-3">
                    <div className="grid sm:grid-cols-2 gap-3">
                        {FIELDS.map(([key, label]) => (
                            <div key={key}>
                                <label htmlFor={`utm-${key}`} className="block font-mono text-xs uppercase tracking-[0.14em] text-faint mb-1.5">
                                    {label}
                                </label>
                                <input
                                    id={`utm-${key}`}
                                    type="text"
                                    value={values[key] ?? ''}
                                    onChange={(e) => setField(key, e.target.value)}
                                    onKeyDown={(e) => {
                                        // These fields sit inside the create form, whose
                                        // only submit button is Create. Enter here means
                                        // "add these", not "create the link without them".
                                        if (e.key !== 'Enter' || e.nativeEvent.isComposing) return;
                                        e.preventDefault();
                                        void apply();
                                    }}
                                    className={inputClass}
                                    autoComplete="off"
                                />
                            </div>
                        ))}
                    </div>
                    <button
                        type="button"
                        onClick={() => void apply()}
                        disabled={!canApply || pending}
                        aria-busy={pending}
                        className="inline-flex items-center justify-center rounded-lg border border-line2 bg-surface px-4 py-2 text-sm font-medium text-ink transition-colors hover:border-ink/30 disabled:opacity-70"
                    >
                        Add to URL
                    </button>
                    {error && <p role="alert" className="text-xs text-danger">{error}</p>}
                </div>
            )}
        </div>
    );
}
