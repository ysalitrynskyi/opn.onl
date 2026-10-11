/** One URL per line; a line that is not an http(s) URL is not sent. */
export function parseBulkUrls(text: string): string[] {
    return text
        .split('\n')
        .map((line) => line.trim())
        .filter((line) => line && /^https?:\/\/.+/.test(line));
}

export interface BulkFailure {
    /** The submitted URL the server turned down; null when the message names none. */
    url: string | null;
    reason: string;
}

export interface BulkOutcome {
    failures: BulkFailure[];
    /** The submitted URLs that did not make it, in the order they were sent. */
    remaining: string[];
}

const SHORTEN_PREFIX = 'Failed to shorten ';

/**
 * The server answers a bulk request with one string per failure, either
 * `<url>: <reason>` or `Failed to shorten <url>: <reason>`, and does not name
 * the URLs that worked. So the failed ones are recovered from the messages.
 */
export function readBulkErrors(submitted: string[], errors: string[]): BulkOutcome {
    const sent = new Set(submitted);
    const failedCount = new Map<string, number>();
    const failures: BulkFailure[] = [];

    for (const message of errors) {
        const text = message.startsWith(SHORTEN_PREFIX) ? message.slice(SHORTEN_PREFIX.length) : message;
        const split = text.indexOf(': ');
        const url = split > 0 ? text.slice(0, split) : null;
        if (url !== null && sent.has(url)) {
            failures.push({ url, reason: text.slice(split + 2) });
            failedCount.set(url, (failedCount.get(url) ?? 0) + 1);
        } else {
            failures.push({ url: null, reason: message });
        }
    }

    // A URL pasted twice is two lines; keep as many as failed.
    const remaining = submitted.filter((url) => {
        const left = failedCount.get(url) ?? 0;
        if (left === 0) return false;
        failedCount.set(url, left - 1);
        return true;
    });

    return { failures, remaining };
}
