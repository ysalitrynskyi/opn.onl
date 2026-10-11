import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Developers from './Developers';

// The curl in the "REST in 60 seconds" block is copied and run as written, so
// what it sends has to be what the API reads.
describe('Developers quickstart', () => {
    function quickstart(): string {
        render(<Developers />);
        const block = [...document.querySelectorAll('pre')].find((pre) =>
            pre.textContent?.includes('OPN_API_KEY'),
        );
        expect(block, 'the quickstart block is on the page').toBeDefined();
        return block!.textContent ?? '';
    }

    it('names the alias field the way the API does', () => {
        const body = quickstart().match(/-d '(\{[\s\S]*?\})'/)?.[1];
        expect(body, 'the curl has a JSON body').toBeDefined();

        const sent = JSON.parse(body!);
        expect(sent.original_url).toMatch(/^https:\/\//);
        // CreateLinkRequest reads `custom_alias`; any other key is dropped
        // silently and the caller gets a random code.
        expect(sent.custom_alias).toBe('launch');
        expect(sent).not.toHaveProperty('custom_code');
    });

    it('shows the short link on the address the API returns it on', () => {
        expect(quickstart()).toContain('"short_url": "https://opn.onl/launch"');
    });

    it('lists the endpoint that fetches one link', () => {
        render(<Developers />);
        expect(screen.getByText(/fetch one link/i)).toBeInTheDocument();
    });
});
