import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, expect, it } from 'vitest';

// Crawlers and older browsers ask for /favicon.ico whatever the page links.
// nginx serves public files as they are, so a missing file was a 404.
describe('/favicon.ico', () => {
    it('is an icon file holding the 16, 32 and 48 px favicons', () => {
        const ico = readFileSync(resolve(process.cwd(), 'public/favicon.ico'));

        expect(ico.readUInt16LE(0)).toBe(0); // reserved
        expect(ico.readUInt16LE(2)).toBe(1); // 1 = icon
        const count = ico.readUInt16LE(4);

        const sizes: number[] = [];
        for (let i = 0; i < count; i++) {
            const entry = 6 + i * 16;
            const size = ico.readUInt8(entry) || 256;
            const bytes = ico.readUInt32LE(entry + 8);
            const offset = ico.readUInt32LE(entry + 12);
            expect(offset + bytes).toBeLessThanOrEqual(ico.length);
            // Each image is a PNG of the size its directory entry claims.
            const png = ico.subarray(offset, offset + bytes);
            expect(png.subarray(0, 8)).toEqual(Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]));
            expect(png.readUInt32BE(16)).toBe(size);
            expect(png.readUInt32BE(20)).toBe(size);
            sizes.push(size);
        }
        expect(sizes).toEqual([16, 32, 48]);
    });
});
