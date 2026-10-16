import { act } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../../test/test-utils';
import CreateLinkForm from './CreateLinkForm';
import type { AppSettings } from './types';

const settings: AppSettings = {
    custom_aliases_enabled: true,
    min_alias_length: 5,
    max_alias_length: 32,
    qr_branding_enabled: false,
    burn_after_reading_enabled: true,
    safe_link_interstitial_enabled: false,
    conditional_routing_enabled: false,
    link_in_bio_enabled: false,
};

function jsonResponse(body: unknown, status = 200): Response {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: () => Promise.resolve(body),
    } as Response;
}

const RATE_LIMITED = {
    error: 'Too many requests',
    retry_after: 12,
    message: 'Rate limit exceeded. Please try again in 12 seconds.',
};

function renderForm(
    appSettings: AppSettings = settings,
    extra: {
        folders?: { id: number; name: string; color: string | null; link_count: number }[];
        tags?: { id: number; name: string; color: string | null; link_count: number }[];
        orgId?: number | null;
        orgName?: string | null;
    } = {},
) {
    const onCreated = vi.fn();
    const onError = vi.fn();
    const view = render(
        <CreateLinkForm
            appSettings={appSettings}
            initialUrl=""
            onCreated={onCreated}
            onError={onError}
            {...extra}
        />,
    );
    return { ...view, onCreated, onError };
}

async function openAdvanced() {
    fireEvent.click(screen.getByRole('button', { name: 'Advanced options' }));
}

function createPosts() {
    return vi.mocked(fetch).mock.calls.filter(([input, init]) => {
        const url = String(input);
        const method = (init?.method ?? 'GET').toUpperCase();
        return method === 'POST' && url.endsWith('/links');
    });
}

describe('CreateLinkForm', () => {
    beforeEach(() => {
        vi.mocked(fetch).mockReset();
        vi.mocked(fetch).mockResolvedValue(jsonResponse({ id: 1, code: 'abc' }, 201));
    });

    it('sends notes, a click limit and a start date as RFC 3339 UTC', async () => {
        const { user, onCreated } = renderForm();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
        await openAdvanced();
        fireEvent.change(screen.getByLabelText('Notes'), { target: { value: 'hello note' } });
        fireEvent.change(screen.getByLabelText('Click limit'), { target: { value: '3' } });
        fireEvent.change(screen.getByLabelText('Start date'), { target: { value: '2031-04-01T09:15' } });
        fireEvent.change(screen.getByLabelText(/^expiration$/i), { target: { value: '2031-04-02' } });

        await user.click(screen.getByRole('button', { name: /^Create$/ }));

        await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
        expect(createPosts()).toHaveLength(1);
        expect(JSON.parse(String(createPosts()[0][1]?.body))).toEqual({
            original_url: 'https://example.com/a',
            notes: 'hello note',
            max_clicks: 3,
            starts_at: new Date('2031-04-01T09:15').toISOString(),
            expires_at: new Date('2031-04-02T23:59:00').toISOString(),
        });
    });

    it('refuses a start that is not before the expiration and does not create the link', async () => {
        const { user, onError, onCreated } = renderForm();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
        await openAdvanced();
        fireEvent.change(screen.getByLabelText('Start date'), { target: { value: '2030-01-16T00:00' } });
        fireEvent.change(screen.getByLabelText(/^expiration$/i), { target: { value: '2030-01-15' } });

        await user.click(screen.getByRole('button', { name: /^Create$/ }));

        expect(onError).toHaveBeenLastCalledWith('Start date must be before the expiration');
        expect(onCreated).not.toHaveBeenCalled();
        expect(createPosts()).toHaveLength(0);
    });

    it('rejects a click limit that is not a positive whole number', async () => {
        const { user, onError } = renderForm();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
        await openAdvanced();
        fireEvent.change(screen.getByLabelText('Click limit'), { target: { value: '0' } });

        await user.click(screen.getByRole('button', { name: /^Create$/ }));

        expect(onError).toHaveBeenLastCalledWith('Click limit must be a positive whole number');
        expect(createPosts()).toHaveLength(0);
    });

    it('shows the notes counter only near the 5000 character cap', async () => {
        renderForm();
        await openAdvanced();
        const notes = screen.getByLabelText('Notes');
        fireEvent.change(notes, { target: { value: 'a'.repeat(4499) } });
        expect(screen.queryByText(/characters left/)).not.toBeInTheDocument();

        fireEvent.change(notes, { target: { value: 'a'.repeat(4500) } });
        expect(screen.getByText('500 characters left')).toBeInTheDocument();

        fireEvent.change(notes, { target: { value: 'a'.repeat(5001) } });
        expect(notes).toHaveValue('a'.repeat(5000));
        expect(screen.getByText('0 characters left')).toBeInTheDocument();
    });

    it('disables the click limit while burn after reading is on and does not send one', async () => {
        const { user, onCreated } = renderForm();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/once' } });
        await openAdvanced();
        const limit = screen.getByLabelText('Click limit');
        fireEvent.change(limit, { target: { value: '9' } });
        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));

        expect(limit).toBeDisabled();
        expect(limit).toHaveValue(1);
        expect(screen.getByText('Burn after reading stops the link after 1 click.')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: /^Create$/ }));
        await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
        const body = JSON.parse(String(createPosts()[0][1]?.body));
        expect(body.burn_after_reading).toBe(true);
        expect(body).not.toHaveProperty('max_clicks');
    });

    it('sends the typed click limit again after burn is turned back off', async () => {
        const { user, onCreated } = renderForm();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/once' } });
        await openAdvanced();
        const limit = screen.getByLabelText('Click limit');
        fireEvent.change(limit, { target: { value: '9' } });
        const burn = screen.getByRole('checkbox', { name: /burn after reading/i });
        await user.click(burn);
        await user.click(burn);

        expect(limit).toBeEnabled();
        expect(limit).toHaveValue(9);
        expect(screen.getByText('Link stops working after this many clicks')).toBeInTheDocument();

        await user.click(screen.getByRole('button', { name: /^Create$/ }));
        await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
        const body = JSON.parse(String(createPosts()[0][1]?.body));
        expect(body.max_clicks).toBe(9);
        expect(body).not.toHaveProperty('burn_after_reading');
    });

    describe('alias check', () => {
        beforeEach(() => {
            vi.useFakeTimers();
        });
        afterEach(() => {
            vi.useRealTimers();
        });

        const aliasInput = () => screen.getByPlaceholderText(/^alias \(/);

        async function advance(ms: number) {
            await act(async () => {
                await vi.advanceTimersByTimeAsync(ms);
            });
        }

        it('waits 300ms and does not ask about an empty or too-short alias', async () => {
            renderForm();
            fireEvent.change(aliasInput(), { target: { value: 'ab' } });
            await advance(1000);
            expect(fetch).not.toHaveBeenCalled();

            fireEvent.change(aliasInput(), { target: { value: 'helloo' } });
            await advance(299);
            expect(fetch).not.toHaveBeenCalled();
            await advance(1);
            expect(fetch).toHaveBeenCalledTimes(1);
            expect(String(vi.mocked(fetch).mock.calls[0][0])).toContain('/links/check-code?code=helloo');
        });

        it('uses the configured minimum length', async () => {
            renderForm({ ...settings, min_alias_length: 4 });
            fireEvent.change(aliasInput(), { target: { value: 'abc' } });
            await advance(1000);
            expect(fetch).not.toHaveBeenCalled();
            fireEvent.change(aliasInput(), { target: { value: 'abcd' } });
            await advance(300);
            expect(fetch).toHaveBeenCalledTimes(1);
            expect(String(vi.mocked(fetch).mock.calls[0][0])).toContain('code=abcd');
        });

        it('shows Available, Taken, and the server validation message', async () => {
            const messages = [
                { available: true, message: 'This alias is available', shown: 'Available' },
                { available: false, message: 'This alias is already taken', shown: 'Taken' },
                { available: false, message: 'This alias is reserved and cannot be used', shown: 'This alias is reserved and cannot be used' },
            ];
            for (const [index, item] of messages.entries()) {
                vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({
                    available: item.available,
                    code: `code${index}x`,
                    message: item.message,
                }));
            }
            renderForm();
            for (const [index, item] of messages.entries()) {
                fireEvent.change(aliasInput(), { target: { value: `code${index}x` } });
                await advance(300);
                expect(screen.getByRole('status')).toHaveTextContent(item.shown);
            }
        });

        it('aborts the previous request and ignores its late response', async () => {
            let resolveFirst: ((value: Response) => void) | undefined;
            let firstSignal: AbortSignal | undefined;
            vi.mocked(fetch).mockImplementation((input, init) => {
                const code = new URL(String(input)).searchParams.get('code');
                if (code === 'helloo') {
                    firstSignal = init?.signal ?? undefined;
                    return new Promise(resolve => {
                        resolveFirst = resolve;
                    });
                }
                return Promise.resolve(jsonResponse({
                    available: true,
                    code,
                    message: 'This alias is available',
                }));
            });
            renderForm();
            fireEvent.change(aliasInput(), { target: { value: 'helloo' } });
            await advance(300);
            expect(fetch).toHaveBeenCalledTimes(1);

            fireEvent.change(aliasInput(), { target: { value: 'worlds' } });
            expect(firstSignal?.aborted).toBe(true);
            await advance(300);
            expect(screen.getByRole('status')).toHaveTextContent('Available');

            await act(async () => {
                resolveFirst?.(jsonResponse({
                    available: false,
                    code: 'helloo',
                    message: 'This alias is already taken',
                }));
            });
            expect(screen.getByRole('status')).toHaveTextContent('Available');
            expect(screen.queryByText('Taken')).not.toBeInTheDocument();
        });

        it('says how long to wait when the alias check is rate limited', async () => {
            vi.mocked(fetch).mockResolvedValue(jsonResponse(RATE_LIMITED, 429));
            renderForm();
            fireEvent.change(aliasInput(), { target: { value: 'helloo' } });
            await advance(300);

            expect(screen.getByRole('status')).toHaveTextContent('Rate limit exceeded. Please try again in 12 seconds.');
            expect(screen.getByRole('status')).toHaveClass('text-danger');
            expect(screen.queryByText('Too many requests')).not.toBeInTheDocument();
        });

        it('still creates the link when the alias check says it is taken', async () => {
            vi.mocked(fetch).mockImplementation((input, init) => {
                const url = String(input);
                if (url.includes('/check-code')) {
                    return Promise.resolve(jsonResponse({
                        available: false,
                        code: 'helloo',
                        message: 'This alias is already taken',
                    }));
                }
                return Promise.resolve(jsonResponse({ id: 1, code: 'helloo' }, 201));
            });
            const { onCreated } = renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
            fireEvent.change(aliasInput(), { target: { value: 'helloo' } });
            await advance(300);
            expect(screen.getByRole('status')).toHaveTextContent('Taken');

            await act(async () => {
                fireEvent.submit(screen.getByRole('button', { name: /^Create$/ }).closest('form')!);
            });
            expect(onCreated).toHaveBeenCalledOnce();
            expect(createPosts()).toHaveLength(1);
            expect(JSON.parse(String(createPosts()[0][1]?.body)).custom_alias).toBe('helloo');
        });
    });

    describe('UTM builder', () => {
        it('calls build-utm and replaces the destination with the server URL', async () => {
            const { user } = renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
            await openAdvanced();
            expect(screen.queryByLabelText('Source')).not.toBeInTheDocument();
            await user.click(screen.getByRole('button', { name: 'Add UTM parameters' }));

            const apply = screen.getByRole('button', { name: 'Add to URL' });
            expect(apply).toBeDisabled();
            fireEvent.change(screen.getByLabelText('Source'), { target: { value: 'newsletter' } });
            fireEvent.change(screen.getByLabelText('Medium'), { target: { value: ' email ' } });
            expect(apply).toBeEnabled();

            vi.mocked(fetch).mockResolvedValueOnce(jsonResponse({
                original_url: 'https://example.com/a',
                url_with_utm: 'https://example.com/a?utm_source=newsletter&utm_medium=email&from=server',
                utm_params: { utm_source: 'newsletter', utm_medium: 'email' },
            }));
            await user.click(apply);

            await waitFor(() => expect(fetch).toHaveBeenCalled());
            const [url, init] = vi.mocked(fetch).mock.calls[0];
            expect(String(url)).toMatch(/\/links\/build-utm$/);
            expect(init?.method).toBe('POST');
            expect(JSON.parse(String(init?.body))).toEqual({
                url: 'https://example.com/a',
                utm_source: 'newsletter',
                utm_medium: 'email',
            });
            expect(screen.getByLabelText('Destination URL')).toHaveValue(
                'https://example.com/a?utm_source=newsletter&utm_medium=email&from=server',
            );
        });

        it('shows the server error and leaves the destination unchanged', async () => {
            const { user } = renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'not a url' } });
            await openAdvanced();
            await user.click(screen.getByRole('button', { name: 'Add UTM parameters' }));
            fireEvent.change(screen.getByLabelText('Campaign'), { target: { value: 'spring' } });
            vi.mocked(fetch).mockResolvedValueOnce({
                ok: false,
                status: 400,
                json: () => Promise.resolve({ error: 'Invalid URL format' }),
            } as Response);

            await user.click(screen.getByRole('button', { name: 'Add to URL' }));

            expect(await screen.findByRole('alert')).toHaveTextContent('Invalid URL format');
            expect(screen.getByLabelText('Destination URL')).toHaveValue('not a url');
        });

        it('says how long to wait when the builder is rate limited', async () => {
            const { user } = renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
            await openAdvanced();
            await user.click(screen.getByRole('button', { name: 'Add UTM parameters' }));
            fireEvent.change(screen.getByLabelText('Campaign'), { target: { value: 'spring' } });
            vi.mocked(fetch).mockResolvedValueOnce(jsonResponse(RATE_LIMITED, 429));

            await user.click(screen.getByRole('button', { name: 'Add to URL' }));

            expect(await screen.findByRole('alert')).toHaveTextContent('Rate limit exceeded. Please try again in 12 seconds.');
            expect(screen.getByLabelText('Destination URL')).toHaveValue('https://example.com/a');
        });

        it('does not send a second request while one is in flight', async () => {
            let resolveBuild: ((value: Response) => void) | undefined;
            vi.mocked(fetch).mockImplementation(() => new Promise(resolve => {
                resolveBuild = resolve;
            }));
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
            await openAdvanced();
            fireEvent.click(screen.getByRole('button', { name: 'Add UTM parameters' }));
            fireEvent.change(screen.getByLabelText('Source'), { target: { value: 'newsletter' } });
            const apply = screen.getByRole('button', { name: 'Add to URL' });
            fireEvent.click(apply);
            fireEvent.click(apply);
            expect(fetch).toHaveBeenCalledTimes(1);
            expect(apply).toBeDisabled();

            await act(async () => {
                resolveBuild?.(jsonResponse({
                    original_url: 'https://example.com/a',
                    url_with_utm: 'https://example.com/a?utm_source=newsletter',
                    utm_params: {},
                }));
            });
            await waitFor(() => expect(screen.getByLabelText('Destination URL')).toHaveValue(
                'https://example.com/a?utm_source=newsletter',
            ));
            expect(fetch).toHaveBeenCalledTimes(1);
        });
    });

    describe('destination check', () => {
        const url = 'https://example.com/a';

        function arm(body: unknown, status = 200) {
            vi.mocked(fetch).mockResolvedValue(jsonResponse(body, status));
        }

        it('is disabled until there is a destination', () => {
            renderForm();
            expect(screen.getByRole('button', { name: 'Check destination' })).toBeDisabled();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: url } });
            expect(screen.getByRole('button', { name: 'Check destination' })).toBeEnabled();
        });

        it('reports a reachable status code', async () => {
            arm({ url, reachable: true, status_code: 200, response_time_ms: 12, error: null });
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: url } });
            fireEvent.click(screen.getByRole('button', { name: 'Check destination' }));

            expect(await screen.findByRole('status')).toHaveTextContent('Reachable · HTTP 200');
            const [endpoint, init] = vi.mocked(fetch).mock.calls[0];
            expect(String(endpoint)).toMatch(/\/links\/health-check$/);
            expect(init?.method).toBe('POST');
            expect(JSON.parse(String(init?.body))).toEqual({ url });
        });

        it('reports a redirect by its status code', async () => {
            arm({ url, reachable: true, status_code: 302, response_time_ms: 4, error: null });
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: url } });
            fireEvent.click(screen.getByRole('button', { name: 'Check destination' }));
            expect(await screen.findByRole('status')).toHaveTextContent('Redirect · HTTP 302');
        });

        it('shows the server error when the destination is refused', async () => {
            arm({
                url: 'http://10.0.0.1/',
                reachable: false,
                status_code: null,
                response_time_ms: null,
                error: 'URL resolves to a disallowed (internal/private) address',
            }, 400);
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'http://10.0.0.1/' } });
            fireEvent.click(screen.getByRole('button', { name: 'Check destination' }));
            expect(await screen.findByRole('status')).toHaveTextContent(
                'URL resolves to a disallowed (internal/private) address',
            );
        });

        it('says how long to wait when the check is rate limited', async () => {
            arm(RATE_LIMITED, 429);
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: url } });
            fireEvent.click(screen.getByRole('button', { name: 'Check destination' }));

            expect(await screen.findByRole('status')).toHaveTextContent('Rate limit exceeded. Please try again in 12 seconds.');
            expect(screen.queryByText('Too many requests')).not.toBeInTheDocument();
        });

        it('sends one request per click and stays disabled while that request is pending', async () => {
            let resolveHealth: ((value: Response) => void) | undefined;
            vi.mocked(fetch).mockImplementation(() => new Promise(resolve => {
                resolveHealth = resolve;
            }));
            renderForm();
            fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: url } });
            const button = screen.getByRole('button', { name: 'Check destination' });
            fireEvent.click(button);
            fireEvent.click(button);
            expect(fetch).toHaveBeenCalledTimes(1);
            await waitFor(() => expect(button).toBeDisabled());

            await act(async () => {
                resolveHealth?.(jsonResponse({ url, reachable: true, status_code: 204, error: null }));
            });
            expect(await screen.findByRole('status')).toHaveTextContent('Reachable · HTTP 204');
            expect(button).toBeEnabled();
            expect(fetch).toHaveBeenCalledTimes(1);
        });
    });

    it('files a new link in the open team, folder and tags', async () => {
        const { user, onCreated } = renderForm(settings, {
            orgId: 8,
            orgName: 'Acme',
            folders: [{ id: 4, name: 'Campaigns', color: '#2563eb', link_count: 0 }],
            tags: [
                { id: 9, name: 'launch', color: '#e11d48', link_count: 0 },
                { id: 2, name: 'press', color: null, link_count: 0 },
            ],
        });
        expect(screen.getByText('Creating in Acme')).toBeInTheDocument();
        fireEvent.change(screen.getByLabelText('Destination URL'), { target: { value: 'https://example.com/a' } });
        await openAdvanced();
        await user.selectOptions(screen.getByLabelText('Folder'), '4');
        await user.click(screen.getByRole('checkbox', { name: 'launch' }));
        await user.click(screen.getByRole('checkbox', { name: 'press' }));
        await user.click(screen.getByRole('button', { name: /^Create$/ }));

        await waitFor(() => expect(onCreated).toHaveBeenCalledOnce());
        expect(JSON.parse(String(createPosts()[0][1]?.body))).toEqual({
            original_url: 'https://example.com/a',
            org_id: 8,
            folder_id: 4,
            tag_ids: [2, 9],
        });
    });
});
