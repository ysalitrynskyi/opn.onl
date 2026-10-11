import { useEffect, useRef } from 'react';
import { ArrowDown, ArrowUp, Plus, Trash2 } from 'lucide-react';
import type { RoutingRule } from './types';
import { sharesConditions } from './routingRules';

const DEVICES = ['', 'Mobile', 'Tablet', 'Desktop'];
const MAX_WEIGHT = 1000;

type Direction = 'up' | 'down';

/**
 * Controlled editor for a link's smart-routing rules. Pure presentation — the
 * parent owns the array (and the persistence). The order of the list is the
 * priority the parent saves, so the controls here only have to reorder it.
 */
export default function RoutingRulesEditor({
    rules,
    onChange,
}: {
    rules: RoutingRule[];
    onChange: (rules: RoutingRule[]) => void;
}) {
    const listRef = useRef<HTMLDivElement>(null);
    const focusAfterMove = useRef<string | null>(null);

    // The moved rule changes position, so the button the user pressed now sits
    // on a different rule; put focus on the moved rule's own button instead.
    useEffect(() => {
        if (!focusAfterMove.current) return;
        listRef.current
            ?.querySelector<HTMLButtonElement>(`[data-move="${focusAfterMove.current}"]`)
            ?.focus();
        focusAfterMove.current = null;
    }, [rules]);

    const update = (i: number, patch: Partial<RoutingRule>) =>
        onChange(rules.map((r, idx) => (idx === i ? { ...r, ...patch } : r)));
    const remove = (i: number) => onChange(rules.filter((_, idx) => idx !== i));
    const add = () => onChange([...rules, { destination_url: '', weight: 1 }]);
    const move = (i: number, direction: Direction) => {
        const target = direction === 'up' ? i - 1 : i + 1;
        if (target < 0 || target >= rules.length) return;
        const next = [...rules];
        [next[i], next[target]] = [next[target], next[i]];
        // Keep the focus on the same arrow unless the rule just reached the end it points at.
        const atEnd = direction === 'up' ? target === 0 : target === rules.length - 1;
        const focused: Direction = atEnd ? (direction === 'up' ? 'down' : 'up') : direction;
        focusAfterMove.current = `${target}-${focused}`;
        onChange(next);
    };
    // Neighbours with identical conditions are an A/B split (see routingRules.ts).
    const inSplit = (i: number) =>
        (i > 0 && sharesConditions(rules[i - 1], rules[i])) ||
        (i < rules.length - 1 && sharesConditions(rules[i], rules[i + 1]));

    return (
        <div ref={listRef} className="space-y-3">
            <div className="space-y-1 text-xs text-faint">
                <p className="font-medium text-muted">First matching rule wins.</p>
                <p>
                    Rules are checked from top to bottom. Leave a field blank to match anything — a
                    rule with everything blank is the catch-all default. The link's own URL is the
                    final fallback. Neighbouring rules with identical conditions split the traffic
                    between them by weight (A/B).
                </p>
            </div>

            {rules.map((r, i) => (
                <div key={i} className="rounded-lg border border-line2 p-3 space-y-2">
                    <div className="flex items-center justify-between">
                        <span className="inline-flex items-center gap-2 font-mono text-xs uppercase tracking-[0.14em] text-faint">
                            Rule {i + 1}
                            {inSplit(i) && (
                                <span className="rounded border border-line2 px-1.5 py-0.5 normal-case tracking-normal text-muted">
                                    A/B split
                                </span>
                            )}
                        </span>
                        <span className="inline-flex items-center gap-1">
                            <button
                                type="button"
                                data-move={`${i}-up`}
                                aria-label={`Move rule ${i + 1} up`}
                                disabled={i === 0}
                                onClick={() => move(i, 'up')}
                                className="rounded p-1 text-faint transition-colors hover:text-ink disabled:opacity-40 disabled:hover:text-faint"
                            >
                                <ArrowUp className="h-3.5 w-3.5" />
                            </button>
                            <button
                                type="button"
                                data-move={`${i}-down`}
                                aria-label={`Move rule ${i + 1} down`}
                                disabled={i === rules.length - 1}
                                onClick={() => move(i, 'down')}
                                className="rounded p-1 text-faint transition-colors hover:text-ink disabled:opacity-40 disabled:hover:text-faint"
                            >
                                <ArrowDown className="h-3.5 w-3.5" />
                            </button>
                        </span>
                    </div>
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
                                maxLength={3}
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
                        {inSplit(i) ? (
                            <label className="inline-flex items-center gap-2 text-xs text-faint">
                                Weight (A/B)
                                <input
                                    type="number"
                                    min={1}
                                    max={MAX_WEIGHT}
                                    value={r.weight ?? 1}
                                    onChange={(e) =>
                                        update(i, {
                                            weight: Math.min(MAX_WEIGHT, Math.max(1, parseInt(e.target.value, 10) || 1)),
                                        })
                                    }
                                    className="w-20 rounded-md border border-line2 bg-surface px-2 py-1 text-sm text-ink"
                                />
                            </label>
                        ) : (
                            <span />
                        )}
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
