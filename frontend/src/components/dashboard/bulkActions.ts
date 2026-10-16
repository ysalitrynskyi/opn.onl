import { pluralize } from '../../utils/plural';

/**
 * `POST /links/bulk/delete` and `POST /links/bulk/update` both refuse more
 * than this many ids. A larger selection is sent as several requests.
 */
export const BULK_IDS_PER_REQUEST = 500;

/** Split `items` into slices of `size`. The last slice may be shorter. */
export function chunkIds<T>(items: readonly T[], size = BULK_IDS_PER_REQUEST): T[][] {
    if (size < 1) {
        throw new Error('Bulk chunk size must be at least 1');
    }
    const chunks: T[][] = [];
    for (let i = 0; i < items.length; i += size) {
        chunks.push(items.slice(i, i + size));
    }
    return chunks;
}

/**
 * `deleted` or `updated` from a bulk response. Null when the body is missing
 * that count: the server does not name which ids it skipped, so a missing
 * count must not be read as "all of them".
 */
export function bulkAffectedCount(body: unknown, field: 'deleted' | 'updated'): number | null {
    if (!body || typeof body !== 'object') return null;
    const value = (body as Record<string, unknown>)[field];
    return typeof value === 'number' && Number.isFinite(value) && value >= 0 ? value : null;
}

export interface BulkRun {
    /** Sum of the counts the server reported for chunks it accepted. */
    affected: number;
    /** How many ids were in the chunks the server accepted. */
    sent: number;
    /** Why a later chunk was refused. Earlier chunks have already been applied. */
    error?: string;
}

export async function runBulkChunks(
    ids: readonly number[],
    postChunk: (
        chunk: readonly number[],
    ) => Promise<{ ok: true; affected: number } | { ok: false; error: string }>,
    size = BULK_IDS_PER_REQUEST,
): Promise<BulkRun> {
    let affected = 0;
    let sent = 0;
    for (const chunk of chunkIds(ids, size)) {
        const result = await postChunk(chunk);
        if (!result.ok) {
            return { affected, sent, error: result.error };
        }
        affected += result.affected;
        sent += chunk.length;
    }
    return { affected, sent };
}

/**
 * What to tell the user after every chunk was accepted. `complete` is false
 * when the server skipped some ids: the response is only a count, so the
 * shortfall is the whole of the failure report.
 */
export function bulkResultMessage(
    action: 'delete' | 'update',
    affected: number,
    sent: number,
): { complete: boolean; message: string } {
    const verb = action === 'delete' ? 'Deleted' : 'Updated';
    const participle = action === 'delete' ? 'deleted' : 'updated';
    if (affected >= sent) {
        return {
            complete: true,
            message: `${verb} ${affected.toLocaleString()} ${pluralize(affected, 'link')}.`,
        };
    }
    const skipped = sent - affected;
    return {
        complete: false,
        message: `${verb} ${affected.toLocaleString()} of ${sent.toLocaleString()} ${pluralize(sent, 'link')}. ${skipped.toLocaleString()} could not be ${participle}.`,
    };
}

/** A chunk failed after earlier chunks had already been accepted. */
export function bulkStoppedMessage(action: 'delete' | 'update', affected: number, error: string): string {
    const verb = action === 'delete' ? 'Deleted' : 'Updated';
    return `${verb} ${affected.toLocaleString()} ${pluralize(affected, 'link')}, then the rest failed: ${error}`;
}

/** End of the picked local day, the same instant the edit dialog sends for an expiry. */
export function endOfLocalDayIso(ymd: string): string {
    return new Date(`${ymd}T23:59:00`).toISOString();
}
