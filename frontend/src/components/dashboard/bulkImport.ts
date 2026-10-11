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

/** One entry of the bulk response's `failed` list. */
export interface BulkFailedEntry {
    url: string;
    error: string;
}

/**
 * Which submitted URLs failed, and why. The server lists them in `failed` as
 * `{url, error}`; a server from before that field only sends `errors`, one
 * string per failure (`<url>: <reason>` or `Failed to shorten <url>: <reason>`),
 * so the URLs are recovered from those messages instead.
 */
export function readBulkErrors(
    submitted: string[],
    errors: string[],
    failed?: BulkFailedEntry[],
): BulkOutcome {
    if (Array.isArray(failed)) {
        return fromFailedList(submitted, failed);
    }
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

    return { failures, remaining: keepFailed(submitted, failedCount) };
}

/** The submitted lines that failed, in order. A URL pasted twice is two lines;
 * keep as many as failed. */
function keepFailed(submitted: string[], failedCount: Map<string, number>): string[] {
    return submitted.filter((url) => {
        const left = failedCount.get(url) ?? 0;
        if (left === 0) return false;
        failedCount.set(url, left - 1);
        return true;
    });
}

function fromFailedList(submitted: string[], failed: BulkFailedEntry[]): BulkOutcome {
    const failedCount = new Map<string, number>();
    const failures: BulkFailure[] = failed.map(({ url, error }) => {
        failedCount.set(url, (failedCount.get(url) ?? 0) + 1);
        return { url, reason: reasonWithoutUrl(url, error) };
    });
    return { failures, remaining: keepFailed(submitted, failedCount) };
}

/** The message minus the URL it starts with, which the list already shows. */
function reasonWithoutUrl(url: string, error: string): string {
    for (const prefix of [`${SHORTEN_PREFIX}${url}: `, `${url}: `]) {
        if (error.startsWith(prefix)) return error.slice(prefix.length);
    }
    return error;
}
