import { Plus, Trash2 } from 'lucide-react';
import type { RoutingRule } from './types';

const DEVICES = ['', 'Mobile', 'Tablet', 'Desktop'];

/**
 * Controlled editor for a link's smart-routing rules. Pure presentation — the
 * parent owns the array (and the persistence).
 */
export default function RoutingRulesEditor({
    rules,
    onChange,
}: {
    rules: RoutingRule[];
    onChange: (rules: RoutingRule[]) => void;
}) {
    const update = (i: number, patch: Partial<RoutingRule>) =>
        onChange(rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
    const remove = (i: number) => onChange(rules.filter((_, idx) => idx !== i));
    const add = () => onChange([...rules, { destination_url: '', weight: 1 }]);

    return (
        <div className="space-y-3">
            <p className="text-xs text-faint">
                Rules are evaluated top to bottom; the first match wins. Leave a field blank to match
                anything — a rule with everything blank is the catch-all default. The link's own URL is
                the final fallback.
            </p>

            {rules.map((r, i) => (
                <div key={i} className="rounded-lg border border-line2 p-3 space-y-2">
                    <div className="grid grid-cols-2 gap-2">
                        <label className="block text-xs text-faint">
                            Device
                            <select
                                value={r.match_device ?? ''}
                                onChange={(e) => update(i, { match_device: e.target.value || null })}
                                className="mt-1 w-full rounded-md border border-line2 bg-surface px-2 py-1.5 text-sm text-ink"
                            >
                                {DEVICES.map((d) => (
                                    <option key={d} value={d}>{d || 'Any'}</option>
                                ))}
                            </select>
                        </label>
                        <label className="block text-xs text-faint">
                            OS
                            <input
                                value={r.match_os ?? ''}
                                onChange={(e) => update(i, { match_os: e.target.value || null })}
                                placeholder="Any (e.g. iOS)"
                                className="mt-1 w-full rounded-md border border-line2 bg-surface px-2 py-1.5 text-sm text-ink placeholder:text-faint"
                            />
                        </label>
                        <label className="block text-xs text-faint">
                            Country (ISO)
                            <input
                                value={r.match_country ?? ''}
                                onChange={(e) => update(i, { match_country: e.target.value.toUpperCase() || null })}
                                placeholder="Any (e.g. US)"
                                maxLength={2}
                                className="mt-1 w-full rounded-md border border-line2 bg-surface px-2 py-1.5 text-sm text-ink placeholder:text-faint"
                            />
                        </label>
                        <label className="block text-xs text-faint">
                            Language
                            <input
                                value={r.match_lang ?? ''}
                                onChange={(e) => update(i, { match_lang: e.target.value.toLowerCase() || null })}
                                placeholder="Any (e.g. en)"
                                maxLength={5}
                                className="mt-1 w-full rounded-md border border-line2 bg-surface px-2 py-1.5 text-sm text-ink placeholder:text-faint"
                            />
                        </label>
                    </div>
                    <label className="block text-xs text-faint">
                        Destination URL
                        <input
                            value={r.destination_url}
                            onChange={(e) => update(i, { destination_url: e.target.value })}
                            placeholder="https://…"
                            className="mt-1 w-full rounded-md border border-line2 bg-surface px-2 py-1.5 font-mono text-sm text-ink placeholder:text-faint"
                        />
                    </label>
                    <div className="flex items-center justify-between">
                        <label className="inline-flex items-center gap-2 text-xs text-faint">
                            Weight (A/B)
                            <input
                                type="number"
                                min={1}
                                value={r.weight ?? 1}
                                onChange={(e) => update(i, { weight: Math.max(1, parseInt(e.target.value, 10) || 1) })}
                                className="w-16 rounded-md border border-line2 bg-surface px-2 py-1 text-sm text-ink"
                            />
                        </label>
                        <button
                            type="button"
                            onClick={() => remove(i)}
                            className="inline-flex items-center gap-1 text-xs text-danger hover:underline"
                        >
                            <Trash2 className="h-3.5 w-3.5" /> Remove
                        </button>
                    </div>
                </div>
            ))}

            <button
                type="button"
                onClick={add}
                className="inline-flex items-center gap-1.5 rounded-lg border border-line2 px-3 py-1.5 text-sm font-medium text-muted transition-colors hover:text-ink hover:border-ink/30"
            >
                <Plus className="h-3.5 w-3.5" /> Add rule
            </button>
        </div>
    );
}
