import { describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../../test/test-utils';
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

async function saveWith(link: LinkData, toggleBurn: boolean) {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const { user } = render(
        <EditModal link={link} onClose={vi.fn()} onSave={onSave} burnEnabled />,
    );
    if (toggleBurn) {
        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
    }
    await user.click(screen.getByRole('button', { name: /save changes/i }));
    await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
    return onSave.mock.calls[0][1] as Record<string, unknown>;
}

// The click limit is on the form, but an untouched limit is not sent. A burn
// link's implied cap is removed by the server when the flag goes off, so
// un-ticking must not put that cap back into the body.
describe('EditModal burn after reading', () => {
    it('sends burn_after_reading false when a burn link is un-ticked', async () => {
        const payload = await saveWith(
            { ...baseLink, burn_after_reading: true, burned_at: null },
            true,
        );
        expect(payload.burn_after_reading).toBe(false);
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
    });

    it('sends burn_after_reading true when a link is ticked', async () => {
        const payload = await saveWith({ ...baseLink, burn_after_reading: false }, true);
        expect(payload.burn_after_reading).toBe(true);
        expect(payload).not.toHaveProperty('max_clicks');
    });

    it('keeps sending the flag a burn link was opened with when it is left alone', async () => {
        const payload = await saveWith({ ...baseLink, burn_after_reading: true }, false);
        expect(payload.burn_after_reading).toBe(true);
    });

    // Saving the form must not read as "un-burn" to the server, or every save of
    // a capped link that is not a burn link would drop its cap.
    it('keeps sending false for a link that was never burn when it is left alone', async () => {
        const payload = await saveWith({ ...baseLink, burn_after_reading: false }, false);
        expect(payload.burn_after_reading).toBe(false);
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
    });

    it('leaves the flag out when the instance has burn after reading switched off', async () => {
        const onSave = vi.fn().mockResolvedValue(undefined);
        const { user } = render(
            <EditModal
                link={{ ...baseLink, burn_after_reading: true }}
                onClose={vi.fn()}
                onSave={onSave}
            />,
        );
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1].burn_after_reading).toBeUndefined();
    });
});
