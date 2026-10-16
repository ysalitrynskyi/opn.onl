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

// With the feature off there is no burn checkbox, so a link saved as burn
// could never get its cap changed if the cap stayed locked.
describe('EditModal click limit of a burn link with the feature switched off', () => {
    function renderOff(link: LinkData) {
        const onSave = vi.fn().mockResolvedValue(undefined);
        const view = render(<EditModal link={link} onClose={vi.fn()} onSave={onSave} />);
        return { ...view, onSave };
    }

    it('lets the cap be raised', async () => {
        const { user, onSave } = renderOff({ ...baseLink, burn_after_reading: true, max_clicks: 1 });
        const limit = screen.getByLabelText('Click limit');
        expect(limit).toBeEnabled();
        expect(limit).toHaveValue(1);
        expect(screen.getByText('Link stops working after this many clicks')).toBeInTheDocument();

        await user.clear(limit);
        await user.type(limit, '5');
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1] as Record<string, unknown>;
        expect(payload.max_clicks).toBe(5);
        expect(payload).not.toHaveProperty('remove_max_clicks');
        // The flag stays off the body while the feature is off.
        expect(payload.burn_after_reading).toBeUndefined();
    });

    it('lets the cap be removed', async () => {
        const { user, onSave } = renderOff({ ...baseLink, burn_after_reading: true, max_clicks: 3 });
        const remove = screen.getByRole('checkbox', { name: 'Remove limit' });
        expect(remove).toBeEnabled();

        await user.click(remove);
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1] as Record<string, unknown>;
        expect(payload.remove_max_clicks).toBe(true);
        expect(payload).not.toHaveProperty('max_clicks');
    });

    it('still leaves an untouched cap out of the save', async () => {
        const { user, onSave } = renderOff({ ...baseLink, burn_after_reading: true, max_clicks: 1 });
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1] as Record<string, unknown>;
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
    });

    it('stays locked while the feature is on and burn is ticked', () => {
        render(
            <EditModal
                link={{ ...baseLink, burn_after_reading: true, max_clicks: 3 }}
                onClose={vi.fn()}
                onSave={vi.fn()}
                burnEnabled
            />,
        );
        expect(screen.getByLabelText('Click limit')).toBeDisabled();
        expect(screen.getByRole('checkbox', { name: 'Remove limit' })).toBeDisabled();
        expect(screen.getByText('Burn after reading stops the link after 3 clicks.')).toBeInTheDocument();
    });
});
