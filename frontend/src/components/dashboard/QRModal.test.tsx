import { useState } from 'react';
import { describe, expect, it, vi } from 'vitest';
import { render, screen } from '../../test/test-utils';
import QRModal from './QRModal';
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

describe('QRModal', () => {
    it('focuses the first control, traps Tab, and restores focus on close', async () => {
        function Harness() {
            const [open, setOpen] = useState(false);
            return (
                <>
                    <button type="button" onClick={() => setOpen(true)}>Open QR</button>
                    {open && (
                        <QRModal
                            link={baseLink}
                            onClose={() => setOpen(false)}
                            brandingEnabled={false}
                        />
                    )}
                </>
            );
        }

        const { user } = render(<Harness />);
        const opener = screen.getByRole('button', { name: 'Open QR' });
        await user.click(opener);

        const dialog = screen.getByRole('dialog');
        expect(dialog).toHaveAttribute('aria-modal', 'true');
        expect(dialog).toHaveAccessibleName('QR code');
        expect(screen.getByRole('button', { name: 'Close' })).toHaveFocus();

        for (let i = 0; i < 6; i++) {
            await user.tab();
            expect(dialog.contains(document.activeElement)).toBe(true);
            expect(opener).not.toHaveFocus();
        }

        await user.click(screen.getByRole('button', { name: 'Close' }));
        expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
        expect(opener).toHaveFocus();
    });
});
