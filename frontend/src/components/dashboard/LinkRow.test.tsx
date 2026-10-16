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

function renderRow(overrides: Partial<LinkData> = {}, clickFlash = 0) {
    return render(
        <LinkRow
            link={{ ...base, ...overrides }}
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
        const { rerender } = renderRow({}, 1);
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
});
