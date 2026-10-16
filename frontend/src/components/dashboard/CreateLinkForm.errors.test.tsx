import { beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../../test/test-utils';
import CreateLinkForm from './CreateLinkForm';
import type { AppSettings } from './types';

const settings: AppSettings = {
    custom_aliases_enabled: false,
    min_alias_length: 5,
    max_alias_length: 25,
    qr_branding_enabled: false,
    burn_after_reading_enabled: false,
    safe_link_interstitial_enabled: false,
    conditional_routing_enabled: false,
    link_in_bio_enabled: false,
};

const SAME_URL = 'You have shortened this URL too many times. Please wait a few minutes.';

function jsonResponse(body: unknown, status = 200) {
    return {
        ok: status >= 200 && status < 300,
        status,
        json: async () => body,
    } as Response;
}

describe('CreateLinkForm errors', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it('shows the rate-limit message, not the generic error', async () => {
        vi.mocked(global.fetch).mockResolvedValue(
            jsonResponse(
                {
                    error: 'Too many requests',
                    retry_after: 42,
                    message: SAME_URL,
                },
                429,
            ),
        );
        const onError = vi.fn();
        const { user } = render(
            <CreateLinkForm appSettings={settings} initialUrl="" onCreated={vi.fn()} onError={onError} />,
        );

        await user.type(screen.getByPlaceholderText('https://example.com/long-url'), 'https://iana.org/repeated');
        await user.click(screen.getByRole('button', { name: 'Create' }));

        await waitFor(() => expect(onError).toHaveBeenCalledWith(SAME_URL));
        const [, init] = vi.mocked(global.fetch).mock.calls[0];
        expect(init?.method).toBe('POST');
        expect(String(vi.mocked(global.fetch).mock.calls[0][0])).toContain('/links');
        expect(init?.body).toBe(JSON.stringify({ original_url: 'https://iana.org/repeated' }));
    });

    it('still shows a plain error when the response has no message', async () => {
        vi.mocked(global.fetch).mockResolvedValue(jsonResponse({ error: 'URL is blocked' }, 403));
        const onError = vi.fn();
        const { user } = render(
            <CreateLinkForm appSettings={settings} initialUrl="" onCreated={vi.fn()} onError={onError} />,
        );

        await user.type(screen.getByPlaceholderText('https://example.com/long-url'), 'https://iana.org/blocked');
        await user.click(screen.getByRole('button', { name: 'Create' }));

        await waitFor(() => expect(onError).toHaveBeenCalledWith('URL is blocked'));
    });
});
