import { describe, expect, it } from 'vitest';
import {
    BULK_IDS_PER_REQUEST,
    bulkAffectedCount,
    bulkResultMessage,
    bulkStoppedMessage,
    chunkIds,
    endOfLocalDayIso,
    runBulkChunks,
} from './bulkActions';

describe('chunkIds', () => {
    it('keeps a short list in one chunk', () => {
        expect(chunkIds([1, 2, 3])).toEqual([[1, 2, 3]]);
    });

    it('splits on the server cap of 500', () => {
        const ids = Array.from({ length: BULK_IDS_PER_REQUEST + 1 }, (_, i) => i + 1);
        const chunks = chunkIds(ids);
        expect(chunks).toHaveLength(2);
        expect(chunks[0]).toHaveLength(BULK_IDS_PER_REQUEST);
        expect(chunks[1]).toEqual([BULK_IDS_PER_REQUEST + 1]);
    });

    it('uses an explicit size when a test asks for one', () => {
        expect(chunkIds([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
    });

    it('returns nothing for an empty selection', () => {
        expect(chunkIds([])).toEqual([]);
    });
});

describe('bulk responses', () => {
    it('reads a count and refuses a body that has none', () => {
        expect(bulkAffectedCount({ deleted: 2 }, 'deleted')).toBe(2);
        expect(bulkAffectedCount({ updated: 0 }, 'updated')).toBe(0);
        expect(bulkAffectedCount({}, 'deleted')).toBeNull();
        expect(bulkAffectedCount({ deleted: '2' }, 'deleted')).toBeNull();
        expect(bulkAffectedCount(null, 'deleted')).toBeNull();
    });

    it('treats a shortfall as a failure and does not name ids the server omitted', () => {
        expect(bulkResultMessage('delete', 2, 2)).toEqual({
            complete: true,
            message: 'Deleted 2 links.',
        });
        expect(bulkResultMessage('delete', 1, 1)).toEqual({
            complete: true,
            message: 'Deleted 1 link.',
        });
        expect(bulkResultMessage('delete', 1, 2)).toEqual({
            complete: false,
            message: 'Deleted 1 of 2 links. 1 could not be deleted.',
        });
        expect(bulkResultMessage('update', 0, 3)).toEqual({
            complete: false,
            message: 'Updated 0 of 3 links. 3 could not be updated.',
        });
    });

    it('says a later chunk failed after the earlier ones were counted', () => {
        expect(bulkStoppedMessage('delete', 2, 'Database error')).toBe(
            'Deleted 2 links, then the rest failed: Database error',
        );
    });

    it('stops at the first refused chunk and keeps the count already accepted', async () => {
        const seen: number[][] = [];
        const run = await runBulkChunks([1, 2, 3, 4, 5], async (chunk) => {
            seen.push([...chunk]);
            if (seen.length === 2) return { ok: false, error: 'Database error' };
            return { ok: true, affected: chunk.length };
        }, 2);

        expect(seen).toEqual([[1, 2], [3, 4]]);
        expect(run).toEqual({ affected: 2, sent: 2, error: 'Database error' });
    });
});

describe('endOfLocalDayIso', () => {
    it('is the end of the picked local day, matching a single-link edit', () => {
        expect(endOfLocalDayIso('2030-01-15')).toBe(new Date('2030-01-15T23:59:00').toISOString());
    });
});
