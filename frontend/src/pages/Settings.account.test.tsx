import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, mockToken, render, screen, waitFor, within } from '../test/test-utils';
import Settings from './Settings';

// Stable navigate identity: Settings' load effect depends on it.
const mockNavigate = vi.fn();
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return { ...actual, useNavigate: () => mockNavigate };
});

const profile = {
    id: 1,
    email: 'test@example.com',
    email_verified: true,
    is_admin: false,
    created_at: '2024-01-01T00:00:00Z',
    link_count: 42,
    total_clicks: 1234,
    display_name: 'Test User',
    bio: 'Software developer',
    website: 'https://example.com',
    avatar_url: null,
    location: 'San Francisco, CA',
    bio_username: null,
    bio_enabled: false,
    bio_theme: null,
};

const appSettings = {
    account_deletion_enabled: true,
    custom_aliases_enabled: true,
    passkeys_enabled: true,
    api_keys_enabled: true,
    link_in_bio_enabled: true,
    min_alias_length: 5,
    max_alias_length: 50,
};

const passkey = { id: 7, name: 'MacBook Pro', created_at: '2024-01-01T00:00:00Z', last_used: null };

interface Call {
    url: string;
    method: string;
    body: unknown;
    authorization: string | undefined;
}

function reply(body: unknown, status = 200, headers: Record<string, string> = {}) {
    const lower = Object.fromEntries(Object.entries(headers).map(([key, value]) => [key.toLowerCase(), value]));
    return Promise.resolve({
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.resolve(body),
        headers: { get: (name: string) => lower[name.toLowerCase()] ?? null },
    });
}

/**
 * A fake API for the Settings page. `override` answers first; whatever it
 * leaves undefined falls through to the account's default data. Returns every
 * request made, in order.
 */
function installApi(
    override: (call: Call) => Promise<unknown> | undefined = () => undefined,
    account: Partial<typeof profile> = {},
): Call[] {
    const calls: Call[] = [];
    global.fetch = vi.fn((url: string, init?: RequestInit) => {
        const headers = (init?.headers ?? {}) as Record<string, string>;
        const call: Call = {
            url: String(url),
            method: init?.method ?? 'GET',
            body: init?.body ? JSON.parse(String(init.body)) : undefined,
            authorization: headers.Authorization,
        };
        calls.push(call);
        const custom = override(call);
        if (custom) return custom;
        if (call.url.includes('/auth/me')) return reply({ ...profile, ...account });
        if (call.url.includes('/auth/settings')) return reply(appSettings);
        if (call.url.includes('/auth/passkeys')) return reply({ passkeys: [passkey] });
        if (call.url.includes('/auth/api-keys')) return reply([]);
        return reply({});
    }) as unknown as typeof fetch;
    return calls;
}

describe('Settings account correctness', () => {
    beforeEach(() => {
        localStorage.setItem('token', mockToken);
    });

    describe('Edit profile', () => {
        it('sends an empty string for every field the user cleared', async () => {
            const calls = installApi((call) =>
                call.url.includes('/auth/profile')
                    ? reply({ ...profile, display_name: null, bio: null, website: null, location: null })
                    : undefined,
            );

            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Edit profile'));
            await screen.findByLabelText('Display Name');

            for (const id of ['display-name', 'bio', 'website', 'location']) {
                const field = document.getElementById(id) as HTMLInputElement;
                expect(field.value, `#${id} starts filled`).not.toBe('');
                fireEvent.change(field, { target: { value: '' } });
            }
            await user.click(screen.getByRole('button', { name: 'Save Profile' }));

            await waitFor(() => {
                expect(calls.some((c) => c.url.includes('/auth/profile') && c.method === 'PUT')).toBe(true);
            });
            const put = calls.find((c) => c.url.includes('/auth/profile') && c.method === 'PUT')!;
            // A missing key means "leave it as it is" to the server, so a
            // cleared field has to be present and empty.
            expect(put.body).toEqual({ display_name: '', bio: '', website: '', location: '' });
            expect(await screen.findByText('Profile updated successfully')).toBeInTheDocument();
        });

        it('sends the other fields unchanged when only one is cleared', async () => {
            const calls = installApi();

            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Edit profile'));
            await screen.findByLabelText('Display Name');
            fireEvent.change(document.getElementById('location') as HTMLInputElement, { target: { value: '' } });
            await user.click(screen.getByRole('button', { name: 'Save Profile' }));

            await waitFor(() => {
                expect(calls.some((c) => c.url.includes('/auth/profile') && c.method === 'PUT')).toBe(true);
            });
            const put = calls.find((c) => c.url.includes('/auth/profile') && c.method === 'PUT')!;
            expect(put.body).toEqual({
                display_name: 'Test User',
                bio: 'Software developer',
                website: 'https://example.com',
                location: '',
            });
        });

        it('limits each field to what the server accepts', async () => {
            installApi();
            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Edit profile'));
            await screen.findByLabelText('Display Name');

            expect(document.getElementById('display-name')).toHaveAttribute('maxlength', '100');
            expect(document.getElementById('bio')).toHaveAttribute('maxlength', '500');
            expect(document.getElementById('location')).toHaveAttribute('maxlength', '100');
            expect(document.getElementById('website')).toHaveAttribute('maxlength', '2048');
        });

        it('shows the server reason when it refuses the profile', async () => {
            installApi((call) =>
                call.url.includes('/auth/profile')
                    ? reply({ error: 'bio: must be at most 500 characters' }, 400)
                    : undefined,
            );

            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Edit profile'));
            await screen.findByLabelText('Display Name');
            await user.click(screen.getByRole('button', { name: 'Save Profile' }));

            expect(await screen.findByRole('alert')).toHaveTextContent('bio: must be at most 500 characters');
        });
    });

    describe('Deleting a passkey', () => {
        it('keeps the replacement token and reloads with it instead of logging out', async () => {
            const calls = installApi((call) =>
                call.url.includes('/auth/passkey/delete')
                    ? reply({ message: 'Passkey deleted successfully', token: 'fresh.jwt.token' })
                    : undefined,
            );

            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Delete passkey'));

            expect(await screen.findByText('Passkey deleted successfully')).toBeInTheDocument();
            expect(localStorage.getItem('token')).toBe('fresh.jwt.token');

            // The reload that follows must carry the new token: the old one was
            // revoked by the delete, and a 401 here would log the user out.
            const deleteAt = calls.findIndex((c) => c.url.includes('/auth/passkey/delete'));
            await waitFor(() => {
                expect(calls.slice(deleteAt + 1).some((c) => c.url.includes('/auth/me'))).toBe(true);
            });
            for (const reload of calls.slice(deleteAt + 1).filter((c) => c.authorization)) {
                expect(reload.authorization).toBe('Bearer fresh.jwt.token');
            }
            expect(mockNavigate).not.toHaveBeenCalledWith('/login');
        });

        it('keeps the stored token when the server sends none', async () => {
            installApi((call) =>
                call.url.includes('/auth/passkey/delete') ? reply({ message: 'Passkey deleted successfully' }) : undefined,
            );

            const { user } = render(<Settings />);
            await user.click(await screen.findByLabelText('Delete passkey'));

            expect(await screen.findByText('Passkey deleted successfully')).toBeInTheDocument();
            expect(localStorage.getItem('token')).toBe(mockToken);
        });
    });

    describe('Adding a passkey', () => {
        it('shows the reason the server gives when it refuses to start', async () => {
            installApi((call) =>
                call.url.includes('/passkey/register/start')
                    ? reply({ error: 'Passkeys are disabled on this instance' }, 403)
                    : undefined,
            );

            const { user } = render(<Settings />);
            await user.click(await screen.findByRole('button', { name: /add passkey/i }));

            expect(await screen.findByRole('alert')).toHaveTextContent('Passkeys are disabled on this instance');
        });

        it('shows the reason the server gives when it refuses to finish', async () => {
            const original = Object.getOwnPropertyDescriptor(navigator, 'credentials');
            Object.defineProperty(navigator, 'credentials', {
                configurable: true,
                value: {
                    create: vi.fn().mockResolvedValue({
                        id: 'cred',
                        type: 'public-key',
                        rawId: new Uint8Array([1]).buffer,
                        response: {
                            attestationObject: new Uint8Array([2]).buffer,
                            clientDataJSON: new Uint8Array([3]).buffer,
                        },
                    }),
                },
            });
            try {
                installApi((call) => {
                    if (call.url.includes('/passkey/register/start')) {
                        return reply({ options: { publicKey: { challenge: 'AQID', user: { id: 'AQID' } } } });
                    }
                    if (call.url.includes('/passkey/register/finish')) {
                        return reply({ error: 'This passkey is already registered' }, 409);
                    }
                    return undefined;
                });

                const { user } = render(<Settings />);
                await user.click(await screen.findByRole('button', { name: /add passkey/i }));

                expect(await screen.findByRole('alert')).toHaveTextContent('This passkey is already registered');
            } finally {
                if (original) Object.defineProperty(navigator, 'credentials', original);
                else Reflect.deleteProperty(navigator, 'credentials');
            }
        });

        it('does not offer Add Passkey to an unverified account, and says why', async () => {
            const calls = installApi(undefined, { email_verified: false });

            const { user } = render(<Settings />);
            const add = await screen.findByRole('button', { name: /add passkey/i });

            expect(add).toBeDisabled();
            expect(add).toHaveAccessibleDescription('Verify your email to add a passkey.');
            await user.click(add);
            expect(calls.some((c) => c.url.includes('/passkey/register/start'))).toBe(false);
        });

        it('lets an unverified account add a passkey when verification is not required', async () => {
            installApi(
                (call) => call.url.includes('/auth/settings')
                    ? reply({ ...appSettings, require_email_verification: false, email_configured: false })
                    : undefined,
                { email_verified: false },
            );

            render(<Settings />);

            expect(await screen.findByText('Unverified')).toBeInTheDocument();
            const add = await screen.findByRole('button', { name: /add passkey/i });
            expect(add).toBeEnabled();
            expect(screen.queryByText('Verify your email to add a passkey.')).not.toBeInTheDocument();
            expect(screen.queryByText(/access all features/i)).not.toBeInTheDocument();
            expect(screen.queryByRole('button', { name: 'Resend' })).not.toBeInTheDocument();
        });

        it('offers a resend without the nag when verification is optional and mail works', async () => {
            installApi(
                (call) => call.url.includes('/auth/settings')
                    ? reply({ ...appSettings, require_email_verification: false, email_configured: true })
                    : undefined,
                { email_verified: false },
            );

            render(<Settings />);

            expect(await screen.findByRole('button', { name: 'Resend' })).toBeInTheDocument();
            expect(screen.getByText('Email not verified')).toBeInTheDocument();
            expect(screen.queryByText(/access all features/i)).not.toBeInTheDocument();
            expect(screen.queryByText('Verify your email to add a passkey.')).not.toBeInTheDocument();
            expect(screen.getByRole('button', { name: /add passkey/i })).toBeEnabled();
        });

        it('offers it, without the hint, once the email is verified', async () => {
            installApi();

            render(<Settings />);
            const add = await screen.findByRole('button', { name: /add passkey/i });

            expect(add).toBeEnabled();
            expect(screen.queryByText('Verify your email to add a passkey.')).not.toBeInTheDocument();
        });
    });

    describe('Deleting the account', () => {
        afterEach(() => {
            vi.restoreAllMocks();
        });

        const reporting = (days: number | null) => (call: Call) =>
            call.url.includes('/auth/settings') ? reply({ ...appSettings, account_purge_days: days }) : undefined;

        async function openDeleteForm() {
            const { user } = render(<Settings />);
            await user.click(await screen.findByRole('button', { name: 'Delete Account' }));
            return user;
        }

        it('says what happens and quotes the removal window the server reports', async () => {
            installApi(reporting(14));
            await openDeleteForm();

            expect(screen.getByText('Your account closes as soon as you confirm.')).toBeInTheDocument();
            expect(screen.getByText(/short links stop working right away/)).toBeInTheDocument();
            expect(
                screen.getByText(/permanently removed 14 days after you delete the account/),
            ).toBeInTheDocument();
            expect(screen.getByText(/only an administrator can restore it/)).toBeInTheDocument();
            // Not a number baked into the deletion copy, and not the old, false promise.
            // The API key form has its own "30 days" expiry choice, so look only here.
            const zone = screen.getByRole('heading', { name: 'Danger Zone' }).closest('section');
            expect(zone).not.toBeNull();
            expect(within(zone as HTMLElement).queryByText(/30 days/)).not.toBeInTheDocument();
            expect(screen.queryByText(/All your links and data will be permanently deleted/)).not.toBeInTheDocument();
        });

        it('says "1 day" for a one-day window', async () => {
            installApi(reporting(1));
            await openDeleteForm();

            expect(screen.getByText(/permanently removed 1 day after you delete the account/)).toBeInTheDocument();
        });

        it('does not name a date when the operator keeps deleted accounts', async () => {
            installApi(reporting(null));
            await openDeleteForm();

            expect(screen.getByText(/stays on the server until an administrator removes it/)).toBeInTheDocument();
            expect(screen.queryByText(/permanently removed \d+ day/)).not.toBeInTheDocument();
        });

        it('repeats the same facts in the final confirmation, and sends nothing when declined', async () => {
            const calls = installApi(reporting(14));
            const confirm = vi.spyOn(window, 'confirm').mockReturnValue(false);
            const user = await openDeleteForm();

            await user.type(screen.getByLabelText('Confirm password'), 'password123');
            await user.click(screen.getByRole('button', { name: 'Delete My Account' }));

            expect(confirm).toHaveBeenCalledTimes(1);
            const question = String(confirm.mock.calls[0][0]);
            expect(question).toContain('links stop working right away');
            expect(question).toContain('permanently removed 14 days after you delete the account');
            expect(question).not.toContain('This cannot be undone');
            expect(calls.some((c) => c.url.includes('/auth/delete-account'))).toBe(false);
        });

        it('deletes once the user confirms', async () => {
            const calls = installApi(reporting(14));
            vi.spyOn(window, 'confirm').mockReturnValue(true);
            const user = await openDeleteForm();

            await user.type(screen.getByLabelText('Confirm password'), 'password123');
            await user.click(screen.getByRole('button', { name: 'Delete My Account' }));

            await waitFor(() => expect(calls.some((c) => c.url.includes('/auth/delete-account'))).toBe(true));
            const sent = calls.find((c) => c.url.includes('/auth/delete-account'))!;
            expect(sent.method).toBe('POST');
            expect(sent.body).toEqual({ email: profile.email, password: 'password123' });
            await waitFor(() => expect(mockNavigate).toHaveBeenCalledWith('/'));
        });
    });

    describe('Copying a new API key', () => {
        const key = 'opn_ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789abcd';

        afterEach(() => {
            window.getSelection()?.removeAllRanges();
        });

        /**
         * Create a key and return the clicker. `clipboard` is set after the page
         * renders because the user-event that `render` sets up installs a
         * clipboard of its own.
         */
        async function createKey(clipboard: unknown) {
            installApi((call) =>
                call.url.includes('/auth/api-keys') && call.method === 'POST'
                    ? reply({ id: 3, name: 'API key', key, key_prefix: key.slice(0, 12), created_at: '2024-01-01T00:00:00Z' }, 201)
                    : undefined,
            );
            const view = render(<Settings />);
            await view.user.click(await screen.findByRole('button', { name: /create key/i }));
            await screen.findByText(key);
            Object.defineProperty(navigator, 'clipboard', { configurable: true, value: clipboard });
            return view.user;
        }

        it('says it copied only after the write succeeds', async () => {
            const writeText = vi.fn().mockResolvedValue(undefined);
            const user = await createKey({ writeText });

            await user.click(screen.getByRole('button', { name: 'Copy' }));

            expect(writeText).toHaveBeenCalledWith(key);
            expect(await screen.findByText('Copied to clipboard')).toBeInTheDocument();
            expect(screen.queryByText(/could not copy/i)).not.toBeInTheDocument();
        });

        it('does not claim it copied when the browser has no clipboard, and selects the key', async () => {
            const user = await createKey(undefined);

            await user.click(screen.getByRole('button', { name: 'Copy' }));

            expect(await screen.findByText(/could not copy automatically/i)).toBeInTheDocument();
            expect(screen.queryByText('Copied to clipboard')).not.toBeInTheDocument();
            expect(window.getSelection()?.toString()).toBe(key);
        });

        it('does not claim it copied when the write is refused, and selects the key', async () => {
            const writeText = vi.fn().mockRejectedValue(new DOMException('denied', 'NotAllowedError'));
            const user = await createKey({ writeText });

            await user.click(screen.getByRole('button', { name: 'Copy' }));

            expect(await screen.findByText(/could not copy automatically/i)).toBeInTheDocument();
            expect(screen.queryByText('Copied to clipboard')).not.toBeInTheDocument();
            expect(window.getSelection()?.toString()).toBe(key);
        });
    });

    describe('Public profile', () => {
        const shown = { timeout: 8000 };
        const hello = {
            id: 1,
            code: 'hello',
            title: 'Hello',
            bio_visible: true,
            bio_label: null,
            bio_position: 0,
            org_id: null,
            created_at: '2024-01-01T00:00:00Z',
        };
        const world = {
            id: 2,
            code: 'world',
            title: 'World',
            bio_visible: true,
            bio_label: null,
            bio_position: 1,
            org_id: null,
            created_at: '2024-01-02T00:00:00Z',
        };
        // Returned newest-position first, so the page has to sort.
        const rows = [
            world,
            hello,
            { id: 3, code: 'hid', title: 'Hidden', bio_visible: false, bio_label: null, bio_position: 0, org_id: null, created_at: '2024-01-03T00:00:00Z' },
            { id: 4, code: 'org', title: 'Org link', bio_visible: true, bio_label: 'Org', bio_position: null, org_id: 9, created_at: '2024-01-04T00:00:00Z' },
        ];

        const publicProfile = { ...profile, bio_username: 'ada', bio_enabled: true, bio_theme: null as string | null };

        function withLinks(extra: (call: Call) => Promise<unknown> | undefined = () => undefined) {
            return installApi((call) => {
                const custom = extra(call);
                if (custom) return custom;
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) return reply(rows);
                return undefined;
            });
        }

        function puts(calls: Call[], suffix: string) {
            return calls.filter((call) => call.method === 'PUT' && call.url.endsWith(suffix));
        }

        it('sorts personal bio links, offers up and down, and leaves hidden and organization links out', async () => {
            const calls = withLinks();
            render(<Settings />);

            const helloLabel = await screen.findByLabelText('Label for Hello', {}, shown);
            const items = screen.getAllByRole('listitem');
            expect(items[0]).toHaveTextContent('Hello');
            expect(items[1]).toHaveTextContent('World');
            expect(items).toHaveLength(2);
            expect(screen.queryByText('Hidden')).not.toBeInTheDocument();
            expect(screen.getByText(/1 organization link stays at the end of the page/)).toBeInTheDocument();
            expect(calls.some((call) => call.method === 'GET' && call.url.includes('bio_visible=true'))).toBe(true);
            expect(screen.getByRole('button', { name: 'Move Hello up' })).toBeDisabled();
            expect(screen.getByRole('button', { name: 'Move World down' })).toBeDisabled();
            expect(helloLabel).toHaveValue('');
        });

        it('reverts the list when reordering is refused', async () => {
            const calls = withLinks((call) =>
                call.method === 'PUT' && call.url.endsWith('/auth/bio/order')
                    ? reply({ error: 'List every personal link on your bio page' }, 400)
                    : undefined,
            );
            const view = render(<Settings />);
            await screen.findByLabelText('Label for Hello', {}, shown);

            await view.user.click(screen.getByRole('button', { name: 'Move Hello down' }));

            expect(await screen.findByRole('alert', {}, shown)).toHaveTextContent('List every personal link on your bio page');
            const items = screen.getAllByRole('listitem');
            expect(items[0]).toHaveTextContent('Hello');
            expect(items[1]).toHaveTextContent('World');
            expect(puts(calls, '/auth/bio/order')[0].body).toEqual({ link_ids: [2, 1] });
        });

        it('sends the reordered ids and keeps the buttons disabled until that finishes', async () => {
            let release!: () => void;
            const gate = new Promise<void>((resolve) => { release = resolve; });
            const calls = withLinks((call) => {
                if (call.method === 'PUT' && call.url.endsWith('/auth/bio/order')) {
                    const ids = (call.body as { link_ids: number[] }).link_ids;
                    return gate.then(() => reply({ link_ids: ids }));
                }
                return undefined;
            });
            const view = render(<Settings />);
            await screen.findByLabelText('Label for Hello', {}, shown);

            const pending = view.user.click(screen.getByRole('button', { name: 'Move Hello down' }));
            // "Move Hello up" starts disabled because Hello is first. It stays
            // disabled through the request, and is enabled only once Hello is
            // second and the request has finished.
            await waitFor(() => {
                expect(screen.getAllByRole('listitem')[0]).toHaveTextContent('World');
                expect(screen.getByRole('button', { name: 'Move Hello up' })).toBeDisabled();
            }, shown);
            release();
            await pending;
            await waitFor(() => expect(screen.getByRole('button', { name: 'Move Hello up' })).toBeEnabled(), shown);
            expect(puts(calls, '/auth/bio/order')[0].body).toEqual({ link_ids: [2, 1] });
            expect(screen.getAllByRole('listitem')[0]).toHaveTextContent('World');
        });

        it('saves a label on blur and skips a blur that changed nothing', async () => {
            const calls = withLinks((call) =>
                call.method === 'PUT' && call.url.endsWith('/links/1')
                    ? reply({ ...hello, bio_label: (call.body as { bio_label: string }).bio_label || null })
                    : undefined,
            );
            render(<Settings />);
            const input = await screen.findByLabelText('Label for Hello', {}, shown);

            fireEvent.blur(input);
            expect(puts(calls, '/links/1')).toHaveLength(0);

            fireEvent.change(input, { target: { value: 'Shop' } });
            fireEvent.blur(input);
            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(1));
            expect(puts(calls, '/links/1')[0].body).toEqual({ bio_label: 'Shop' });

            fireEvent.blur(input);
            expect(puts(calls, '/links/1')).toHaveLength(1);
        });

        it('clears a label by sending an empty string', async () => {
            const labeled = { ...hello, bio_label: 'Shop' };
            const calls = installApi((call) => {
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) return reply([labeled, world]);
                if (call.method === 'PUT' && call.url.endsWith('/links/1')) return reply({ ...labeled, bio_label: null });
                return undefined;
            });
            render(<Settings />);
            const input = await screen.findByLabelText('Label for Shop', {}, shown);
            expect(input).toHaveValue('Shop');

            fireEvent.change(input, { target: { value: '' } });
            fireEvent.blur(input);

            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(1));
            expect(puts(calls, '/links/1')[0].body).toEqual({ bio_label: '' });
        });

        it('sends the theme with the bio settings, and an unchanged avatar is not saved', async () => {
            const calls = installApi((call) => {
                if (call.method === 'GET' && call.url.includes('/links?')) return reply([]);
                if (call.method === 'PUT' && call.url.endsWith('/auth/bio')) {
                    return reply({ bio_username: '', bio_enabled: false, bio_theme: (call.body as { bio_theme: string }).bio_theme || null });
                }
                return undefined;
            });
            const view = render(<Settings />);
            expect(await screen.findByLabelText('Username', {}, shown)).toBeInTheDocument();
            expect(screen.queryByText(/No links on your bio page yet/)).not.toBeInTheDocument();

            await view.user.click(screen.getByRole('button', { name: 'Save' }));
            await screen.findByText('Bio page settings saved', {}, shown);
            expect(puts(calls, '/auth/bio')[0].body).toEqual({
                bio_username: '',
                bio_enabled: false,
                bio_theme: '',
            });
            expect(puts(calls, '/auth/profile')).toHaveLength(0);

            await view.user.click(screen.getByRole('radio', { name: 'Ink' }));
            await view.user.click(screen.getByRole('button', { name: 'Save' }));
            await waitFor(() => expect(puts(calls, '/auth/bio')).toHaveLength(2));
            expect(puts(calls, '/auth/bio')[1].body).toEqual({
                bio_username: '',
                bio_enabled: false,
                bio_theme: 'ink',
            });
        });

        it('refuses an unsafe avatar before saving, and previews a saved http avatar through the proxy', async () => {
            const avatar = 'https://cdn.iana.org/a.png';
            const calls = installApi((call) => {
                if (call.method === 'GET' && call.url.includes('/links?')) return reply([]);
                if (call.method === 'PUT' && call.url.endsWith('/auth/bio')) {
                    return reply({
                        bio_username: (call.body as { bio_username: string }).bio_username,
                        bio_enabled: (call.body as { bio_enabled: boolean }).bio_enabled,
                        bio_theme: (call.body as { bio_theme: string }).bio_theme || null,
                    });
                }
                if (call.method === 'PUT' && call.url.endsWith('/auth/profile')) {
                    return reply({ avatar_url: (call.body as { avatar_url: string }).avatar_url });
                }
                return undefined;
            });
            const view = render(<Settings />);
            const avatarInput = await screen.findByLabelText('Avatar URL', {}, shown);

            fireEvent.change(avatarInput, { target: { value: 'javascript:alert(1)' } });
            await view.user.click(screen.getByRole('button', { name: 'Save' }));
            expect(await screen.findByRole('alert', {}, shown)).toHaveTextContent('Avatar must be an http or https URL');
            expect(puts(calls, '/auth/bio')).toHaveLength(0);
            expect(puts(calls, '/auth/profile')).toHaveLength(0);

            fireEvent.change(screen.getByLabelText('Username'), { target: { value: 'ada' } });
            await view.user.click(screen.getByRole('checkbox', { name: 'Make my bio page public' }));
            fireEvent.change(avatarInput, { target: { value: avatar } });
            await view.user.click(screen.getByRole('button', { name: 'Save' }));

            await screen.findByText('Bio page settings saved', {}, shown);
            expect(puts(calls, '/auth/bio')[0].body).toEqual({
                bio_username: 'ada',
                bio_enabled: true,
                bio_theme: '',
            });
            expect(puts(calls, '/auth/profile')[0].body).toEqual({ avatar_url: avatar });
            expect(screen.getByRole('img', { name: 'Avatar preview' })).toHaveAttribute('src', expect.stringContaining('/api/bio/avatar?url='));
        });

        it('does not load bio links until the page is public or the list is opened', async () => {
            const calls = installApi();
            const view = render(<Settings />);
            await screen.findByLabelText('Username', {}, shown);

            expect(calls.some((call) => call.url.includes('/links?'))).toBe(false);
            expect(screen.queryByText(/No links on your bio page yet/)).not.toBeInTheDocument();

            await view.user.click(screen.getByRole('button', { name: 'Show links on your page' }));

            expect(await screen.findByText(/No links on your bio page yet/, {}, shown)).toBeInTheDocument();
            const lists = calls.filter((call) => call.method === 'GET' && call.url.includes('/links?'));
            expect(lists).toHaveLength(1);
            expect(lists[0].url).toContain('bio_visible=true');
        });

        it('orders a fractional second by the instant, then the higher id', async () => {
            const rows = [
                { id: 2, code: 'early', title: 'Early', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2026-01-01T00:00:00.1Z' },
                { id: 1, code: 'later', title: 'Later', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2026-01-01T00:00:00.12Z' },
                { id: 10, code: 'low', title: 'Low', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2026-01-02T00:00:00Z' },
                { id: 11, code: 'high', title: 'High', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2026-01-02T00:00:00Z' },
            ];
            installApi((call) => {
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) return reply(rows);
                return undefined;
            });
            render(<Settings />);

            const items = await screen.findAllByRole('listitem', {}, shown);
            expect(items.map((item) => item.textContent)).toEqual([
                expect.stringContaining('High'),
                expect.stringContaining('Low'),
                expect.stringContaining('Later'),
                expect.stringContaining('Early'),
            ]);
        });

        it('keeps newer label text when an earlier save returns', async () => {
            let release!: () => void;
            const gate = new Promise<void>((resolve) => { release = resolve; });
            const calls = withLinks((call) => {
                if (call.method === 'PUT' && call.url.endsWith('/links/1')) {
                    const label = (call.body as { bio_label: string }).bio_label;
                    if (label === 'Shop') return gate.then(() => reply({ ...hello, bio_label: 'Shop' }));
                    return reply({ ...hello, bio_label: label || null });
                }
                return undefined;
            });
            render(<Settings />);
            const input = await screen.findByLabelText('Label for Hello', {}, shown);

            fireEvent.change(input, { target: { value: 'Shop' } });
            fireEvent.blur(input);
            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(1), shown);

            fireEvent.change(input, { target: { value: 'Shop window' } });
            fireEvent.blur(input);
            release();

            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(2), shown);
            expect(puts(calls, '/links/1')[0].body).toEqual({ bio_label: 'Shop' });
            expect(puts(calls, '/links/1')[1].body).toEqual({ bio_label: 'Shop window' });
            expect(await screen.findByLabelText('Label for Shop window', {}, shown)).toHaveValue('Shop window');
        });

        it('saves again when the text goes back to the value before an earlier save returns', async () => {
            let release!: () => void;
            const gate = new Promise<void>((resolve) => { release = resolve; });
            const calls = withLinks((call) => {
                if (call.method === 'PUT' && call.url.endsWith('/links/1')) {
                    const label = (call.body as { bio_label: string }).bio_label;
                    if (label === 'Shop') return gate.then(() => reply({ ...hello, bio_label: 'Shop' }));
                    return reply({ ...hello, bio_label: label || null });
                }
                return undefined;
            });
            render(<Settings />);
            const input = await screen.findByLabelText('Label for Hello', {}, shown);

            fireEvent.change(input, { target: { value: 'Shop' } });
            fireEvent.blur(input);
            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(1), shown);

            // Back to what the row held before the first save: the page still
            // has to clear the label the first request is about to store.
            fireEvent.change(input, { target: { value: '' } });
            fireEvent.blur(input);
            release();

            await waitFor(() => expect(puts(calls, '/links/1')).toHaveLength(2), shown);
            expect(puts(calls, '/links/1')[1].body).toEqual({ bio_label: '' });
            expect(await screen.findByLabelText('Label for Hello', {}, shown)).toHaveValue('');
        });

        it('does not reorder when a bio link is missing from the list', async () => {
            const calls = installApi((call) => {
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) {
                    return reply([hello, world], 200, { 'X-Total-Count': '3' });
                }
                return undefined;
            });
            render(<Settings />);

            expect(await screen.findByRole('alert', {}, shown)).toHaveTextContent(/Could not load every link on your bio page/);
            expect(screen.queryByText(/No links on your bio page yet/)).not.toBeInTheDocument();
            expect(screen.getByLabelText('Move Hello down')).toBeDisabled();
            fireEvent.click(screen.getByLabelText('Move Hello down'));
            expect(puts(calls, '/auth/bio/order')).toHaveLength(0);
        });

        it('loads a later page before offering a reorder', async () => {
            const orgs = Array.from({ length: 1000 }, (_, index) => ({
                id: 3000 + index,
                code: `o${index}`,
                title: `Org ${index}`,
                bio_visible: true,
                bio_label: null,
                bio_position: null,
                org_id: 9,
                created_at: '2024-06-01T00:00:00Z',
            }));
            const older = { id: 1, code: 'old', title: 'Older personal', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2020-01-01T00:00:00Z' };
            const newer = { id: 2, code: 'new', title: 'Newer personal', bio_visible: true, bio_label: null, bio_position: null, org_id: null, created_at: '2021-01-01T00:00:00Z' };
            const calls = installApi((call) => {
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) {
                    const offset = Number(new URL(call.url).searchParams.get('offset') ?? '0');
                    const page = offset === 0 ? orgs : [older, newer];
                    return reply(page, 200, { 'X-Total-Count': '1002' });
                }
                if (call.method === 'PUT' && call.url.endsWith('/auth/bio/order')) {
                    return reply({ link_ids: (call.body as { link_ids: number[] }).link_ids });
                }
                return undefined;
            });
            const view = render(<Settings />);

            expect(await screen.findByLabelText('Label for Newer personal', {}, shown)).toBeInTheDocument();
            const items = screen.getAllByRole('listitem');
            expect(items).toHaveLength(2);
            expect(items[0]).toHaveTextContent('Newer personal');
            expect(items[1]).toHaveTextContent('Older personal');
            expect(screen.getByText(/1000 organization links stay at the end of the page/)).toBeInTheDocument();
            expect(calls.filter((call) => call.url.includes('/links?')).map((call) => new URL(call.url).searchParams.get('offset'))).toEqual(['0', '1000']);

            await view.user.click(screen.getByRole('button', { name: 'Move Newer personal down' }));
            await waitFor(() => expect(puts(calls, '/auth/bio/order')).toHaveLength(1), shown);
            expect(puts(calls, '/auth/bio/order')[0].body).toEqual({ link_ids: [1, 2] });
        });

        // The 500-link cap itself is unit-tested in bioOrder.test.ts: rendering
        // 501 rows in jsdom takes minutes.

        it('shows a failed link load instead of the empty bio page', async () => {
            installApi((call) => {
                if (call.url.includes('/auth/me')) return reply(publicProfile);
                if (call.method === 'GET' && call.url.includes('/links?')) return reply({ error: 'Database error' }, 500);
                return undefined;
            });
            render(<Settings />);

            expect(await screen.findByRole('alert', {}, shown)).toHaveTextContent('Database error');
            expect(screen.queryByText(/No links on your bio page yet/)).not.toBeInTheDocument();
            expect(screen.queryByRole('listitem')).not.toBeInTheDocument();
        });
    });
});
