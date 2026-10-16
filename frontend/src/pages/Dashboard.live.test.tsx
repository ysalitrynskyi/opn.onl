import { act, waitFor } from '@testing-library/react';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { mockFetchResponse, mockToken, render, screen } from '../test/test-utils';
import Dashboard from './Dashboard';

const link = {
    id: 7,
    code: 'abc123',
    original_url: 'https://example.com/very-long-url',
    short_url: 'http://localhost:3000/abc123',
    click_count: 2,
    created_at: '2026-01-01T00:00:00Z',
    expires_at: null,
    has_password: false,
    tags: [] as { name: string }[],
    folder_id: null,
    notes: null,
    title: null,
    is_active: true,
    is_pinned: false,
};

function event(linkId: number): string {
    // Same click_count on purpose: an uncapped link reports the stored total
    // plus one, so two clicks before the flush can carry the same number.
    return `data: ${JSON.stringify({ link_id: linkId, click_count: 3 })}\n\n`;
}

function openStream(signal?: AbortSignal): { response: Response; push: (text: string) => void } {
    const encoder = new TextEncoder();
    let controller!: ReadableStreamDefaultController<Uint8Array>;
    const stream = new ReadableStream<Uint8Array>({
        start(control) {
            controller = control;
            signal?.addEventListener('abort', () => {
                try {
                    controller.close();
                } catch {
                    // Already closed.
                }
            });
        },
    });
    return {
        response: new Response(stream, { status: 200, headers: { 'Content-Type': 'text/event-stream' } }),
        push: (text) => controller.enqueue(encoder.encode(text)),
    };
}

describe('Dashboard live clicks', () => {
    beforeEach(() => {
        vi.mocked(localStorage.getItem).mockImplementation((key) => (key === 'token' ? mockToken : null));
    });

    it('increments the row and the header total from the stream, and ignores other links', async () => {
        let push: (text: string) => void = () => {};
        vi.mocked(global.fetch).mockImplementation(async (input, init) => {
            const url = String(input);
            if (url.endsWith('/auth/settings')) {
                return mockFetchResponse({
                    custom_aliases_enabled: true,
                    min_alias_length: 5,
                    max_alias_length: 50,
                }) as any;
            }
            if (url.includes('/links/sparklines')) {
                return mockFetchResponse({ sparklines: [] }) as any;
            }
            if (url.endsWith('/sse')) {
                const opened = openStream(init?.signal ?? undefined);
                push = opened.push;
                return opened.response;
            }
            if (url.includes('/links')) {
                return mockFetchResponse([link]) as any;
            }
            return mockFetchResponse({}) as any;
        });

        render(<Dashboard />);

        expect(await screen.findByRole('status', { name: 'Live' })).toBeInTheDocument();
        expect(screen.getByText('2 clicks', { exact: true })).toBeInTheDocument();
        expect(screen.getByRole('link', { name: '2 clicks, analytics for abc123' })).toBeInTheDocument();

        await act(async () => {
            push(event(99));
            push(event(7));
            push(event(7));
            await Promise.resolve();
        });

        expect(await screen.findByRole('link', { name: '4 clicks, analytics for abc123' })).toBeInTheDocument();
        expect(screen.getByText('4 clicks', { exact: true })).toBeInTheDocument();
        expect(screen.getByRole('link', { name: '4 clicks, analytics for abc123' }).querySelector('.animate-live-click')).not.toBeNull();
        expect(screen.queryByRole('link', { name: /99/ })).not.toBeInTheDocument();

        const sseCall = vi.mocked(global.fetch).mock.calls.find(([input]) => String(input).endsWith('/sse'));
        expect(sseCall).toBeDefined();
        expect(String(sseCall?.[0])).not.toContain('token');
        expect(new Headers(sseCall?.[1]?.headers).get('Authorization')).toBe(`Bearer ${mockToken}`);
        await waitFor(() => {
            expect(screen.getByRole('status', { name: 'Live' })).toBeInTheDocument();
        });
    });

    it('moves the account total too when the loaded rows are only part of the account', async () => {
        let push: (text: string) => void = () => {};
        vi.mocked(global.fetch).mockImplementation(async (input, init) => {
            const url = new URL(String(input));
            if (url.pathname === '/sse') {
                const opened = openStream(init?.signal ?? undefined);
                push = opened.push;
                return opened.response;
            }
            if (url.pathname === '/auth/me') return mockFetchResponse({ total_clicks: 4321 }) as any;
            if (url.pathname === '/links/sparklines') return mockFetchResponse({ sparklines: [] }) as any;
            if (url.pathname === '/links') {
                // The server holds more links than it sent, so the rows say nothing about the account.
                return new Response(JSON.stringify([link]), {
                    status: 200,
                    headers: { 'Content-Type': 'application/json', 'X-Total-Count': '250' },
                });
            }
            return mockFetchResponse({ custom_aliases_enabled: true, min_alias_length: 5, max_alias_length: 50 }) as any;
        });

        render(<Dashboard />);

        expect(await screen.findByText('4,321 clicks', { exact: true })).toBeInTheDocument();
        await screen.findByRole('status', { name: 'Live' });

        await act(async () => {
            push(event(7));
            await Promise.resolve();
        });

        expect(await screen.findByText('4,322 clicks', { exact: true })).toBeInTheDocument();
        expect(screen.getByRole('link', { name: '3 clicks, analytics for abc123' })).toBeInTheDocument();
    });
});
