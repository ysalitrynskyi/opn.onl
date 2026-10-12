import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Docs from './Docs';

// The API reference is typed by hand. These pin the parts that were wrong:
// they read the backend source where a value lives there, so a change on one
// side shows up as a failure on the other.
const linksRs = resolve(process.cwd(), '../backend/src/handlers/links.rs');
const hasBackendSource = existsSync(linksRs);
const backend = hasBackendSource ? readFileSync(linksRs, 'utf8') : '';

describe('API reference', () => {
    it('says the folders, tags and teams features are used through the API, which is all there is', () => {
        // The dashboard has no folder, tag or team screens; claiming the
        // features without saying so promised an interface that is not there.
        render(<Docs />);
        for (const title of ['Folders', 'Tags', 'Teams']) {
            const card = screen.getByRole('heading', { level: 4, name: title }).parentElement;
            expect(card, `${title} card`).toHaveTextContent(/via the API/i);
        }
        // The features that do have an interface are not marked.
        expect(screen.getByRole('heading', { level: 4, name: 'Custom Aliases' }).parentElement).not.toHaveTextContent(/api/i);
    });

    it('documents the endpoint that fetches one link', () => {
        render(<Docs />);
        const row = screen.getByText('Get one link, in the same shape as an item of the list');
        expect(row.parentElement).toHaveTextContent('GET');
        expect(row.parentElement).toHaveTextContent('/links/{id}');
    });

    it('shows a request example whose expiry is still ahead', () => {
        const { container } = render(<Docs />);
        const request = [...container.querySelectorAll('code')].find((c) =>
            c.textContent?.includes('"custom_alias"'),
        );
        const expiry = request?.textContent?.match(/"expires_at":\s*"([^"]+)"/)?.[1];
        expect(expiry, 'the create example sets expires_at').toBeDefined();
        // Copying a past expiry creates a link that is dead on arrival.
        expect(new Date(expiry!).getTime()).toBeGreaterThan(Date.now());
    });

    it.skipIf(!hasBackendSource)('states the list limit the backend enforces', () => {
        const limit = backend.match(/const DEFAULT_LINKS_LIMIT: u64 = (\d+);/)?.[1];
        const max = backend.match(/const MAX_LINKS_LIMIT: u64 = (\d+);/)?.[1];
        expect(limit).toBeDefined();
        expect(max).toBe(limit);

        render(<Docs />);
        expect(screen.getByText(/default and maximum: /i)).toHaveTextContent(
            `default and maximum: ${limit}`,
        );
        expect(screen.queryByText(/default: 50\)/)).not.toBeInTheDocument();
        expect(screen.getByText('X-Total-Count')).toBeInTheDocument();
    });

    it.skipIf(!hasBackendSource)('names every remove_* flag a link update takes', () => {
        const update = backend.match(/pub struct UpdateLinkRequest \{([\s\S]*?)\n\}/)?.[1] ?? '';
        const flags = [...update.matchAll(/pub (remove_\w+):/g)].map((m) => m[1]);
        expect(flags.length).toBeGreaterThan(0);

        const { container } = render(<Docs />);
        const documented = [...container.querySelectorAll('code')].map((c) => c.textContent);
        for (const flag of flags) {
            expect(documented, `${flag} is documented`).toContain(flag);
        }
    });
});
