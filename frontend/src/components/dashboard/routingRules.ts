import type { RoutingRule } from './types';

const conditions = (rule: RoutingRule): string[] =>
    [rule.match_device, rule.match_os, rule.match_country, rule.match_lang]
        .map((value) => (value ?? '').trim().toLowerCase());

/**
 * Two rules with the same conditions can never both be reached in order — the
 * upper one always matches first — so they are an A/B split: they share a
 * priority and the server picks between them by weight.
 */
export function sharesConditions(a: RoutingRule, b: RoutingRule): boolean {
    const left = conditions(a);
    const right = conditions(b);
    return left.every((value, i) => value === right[i]);
}

/** Rules in the order the server evaluates them (lowest priority first). */
export function sortByPriority(rules: RoutingRule[]): RoutingRule[] {
    // Array.prototype.sort is stable, so rules that tie keep the order the API sent.
    return [...rules].sort((a, b) => (a.priority ?? 0) - (b.priority ?? 0));
}

/**
 * The body of `PUT /links/{id}/rules`: the editor's order is the priority.
 * Each rule gets its position as its priority; rules that form an A/B split
 * share the position of the first of them.
 */
export function rulesPayload(rules: RoutingRule[]): RoutingRule[] {
    let priority = -1;
    return rules.map((rule, i) => {
        if (i === 0 || !sharesConditions(rules[i - 1], rule)) priority += 1;
        return {
            priority,
            match_device: rule.match_device || null,
            match_os: rule.match_os || null,
            match_country: rule.match_country || null,
            match_lang: rule.match_lang || null,
            destination_url: rule.destination_url,
            weight: rule.weight ?? 1,
        };
    });
}

/**
 * What the user would save: the rules that have a destination, in order, as
 * their editable values. Priorities are left out because the editor derives
 * them from the order, so rules loaded with arbitrary priorities compare equal
 * until the user actually changes something.
 */
export function rulesSignature(rules: RoutingRule[]): string {
    return JSON.stringify(
        rules
            .filter((rule) => rule.destination_url.trim())
            .map((rule) => [...conditions(rule), rule.destination_url.trim(), rule.weight ?? 1]),
    );
}
