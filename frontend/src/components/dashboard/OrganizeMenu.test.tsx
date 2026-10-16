import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../../test/test-utils';
import OrganizeMenu from './OrganizeMenu';
import type { CatalogItem } from './linkScope';

const campaign: CatalogItem = { id: 7, name: 'Campaigns', color: '#2563eb', link_count: 2 };
const launch: CatalogItem = { id: 9, name: 'launch', color: '#e11d48', link_count: 1 };

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.resolve(body),
    } as Response;
}

function noBody(status = 204): Response {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.reject(new Error('no body')),
    } as Response;
}

function posts(method: string, path: string) {
    return vi.mocked(fetch).mock.calls.filter(([input, init]) => {
        const url = String(input);
        return (init?.method ?? 'GET').toUpperCase() === method && url.split('?')[0].endsWith(path);
    });
}

function renderMenu(orgId: number | null = null) {
    const onChanged = vi.fn();
    const onFolderDeleted = vi.fn();
    const onTagDeleted = vi.fn();
    const view = render(
        <OrganizeMenu
            orgId={orgId}
            folders={[campaign]}
            tags={[launch]}
            onChanged={onChanged}
            onFolderDeleted={onFolderDeleted}
            onTagDeleted={onTagDeleted}
        />,
    );
    return { ...view, onChanged, onFolderDeleted, onTagDeleted };
}

describe('OrganizeMenu', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
        vi.mocked(fetch).mockResolvedValue(jsonResponse({ id: 1 }));
    });

    it('creates a folder and a tag in the open team', async () => {
        const { user, onChanged } = renderMenu(5);
        await user.click(screen.getByRole('button', { name: 'Manage' }));
        fireEvent.change(screen.getByRole('textbox', { name: 'New folder name' }), { target: { value: 'Press' } });
        await user.click(screen.getByRole('button', { name: 'Add folder' }));

        await waitFor(() => expect(posts('POST', '/folders')).toHaveLength(1));
        expect(JSON.parse(String(posts('POST', '/folders')[0][1]?.body))).toEqual({
            name: 'Press',
            color: '#2563eb',
            org_id: 5,
        });
        // Only the list that changed is reloaded.
        expect(onChanged).toHaveBeenCalledWith('folder');
        expect(onChanged).not.toHaveBeenCalledWith('tag');

        fireEvent.change(screen.getByRole('textbox', { name: 'New tag name' }), { target: { value: 'q3' } });
        await user.click(screen.getByRole('button', { name: 'Add tag' }));
        await waitFor(() => expect(posts('POST', '/tags')).toHaveLength(1));
        expect(JSON.parse(String(posts('POST', '/tags')[0][1]?.body))).toMatchObject({ name: 'q3', org_id: 5 });
        await waitFor(() => expect(onChanged).toHaveBeenCalledWith('tag'));
    });

    it('renames a folder with its colour', async () => {
        const { user, onChanged } = renderMenu();
        await user.click(screen.getByRole('button', { name: 'Manage' }));
        await user.click(screen.getByRole('button', { name: 'Rename folder Campaigns' }));
        fireEvent.change(screen.getByRole('textbox', { name: 'Folder name' }), { target: { value: 'Archive' } });
        await user.click(screen.getByRole('button', { name: 'Save folder' }));

        await waitFor(() => expect(posts('PUT', '/folders/7')).toHaveLength(1));
        expect(JSON.parse(String(posts('PUT', '/folders/7')[0][1]?.body))).toEqual({
            name: 'Archive',
            color: '#2563eb',
        });
        expect(onChanged).toHaveBeenCalledWith('folder');
    });

    it('confirms that deleting a folder leaves its links, then deletes it', async () => {
        vi.mocked(fetch).mockResolvedValue(noBody());
        const { user, onFolderDeleted, onChanged } = renderMenu();
        await user.click(screen.getByRole('button', { name: 'Manage' }));
        await user.click(screen.getByRole('button', { name: 'Delete folder Campaigns' }));
        expect(screen.getByText(/2 links stay, but they leave this folder/)).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: 'Delete folder' }));

        await waitFor(() => expect(posts('DELETE', '/folders/7')).toHaveLength(1));
        expect(onFolderDeleted).toHaveBeenCalledWith(7);
        expect(onChanged).not.toHaveBeenCalled();
    });

    it('says a deleted tag comes off its links', async () => {
        vi.mocked(fetch).mockResolvedValue(noBody());
        const { user, onTagDeleted } = renderMenu();
        await user.click(screen.getByRole('button', { name: 'Manage' }));
        await user.click(screen.getByRole('button', { name: 'Delete tag launch' }));
        expect(screen.getByText(/It comes off 1 link\. The link stays/)).toBeInTheDocument();
        await user.click(screen.getByRole('button', { name: 'Delete tag' }));
        await waitFor(() => expect(onTagDeleted).toHaveBeenCalledWith(9));
    });

    it('shows the API error and does not refresh', async () => {
        vi.mocked(fetch).mockResolvedValue(jsonResponse({ error: 'Name is too long' }, 400));
        const { user, onChanged } = renderMenu();
        await user.click(screen.getByRole('button', { name: 'Manage' }));
        fireEvent.change(screen.getByRole('textbox', { name: 'New folder name' }), { target: { value: 'Press' } });
        await user.click(screen.getByRole('button', { name: 'Add folder' }));
        expect(await screen.findByRole('alert')).toHaveTextContent('Name is too long');
        expect(onChanged).not.toHaveBeenCalled();
    });
});
