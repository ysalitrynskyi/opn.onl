import { useState } from 'react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../../test/test-utils';
import EditModal from './EditModal';
import type { LinkData } from './types';

const baseLink: LinkData = {
    id: 1,
    code: 'abc_123',
    original_url: 'https://example.com/original',
    short_url: 'https://opn.onl/abc_123',
    title: null,
    click_count: 0,
    created_at: '2026-01-01 00:00:00',
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    tags: [],
};

const okResponse = (body: unknown = []) => ({
    ok: true,
    status: 200,
    json: () => Promise.resolve(body),
});

describe('EditModal', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it.each([
        '2030-05-17 12:34:56',
        '2030-05-17T12:34:56Z',
    ])('loads API expiration format %s into the date field', (expiresAt) => {
        render(
            <EditModal
                link={{ ...baseLink, expires_at: expiresAt }}
                onClose={vi.fn()}
                onSave={vi.fn()}
            />
        );

        expect(screen.getByLabelText(/^expiration date$/i)).toHaveValue('2030-05-17');
    });

    it('preserves the original expiration value when the date is unchanged', async () => {
        const onClose = vi.fn();
        const onSave = vi.fn().mockResolvedValue(undefined);
        const { user } = render(
            <EditModal
                link={{ ...baseLink, expires_at: '2030-05-17 12:34:56' }}
                onClose={onClose}
                onSave={onSave}
            />
        );

        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('expires_at');
        expect(onSave.mock.calls[0][1].remove_expiration).toBeUndefined();
        expect(onClose).toHaveBeenCalledOnce();
    });

    it('serializes an expiration only after the user changes its date', async () => {
        const onSave = vi.fn().mockResolvedValue(undefined);
        const { user } = render(
            <EditModal
                link={{ ...baseLink, expires_at: '2030-05-17T12:34:56Z' }}
                onClose={vi.fn()}
                onSave={onSave}
            />
        );

        fireEvent.change(screen.getByLabelText(/^expiration date$/i), {
            target: { value: '2031-06-20' },
        });
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        // Date-only expiry is end of that local day, matching the create
        // form's 23:59 default. `new Date('YYYY-MM-DD')` is UTC midnight,
        // which is already yesterday west of UTC.
        expect(onSave.mock.calls[0][1].expires_at).toBe(
            new Date('2031-06-20T23:59:00').toISOString(),
        );
    });

    it('uses the local calendar date as the expiration minimum, not UTC', () => {
        const year = vi.spyOn(Date.prototype, 'getFullYear').mockReturnValue(2026);
        const month = vi.spyOn(Date.prototype, 'getMonth').mockReturnValue(8);
        const day = vi.spyOn(Date.prototype, 'getDate').mockReturnValue(17);
        const iso = vi.spyOn(Date.prototype, 'toISOString').mockReturnValue('2026-09-18T03:00:00.000Z');

        try {
            render(
                <EditModal
                    link={baseLink}
                    onClose={vi.fn()}
                    onSave={vi.fn()}
                />
            );

            expect(screen.getByLabelText(/^expiration date$/i)).toHaveAttribute('min', '2026-09-17');
        } finally {
            year.mockRestore();
            month.mockRestore();
            day.mockRestore();
            iso.mockRestore();
        }
    });

    it('keeps the modal open and skips routing when the link save fails', async () => {
        vi.mocked(global.fetch).mockResolvedValue(okResponse() as Response);
        const onClose = vi.fn();
        const onSave = vi.fn().mockRejectedValue(new Error('Link update rejected'));
        const { user } = render(
            <EditModal
                link={baseLink}
                onClose={onClose}
                onSave={onSave}
                routingEnabled
            />
        );

        await user.click(screen.getByRole('button', { name: /save changes/i }));

        expect(await screen.findByRole('alert')).toHaveTextContent('Link update rejected');
        expect(onClose).not.toHaveBeenCalled();
        expect(vi.mocked(global.fetch).mock.calls.some(([, options]) => options?.method === 'PUT')).toBe(false);
    });

    it('keeps the modal open when routing fails after a successful link save', async () => {
        vi.mocked(global.fetch).mockImplementation((_url, options) => {
            if (options?.method === 'PUT') {
                return Promise.resolve({
                    ok: false,
                    status: 422,
                    json: () => Promise.resolve({ error: 'Routing update rejected' }),
                } as Response);
            }
            return Promise.resolve(okResponse() as Response);
        });
        const onClose = vi.fn();
        const onSave = vi.fn().mockResolvedValue(undefined);
        const { user } = render(
            <EditModal
                link={baseLink}
                onClose={onClose}
                onSave={onSave}
                routingEnabled
            />
        );

        await user.click(screen.getByRole('button', { name: /save changes/i }));

        expect(await screen.findByRole('alert')).toHaveTextContent('Routing update rejected');
        expect(onSave).toHaveBeenCalledOnce();
        expect(onClose).not.toHaveBeenCalled();
    });

    it('focuses the first control, traps Tab, and restores focus on close', async () => {
        function Harness() {
            const [open, setOpen] = useState(false);
            return (
                <>
                    <button type="button" onClick={() => setOpen(true)}>Open editor</button>
                    {open && (
                        <EditModal
                            link={baseLink}
                            onClose={() => setOpen(false)}
                            onSave={vi.fn()}
                        />
                    )}
                </>
            );
        }

        const { user } = render(<Harness />);
        const opener = screen.getByRole('button', { name: 'Open editor' });
        await user.click(opener);

        const dialog = screen.getByRole('dialog');
        expect(dialog).toHaveAttribute('aria-modal', 'true');
        expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();

        for (let i = 0; i < 12; i++) {
            await user.tab();
            expect(dialog.contains(document.activeElement)).toBe(true);
            expect(opener).not.toHaveFocus();
        }

        screen.getByRole('button', { name: 'Close' }).focus();
        await user.tab({ shift: true });
        expect(dialog.contains(document.activeElement)).toBe(true);
        expect(document.activeElement).not.toBe(screen.getByRole('button', { name: 'Close' }));

        await user.click(screen.getByRole('button', { name: 'Close' }));
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        expect(opener).toHaveFocus();
    });
});
