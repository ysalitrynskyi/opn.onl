import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen } from '../../test/test-utils';
import LinkRow from './LinkRow';
import { INACTIVE_REASON } from './linkStatus';
import type { LinkData } from './types';

const base: LinkData = {
    id: 7,
    code: 'abc123',
    original_url: 'https://example.com/long',
    short_url: 'http://localhost:3000/abc123',
    title: null,
    click_count: 3,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    tags: [],
};

function row(link: LinkData, extra: { readOnly?: boolean } = {}) {
    return (
        <LinkRow
            link={link}
            linkNumber={1}
            copied={false}
            sourceCopied={false}
            cloning={false}
            selected={false}
            onToggleSelect={vi.fn()}
            onCopy={vi.fn()}
            onCopySource={vi.fn()}
            onPreview={vi.fn()}
            onPin={vi.fn()}
            onClone={vi.fn()}
            onShare={vi.fn()}
            onQr={vi.fn()}
            onEdit={vi.fn()}
            onDelete={vi.fn()}
            {...extra}
        />
    );
}

function renderRow(
    overrides: Partial<LinkData> = {},
    folder: { name: string; color?: string | null } | null = null,
    clickFlash = 0,
) {
    return render(
        <LinkRow
            link={{ ...base, ...overrides }}
            folder={folder}
            linkNumber={1}
            copied={false}
            sourceCopied={false}
            cloning={false}
            selected={false}
            clickFlash={clickFlash}
            onToggleSelect={vi.fn()}
            onCopy={vi.fn()}
            onCopySource={vi.fn()}
            onPreview={vi.fn()}
            onPin={vi.fn()}
            onClone={vi.fn()}
            onShare={vi.fn()}
            onQr={vi.fn()}
            onEdit={vi.fn()}
            onDelete={vi.fn()}
        />,
    );
}

describe('LinkRow status badges', () => {
    beforeEach(() => {
        vi.useFakeTimers({ toFake: ['Date'] });
        vi.setSystemTime(new Date(2026, 9, 1, 12, 0, 0));
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it('labels the row checkbox with the short code', () => {
        renderRow();
        expect(screen.getByRole('checkbox', { name: 'Select link abc123' })).not.toBeChecked();
        expect(screen.queryByText('Inactive')).not.toBeInTheDocument();
        expect(screen.queryByText('Protected')).not.toBeInTheDocument();
    });

    it('shows a click limit and a used-up style once the cap is reached', () => {
        const { rerender } = renderRow({ click_count: 3, max_clicks: 10 });
        expect(screen.getByText('3 / 10 clicks')).not.toHaveClass('text-danger');
        expect(screen.queryByText('Used up')).not.toBeInTheDocument();

        rerender(
            <LinkRow
                link={{ ...base, click_count: 10, max_clicks: 10, is_active: false }}
                linkNumber={1}
                copied={false}
                sourceCopied={false}
                cloning={false}
                selected={false}
                onToggleSelect={vi.fn()}
                onCopy={vi.fn()}
                onCopySource={vi.fn()}
                onPreview={vi.fn()}
                onPin={vi.fn()}
                onClone={vi.fn()}
                onShare={vi.fn()}
                onQr={vi.fn()}
                onEdit={vi.fn()}
                onDelete={vi.fn()}
            />,
        );
        expect(screen.getByText('10 / 10 clicks')).toHaveClass('text-danger');
        expect(screen.getByText('Used up')).toHaveAttribute('title', INACTIVE_REASON.usedUp);
    });

    it('uses the singular when the cap is one click', () => {
        renderRow({ click_count: 0, max_clicks: 1 });
        expect(screen.getByText('0 / 1 click')).toBeInTheDocument();
    });

    it('shows when a link is scheduled to start, on the local calendar day', () => {
        const starts = new Date(2026, 9, 12, 12, 0, 0).toISOString();
        renderRow({ starts_at: starts, is_active: false });
        expect(screen.getByText('Starts Oct 12')).toBeInTheDocument();
        expect(screen.getByText('Not started')).toHaveAttribute('title', INACTIVE_REASON.notStarted);
        expect(screen.queryByText('Inactive')).not.toBeInTheDocument();
    });

    it('does not call a link scheduled once its start time has arrived', () => {
        const starts = new Date(2026, 9, 1, 12, 0, 0).toISOString();
        renderRow({ starts_at: starts });
        expect(screen.queryByText('Not started')).not.toBeInTheDocument();
        expect(screen.queryByText(/Starts /)).not.toBeInTheDocument();
    });

    it('flashes the count after a live click and leaves it alone otherwise', () => {
        const { rerender } = renderRow({}, null, 1);
        const flashed = screen.getByRole('link', { name: '3 clicks, analytics for abc123' });
        expect(flashed.querySelector('.animate-live-click')).not.toBeNull();
        rerender(
            <LinkRow
                link={base}
                linkNumber={1}
                copied={false}
                sourceCopied={false}
                cloning={false}
                selected={false}
                clickFlash={0}
                onToggleSelect={vi.fn()}
                onCopy={vi.fn()}
                onCopySource={vi.fn()}
                onPreview={vi.fn()}
                onPin={vi.fn()}
                onClone={vi.fn()}
                onShare={vi.fn()}
                onQr={vi.fn()}
                onEdit={vi.fn()}
                onDelete={vi.fn()}
            />,
        );
        expect(
            screen.getByRole('link', { name: '3 clicks, analytics for abc123' }).querySelector('.animate-live-click'),
        ).toBeNull();
    });

    it('flags an expiry that has passed and still shows the expiry date', () => {
        const expires = new Date(2026, 8, 20, 12, 0, 0).toISOString();
        renderRow({ expires_at: expires, is_active: false, has_password: true });
        expect(screen.getByText('Expired')).toHaveAttribute('title', INACTIVE_REASON.expired);
        expect(screen.getByText(new Date(expires).toLocaleDateString())).toBeInTheDocument();
        expect(screen.getByText('Protected')).toBeInTheDocument();
        expect(screen.queryByText('Inactive')).not.toBeInTheDocument();
    });

    it('keeps a future expiry as a date, not as expired', () => {
        const expires = new Date(2026, 9, 1, 12, 0, 0).toISOString();
        renderRow({ expires_at: expires });
        expect(screen.queryByText('Expired')).not.toBeInTheDocument();
        expect(screen.getByText(new Date(expires).toLocaleDateString())).toBeInTheDocument();
    });

    it('lets a one-time link say it was opened, ahead of the generic cap', () => {
        renderRow({
            click_count: 1,
            max_clicks: 1,
            burn_after_reading: true,
            is_active: false,
        });
        expect(screen.getByText('Used up')).toHaveAttribute('title', INACTIVE_REASON.usedUpOnce);
    });

    it('treats burned_at as used up even when the cap is not reached', () => {
        renderRow({
            click_count: 0,
            max_clicks: 5,
            burn_after_reading: true,
            burned_at: '2026-09-01T12:00:00Z',
            is_active: false,
        });
        expect(screen.getByText('Used up')).toHaveAttribute('title', INACTIVE_REASON.usedUpOnce);
        expect(screen.getByText('0 / 5 clicks')).not.toHaveClass('text-danger');
    });

    it('prefers not started over expired, the same order as the server', () => {
        renderRow({
            starts_at: new Date(2026, 9, 12, 12, 0, 0).toISOString(),
            expires_at: new Date(2026, 8, 1, 12, 0, 0).toISOString(),
            is_active: false,
        });
        expect(screen.getByText('Not started')).toBeInTheDocument();
        expect(screen.getByText('Starts Oct 12')).toBeInTheDocument();
        expect(screen.queryByText('Expired')).not.toBeInTheDocument();
    });

    it('stays on Inactive when the list has no field that explains it', () => {
        renderRow({ is_active: false });
        expect(screen.getByText('Inactive')).toBeInTheDocument();
        expect(screen.queryByText('Blocked')).not.toBeInTheDocument();
        expect(screen.queryByText('Disabled')).not.toBeInTheDocument();
    });

    // The row is keyed by id, so a reload of the list re-renders the same
    // instance. The clock must be the one at that render, not at mount.
    it('drops Not started once a reload arrives after the start time', () => {
        const starts = new Date(2026, 9, 1, 12, 1, 0).toISOString();
        const { rerender } = render(row({ ...base, starts_at: starts, is_active: false }));
        expect(screen.getByText('Not started')).toBeInTheDocument();
        expect(screen.getByText('Starts Oct 1')).toBeInTheDocument();

        vi.setSystemTime(new Date(2026, 9, 1, 12, 2, 0));
        rerender(row({ ...base, starts_at: starts, is_active: true }));

        expect(screen.queryByText('Not started')).not.toBeInTheDocument();
        expect(screen.queryByText(/^Starts /)).not.toBeInTheDocument();
        expect(screen.queryByText('Inactive')).not.toBeInTheDocument();
    });

    it('does not fall back to Inactive when only the start has passed since the list was read', () => {
        const starts = new Date(2026, 9, 1, 12, 1, 0).toISOString();
        const link = { ...base, starts_at: starts, is_active: false };
        const { rerender } = render(row(link));
        expect(screen.getByText('Not started')).toBeInTheDocument();

        // Any re-render, a search keystroke say, with the list still as it was read.
        vi.setSystemTime(new Date(2026, 9, 1, 12, 2, 0));
        rerender(row(link));

        expect(screen.queryByText('Not started')).not.toBeInTheDocument();
        expect(screen.queryByText('Inactive')).not.toBeInTheDocument();
    });

    it('marks a link expired once its expiry passes, without a remount', () => {
        const expires = new Date(2026, 9, 1, 12, 1, 0).toISOString();
        const link = { ...base, expires_at: expires };
        const { rerender } = render(row(link));
        expect(screen.queryByText('Expired')).not.toBeInTheDocument();

        vi.setSystemTime(new Date(2026, 9, 1, 12, 2, 0));
        rerender(row(link));

        expect(screen.getByText('Expired')).toHaveAttribute('title', INACTIVE_REASON.expired);
    });
});

describe('LinkRow for a team viewer', () => {
    it('offers only what a viewer may do', () => {
        render(row({ ...base, is_pinned: true }, { readOnly: true }));
        expect(screen.queryByRole('checkbox')).not.toBeInTheDocument();
        for (const name of ['Unpin link abc123', 'Clone link abc123', 'Edit link abc123', 'Delete link abc123']) {
            expect(screen.queryByRole('button', { name })).not.toBeInTheDocument();
        }
        expect(screen.getByLabelText('Pinned')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Copy short URL for abc123' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Preview destination for abc123' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Share link abc123' })).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Show QR code for abc123' })).toBeInTheDocument();
    });

    it('keeps every action for anyone else', () => {
        render(row(base));
        expect(screen.getByRole('checkbox', { name: 'Select link abc123' })).toBeInTheDocument();
        for (const name of ['Pin link abc123', 'Clone link abc123', 'Edit link abc123', 'Delete link abc123']) {
            expect(screen.getByRole('button', { name })).toBeInTheDocument();
        }
    });
});

describe('LinkRow folder and tags', () => {
    it('shows the folder and its tags, and a colour dot only for a hex colour', () => {
        renderRow(
            {
                tags: [
                    { id: 1, name: 'launch', color: '#e11d48' },
                    { id: 2, name: 'later', color: 'red' },
                ],
            },
            { name: 'Campaigns', color: '#2563eb' },
        );
        const launch = screen.getByText('launch');
        expect(launch.querySelector('[aria-hidden="true"]')).toHaveStyle({ backgroundColor: '#e11d48' });
        expect(screen.getByText('later').querySelector('[aria-hidden="true"]')).toBeNull();
        expect(screen.getByText('Campaigns').querySelector('[aria-hidden="true"]')).toHaveStyle({
            backgroundColor: '#2563eb',
        });
    });

    it('shows two tags and a count of the rest', () => {
        renderRow({
            tags: Array.from({ length: 20 }, (_, i) => ({ id: i + 1, name: `tag-${String(i).padStart(2, '0')}`, color: null })),
        });
        expect(screen.getAllByText(/^tag-\d{2}$/)).toHaveLength(2);
        expect(screen.getByText('+18')).toHaveAttribute('title', expect.stringContaining('tag-19'));
    });
});
