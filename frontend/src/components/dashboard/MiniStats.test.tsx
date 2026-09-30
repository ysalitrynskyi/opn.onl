import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen } from '../../test/test-utils';
import MiniStats from './MiniStats';
import type { LinkData } from './types';

const DAY_MS = 1000 * 60 * 60 * 24;
const NOW0 = Date.UTC(2026, 3, 11);
const CREATED = new Date(NOW0 - 100 * DAY_MS).toISOString();

const link: LinkData = {
    id: 1,
    code: 'abc123',
    original_url: 'https://example.com',
    short_url: 'http://localhost:3000/abc123',
    title: null,
    click_count: 1000,
    created_at: CREATED,
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    tags: [],
};

describe('MiniStats', () => {
    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('keeps the displayed average stable across re-renders', () => {
        let now = NOW0;
        vi.spyOn(Date, 'now').mockImplementation(() => now);

        const { rerender } = render(<MiniStats link={link} />);
        const first = screen.getByText(/\/day avg/).textContent;

        now = NOW0 + 10 * DAY_MS;
        rerender(<MiniStats link={link} />);

        expect(screen.getByText(/\/day avg/).textContent).toBe(first);
        expect(first).toBe('10.0/day avg');
    });
});
