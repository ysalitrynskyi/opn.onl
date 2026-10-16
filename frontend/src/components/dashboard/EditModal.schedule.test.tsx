import { act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../../test/test-utils';
import EditModal from './EditModal';
import type { LinkData } from './types';

const baseLink: LinkData = {
    id: 7,
    code: 'abc_123',
    original_url: 'https://example.com/original',
    short_url: 'https://opn.onl/abc_123',
    title: 'Keep me',
    click_count: 2,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: '2030-06-01T23:59:00Z',
    has_password: false,
    notes: 'Existing note',
    starts_at: '2030-05-01T15:00:00Z',
    max_clicks: 4,
    is_active: true,
    is_pinned: false,
    tags: [],
};

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.resolve(body),
    } as Response;
}

async function save(link: LinkData = baseLink, burnEnabled = false) {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const view = render(
        <EditModal link={link} onClose={vi.fn()} onSave={onSave} burnEnabled={burnEnabled} />,
    );
    return { ...view, onSave };
}

describe('EditModal schedule, notes and click limit', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
    });

    it('omits title, notes, the click limit and the start date when nothing changed', async () => {
        const { user, onSave } = await save();
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1];
        expect(payload).not.toHaveProperty('title');
        expect(payload).not.toHaveProperty('notes');
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
        expect(payload).not.toHaveProperty('starts_at');
        expect(payload).not.toHaveProperty('remove_starts_at');
        expect(payload).not.toHaveProperty('expires_at');
    });

    it('sends a changed title, notes, click limit and start date', async () => {
        const { user, onSave } = await save({ ...baseLink, title: null, notes: null, max_clicks: null, starts_at: null, expires_at: null });
        fireEvent.change(screen.getByLabelText('Title'), { target: { value: 'Launch' } });
        fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'Internal only' } });
        fireEvent.change(screen.getByLabelText('Click limit'), { target: { value: '12' } });
        fireEvent.change(screen.getByLabelText('Start date'), { target: { value: '2031-04-01T09:15' } });
        fireEvent.change(screen.getByLabelText(/^expiration date$/i), { target: { value: '2031-04-02' } });

        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1]).toMatchObject({
            title: 'Launch',
            notes: 'Internal only',
            max_clicks: 12,
            starts_at: new Date('2031-04-01T09:15').toISOString(),
            expires_at: new Date('2031-04-02T23:59:00').toISOString(),
        });
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('remove_max_clicks');
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('remove_starts_at');
    });

    it('clears a title and notes with an empty string', async () => {
        const { user, onSave } = await save();
        fireEvent.change(screen.getByLabelText('Title'), { target: { value: '' } });
        fireEvent.change(screen.getByLabelText('Notes'), { target: { value: '' } });
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1].title).toBe('');
        expect(onSave.mock.calls[0][1].notes).toBe('');
    });

    it('sends remove_max_clicks and remove_starts_at without the values they clear', async () => {
        const { user, onSave } = await save();
        await user.click(screen.getByRole('checkbox', { name: 'Remove limit' }));
        await user.click(screen.getByRole('checkbox', { name: 'Remove start date' }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1];
        expect(payload.remove_max_clicks).toBe(true);
        expect(payload.remove_starts_at).toBe(true);
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('starts_at');
    });

    it('refuses a start that is not before the expiration', async () => {
        const onSave = vi.fn();
        render(<EditModal link={{ ...baseLink, starts_at: null, expires_at: null }} onClose={vi.fn()} onSave={onSave} />);
        fireEvent.change(screen.getByLabelText('Start date'), { target: { value: '2030-01-16T00:00' } });
        fireEvent.change(screen.getByLabelText(/^expiration date$/i), { target: { value: '2030-01-15' } });
        fireEvent.click(screen.getByRole('button', { name: /save changes/i }));

        expect(await screen.findByRole('alert')).toHaveTextContent('Start date must be before the expiration');
        expect(onSave).not.toHaveBeenCalled();
    });

    it('does not send a click limit while burn after reading is on', async () => {
        const { user, onSave } = await save({ ...baseLink, burn_after_reading: false, max_clicks: 8 }, true);
        fireEvent.change(screen.getByLabelText('Click limit'), { target: { value: '3' } });
        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));

        const limit = screen.getByLabelText('Click limit');
        expect(limit).toBeDisabled();
        expect(limit).toHaveValue(8);
        expect(screen.getByText('Burn after reading stops the link after 8 clicks.')).toBeInTheDocument();
        expect(screen.getByRole('checkbox', { name: 'Remove limit' })).toBeDisabled();

        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1];
        expect(payload.burn_after_reading).toBe(true);
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
    });

    it('drops the implied cap from the body when burn is turned off', async () => {
        const { user, onSave } = await save({ ...baseLink, burn_after_reading: true, max_clicks: 1, expires_at: null, starts_at: null }, true);
        expect(screen.getByLabelText('Click limit')).toHaveValue(1);
        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
        expect(screen.getByLabelText('Click limit')).toHaveValue(null);
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        const payload = onSave.mock.calls[0][1];
        expect(payload.burn_after_reading).toBe(false);
        expect(payload).not.toHaveProperty('max_clicks');
        expect(payload).not.toHaveProperty('remove_max_clicks');
    });

    it('caps notes at 5000 characters and counts down near the limit', () => {
        render(<EditModal link={{ ...baseLink, notes: null }} onClose={vi.fn()} onSave={vi.fn()} />);
        const notes = screen.getByLabelText('Notes');
        fireEvent.change(notes, { target: { value: 'a'.repeat(4499) } });
        expect(screen.queryByText(/characters left/)).not.toBeInTheDocument();
        fireEvent.change(notes, { target: { value: 'b'.repeat(5001) } });
        expect(notes).toHaveValue('b'.repeat(5000));
        expect(screen.getByText('0 characters left')).toBeInTheDocument();
    });

    it('caps the title at 500 characters', () => {
        render(<EditModal link={{ ...baseLink, title: null }} onClose={vi.fn()} onSave={vi.fn()} />);
        const title = screen.getByLabelText('Title');
        fireEvent.change(title, { target: { value: 't'.repeat(501) } });
        expect(title).toHaveValue('t'.repeat(500));
    });

    describe('destination check', () => {
        it('checks the destination currently in the field', async () => {
            vi.mocked(fetch).mockResolvedValue(jsonResponse({
                url: 'https://example.com/next',
                reachable: true,
                status_code: 200,
                error: null,
            }));
            render(<EditModal link={baseLink} onClose={vi.fn()} onSave={vi.fn()} />);
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/next' } });
            fireEvent.click(screen.getByRole('button', { name: 'Check destination' }));

            expect(await screen.findByRole('status')).toHaveTextContent('Reachable · HTTP 200');
            const [endpoint, init] = vi.mocked(fetch).mock.calls[0];
            expect(String(endpoint)).toMatch(/\/links\/health-check$/);
            expect(init?.method).toBe('POST');
            expect(JSON.parse(String(init?.body))).toEqual({ url: 'https://example.com/next' });
        });

        it('shows a refused destination and ignores a second click while pending', async () => {
            let resolveHealth: ((value: Response) => void) | undefined;
            vi.mocked(fetch).mockImplementation(() => new Promise(resolve => {
                resolveHealth = resolve;
            }));
            render(<EditModal link={baseLink} onClose={vi.fn()} onSave={vi.fn()} />);
            const button = screen.getByRole('button', { name: 'Check destination' });
            fireEvent.click(button);
            fireEvent.click(button);
            expect(fetch).toHaveBeenCalledTimes(1);
            await waitFor(() => expect(button).toBeDisabled());

            await act(async () => {
                resolveHealth?.(jsonResponse({
                    url: baseLink.original_url,
                    reachable: false,
                    status_code: null,
                    error: 'Links to local/internal hosts are not allowed',
                }, 400));
            });
            expect(await screen.findByRole('status')).toHaveTextContent('Links to local/internal hosts are not allowed');
            expect(fetch).toHaveBeenCalledTimes(1);
        });
    });
});
