import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../../test/test-utils';
import EditModal from './EditModal';
import type { LinkData } from './types';
import type { CatalogItem } from './linkScope';

const folders: CatalogItem[] = [{ id: 4, name: 'Campaigns', color: '#2563eb', link_count: 1 }];
const tags: CatalogItem[] = [
    { id: 9, name: 'launch', color: '#e11d48', link_count: 1 },
    { id: 2, name: 'press', color: null, link_count: 0 },
];

const link: LinkData = {
    id: 11,
    code: 'abc123',
    original_url: 'https://example.com/long',
    short_url: 'http://localhost:3000/abc123',
    title: null,
    click_count: 1,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    folder_id: 4,
    tags: [{ id: 9, name: 'launch', color: '#e11d48' }],
};

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.resolve(body),
    } as Response;
}

function tagCalls(method: string) {
    return vi.mocked(fetch).mock.calls.filter(([input, init]) => {
        const url = String(input);
        return (init?.method ?? 'GET').toUpperCase() === method && url.includes('/links/11/tags');
    });
}

function renderModal(overrides: Partial<LinkData> = {}) {
    const onSave = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();
    const view = render(
        <EditModal
            link={{ ...link, ...overrides }}
            onClose={onClose}
            onSave={onSave}
            folders={folders}
            tags={tags}
        />,
    );
    return { ...view, onSave, onClose };
}

describe('EditModal folder and tags', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
        vi.mocked(fetch).mockResolvedValue(jsonResponse({ added: 1 }));
    });

    it('sends folder_id when the folder changes and leaves tags alone', async () => {
        const { user, onSave } = renderModal({ folder_id: null, tags: [] });
        await user.selectOptions(screen.getByLabelText('Folder'), '4');
        await user.click(screen.getByRole('button', { name: 'Save changes' }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1]).toMatchObject({ folder_id: 4 });
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('remove_folder');
        expect(tagCalls('POST')).toHaveLength(0);
        expect(tagCalls('DELETE')).toHaveLength(0);
    });

    it('clears a folder with remove_folder and does not send folder_id', async () => {
        const { user, onSave } = renderModal();
        await user.selectOptions(screen.getByLabelText('Folder'), '');
        await user.click(screen.getByRole('button', { name: 'Save changes' }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(onSave.mock.calls[0][1].remove_folder).toBe(true);
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('folder_id');
    });

    it('adds a tag with POST and removes one with DELETE', async () => {
        const { user, onSave, onClose } = renderModal();
        await user.click(screen.getByRole('checkbox', { name: 'press' }));
        await user.click(screen.getByRole('checkbox', { name: 'launch' }));
        await user.click(screen.getByRole('button', { name: 'Save changes' }));

        await waitFor(() => expect(onClose).toHaveBeenCalled());
        expect(onSave).toHaveBeenCalledOnce();
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('folder_id');
        expect(onSave.mock.calls[0][1]).not.toHaveProperty('remove_folder');
        expect(tagCalls('POST')).toHaveLength(1);
        expect(JSON.parse(String(tagCalls('POST')[0][1]?.body))).toEqual({ tag_ids: [2] });
        expect(tagCalls('DELETE')).toHaveLength(1);
        expect(JSON.parse(String(tagCalls('DELETE')[0][1]?.body))).toEqual({ tag_ids: [9] });
    });

    it('does not call the tag routes when the tags are unchanged', async () => {
        vi.mocked(fetch).mockImplementation(() => {
            throw new Error('no tag request');
        });
        const { user, onSave, onClose } = renderModal();
        await user.click(screen.getByRole('button', { name: 'Save changes' }));
        await waitFor(() => expect(onClose).toHaveBeenCalled());
        expect(onSave).toHaveBeenCalledOnce();
    });
});
