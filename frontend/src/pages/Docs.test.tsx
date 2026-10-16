import { readFileSync, existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Docs from './Docs';

const composePath = resolve(process.cwd(), '../docker-compose.yml');

// The API reference is typed by hand. These pin the parts that were wrong:
// they read the backend source where a value lives there, so a change on one
// side shows up as a failure on the other.
const linksRs = resolve(process.cwd(), '../backend/src/handlers/links.rs');
const hasBackendSource = existsSync(linksRs);
const backend = hasBackendSource ? readFileSync(linksRs, 'utf8') : '';

describe('API reference', () => {
    it('says folders and tags are used through the API, and teams are in the app', () => {
        // Folders and tags still have no screens. Teams are managed at /organizations.
        render(<Docs />);
        for (const title of ['Folders', 'Tags']) {
            const card = screen.getByRole('heading', { level: 4, name: title }).parentElement;
            expect(card, `${title} card`).toHaveTextContent(/via the API/i);
        }
        const teams = screen.getByRole('heading', { level: 4, name: 'Teams' }).parentElement;
        expect(teams).toHaveTextContent(/in the app/i);
        expect(teams).not.toHaveTextContent(/via the API/i);
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

    it('lists the variables Compose requires and the hosts the operator must set', () => {
        const compose = readFileSync(composePath, 'utf8');
        const required = [
            ...new Set([...compose.matchAll(/\$\{([A-Z0-9_]+):\?/g)].map((match) => match[1])),
        ].sort();
        expect(required).toEqual(['JWT_SECRET', 'POSTGRES_PASSWORD']);

        render(<Docs />);
        const table = screen.getByRole('table');
        expect(table).not.toHaveTextContent('DATABASE_URL');
        for (const name of required) {
            const row = screen
                .getAllByText(name)
                .map((element) => element.closest('tr'))
                .find((element) => element !== null);
            expect(row, name).toHaveTextContent('Required');
        }
        expect(table).toHaveTextContent('POSTGRES_USER');
        expect(table).toHaveTextContent('postgres');
        expect(table).toHaveTextContent('opn_onl');
        expect(table).toHaveTextContent('https://opn.onl');

        const notice = screen.getByText(/you must set/i, { selector: 'p' });
        expect(notice).toHaveTextContent('BASE_URL');
        expect(notice).toHaveTextContent('FRONTEND_URL');
        expect(notice).toHaveTextContent('https://opn.onl');
        expect(notice).toHaveTextContent('POSTGRES_HOST');
        expect(notice).toHaveTextContent('5432');
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
