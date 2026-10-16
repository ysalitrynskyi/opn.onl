import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor, within } from '../test/test-utils';
import Preview from './Preview';

vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        useParams: () => ({ code: 'abc123+' }),
    };
});

const livePreview = {
    code: 'abc123',
    short_url: 'https://links.example.org/abc123',
    original_url: 'https://example.com/landing',
    domain: 'example.com',
    has_password: false,
    is_expired: false,
    is_active: true,
    burn_after_reading: false,
    created_at: '2024-01-01T00:00:00Z',
    click_count: 7,
    reputation: { verdict: 'safe', source: 'internal_blocklist' },
};

function mockPreview(overrides: Record<string, unknown> = {}) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve({ ...livePreview, ...overrides }),
    });
}

describe('Preview page', () => {
    beforeEach(() => {
        mockPreview();
    });

    afterEach(() => {
        vi.unstubAllEnvs();
    });

    it('gives the icon-only destination link an accessible name', async () => {
        render(<Preview />);

        const link = await screen.findByRole('link', { name: 'Open example.com in a new tab' });
        expect(link).toHaveAttribute('href', 'https://example.com/landing');
    });

    it("names the short link with the instance's own host, not a hard-coded opn.onl", async () => {
        vi.stubEnv('VITE_FRONTEND_URL', 'https://links.example.org');
        render(<Preview />);

        expect(await screen.findByText('links.example.org/abc123')).toBeInTheDocument();
        expect(screen.queryByText(/opn\.onl\//)).not.toBeInTheDocument();
    });
    it('offers a live link normally', async () => {
        render(<Preview />);

        expect(await screen.findByRole('link', { name: /visit link/i })).toBeInTheDocument();
        expect(screen.getByText(/looks safe/i)).toBeInTheDocument();
        expect(screen.queryByText(/no longer active/i)).not.toBeInTheDocument();
    });

    it('says a link that has not started yet is not active yet, and disables Visit Link', async () => {
        // A scheduled link is not "no longer" active: it has not begun.
        mockPreview({ is_active: false, inactive_reason: 'not_started', original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link is not active yet')).toBeInTheDocument();
        expect(screen.queryByText(/no longer active/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/has expired/i)).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Not Active Yet' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
        expect(screen.queryByText('Destination')).not.toBeInTheDocument();
        expect(screen.queryByRole('link', { name: /in a new tab/i })).not.toBeInTheDocument();
    });

    it('says a used-up link has reached its click limit', async () => {
        mockPreview({ is_active: false, inactive_reason: 'used_up', original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has reached its click limit')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Unavailable' })).toBeDisabled();
    });

    it('says an opened one-time link has already been opened', async () => {
        mockPreview({
            is_active: false,
            inactive_reason: 'used_up',
            burn_after_reading: true,
            original_url: '',
            domain: '',
        });
        render(<Preview />);

        expect(await screen.findByText('This one-time link has already been opened')).toBeInTheDocument();
        expect(screen.queryByText(/click limit/i)).not.toBeInTheDocument();
    });

    it('says a blocked link has been disabled', async () => {
        mockPreview({ is_active: false, inactive_reason: 'blocked', original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has been disabled')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Unavailable' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
    });

    it('falls back to the generic line when an older API sends no reason', async () => {
        mockPreview({ is_active: false, original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link is no longer active')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Unavailable' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
    });

    it('keeps the expired wording for an expired link', async () => {
        mockPreview({ is_active: false, is_expired: true, inactive_reason: 'expired', original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has expired')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Expired' })).toBeDisabled();
        expect(screen.queryByRole('link', { name: /visit link/i })).not.toBeInTheDocument();
    });

    it('still reads the expired flag when an older API sends no reason', async () => {
        mockPreview({ is_active: false, is_expired: true, original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has expired')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Link Expired' })).toBeDisabled();
    });

    it('explains a one-time link instead of showing an empty destination', async () => {
        mockPreview({
            original_url: '',
            domain: '',
            burn_after_reading: true,
            reputation: { verdict: 'unknown', source: 'internal_blocklist' },
        });
        render(<Preview />);

        expect(await screen.findByText('Destination hidden')).toBeInTheDocument();
        expect(screen.getByText(/can be opened only once/i)).toBeInTheDocument();
        expect(screen.queryByText(/looks safe/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/heading to/i)).not.toBeInTheDocument();
        expect(screen.queryByRole('link', { name: /in a new tab/i })).not.toBeInTheDocument();
        // The link is still live, so it can be visited.
        expect(screen.getByRole('link', { name: /visit link/i })).toBeInTheDocument();
    });

    it('offers a report on a link that is not active', async () => {
        mockPreview({ is_active: false, inactive_reason: 'blocked', original_url: '', domain: '' });
        render(<Preview />);

        expect(await screen.findByText('This link has been disabled')).toBeInTheDocument();
        expect(screen.getByRole('button', { name: 'Report this link' })).toBeInTheDocument();
    });

    it('does not offer a report when the preview does not exist', async () => {
        global.fetch = vi.fn().mockResolvedValue({
            ok: false,
            status: 404,
            json: () => Promise.resolve({ error: 'not found' }),
        });
        render(<Preview />);

        expect(await screen.findByRole('heading', { name: 'Link Not Found' })).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: 'Report this link' })).not.toBeInTheDocument();
    });
});

describe('Report this link', () => {
    function mockReport(
        report: { status: number; body?: unknown } | 'network',
        overrides: Record<string, unknown> = {},
    ) {
        global.fetch = vi.fn((url: string) => {
            if (String(url).endsWith('/report')) {
                if (report === 'network') return Promise.reject(new Error('offline'));
                const status = report.status;
                return Promise.resolve({
                    ok: status >= 200 && status < 300,
                    status,
                    json: () => Promise.resolve(report.body ?? {}),
                });
            }
            return Promise.resolve({
                ok: true,
                status: 200,
                json: () => Promise.resolve({ ...livePreview, ...overrides }),
            });
        }) as any;
    }

    async function submitReport(fields: { details?: string; email?: string } = {}) {
        render(<Preview />);
        fireEvent.click(await screen.findByRole('button', { name: 'Report this link' }));
        fireEvent.click(screen.getByRole('radio', { name: 'Phishing' }));
        if (fields.details !== undefined) {
            fireEvent.change(screen.getByLabelText('Details'), { target: { value: fields.details } });
        }
        if (fields.email !== undefined) {
            fireEvent.change(screen.getByLabelText('Email (optional)'), { target: { value: fields.email } });
        }
        fireEvent.click(screen.getByRole('button', { name: 'Submit report' }));
    }

    function reportCall() {
        const fetchMock = global.fetch as ReturnType<typeof vi.fn>;
        const call = fetchMock.mock.calls.find((entry) => String(entry[0]).endsWith('/report'));
        expect(call, 'POST /report').toBeTruthy();
        return call as [string, RequestInit];
    }

    it('posts the code, reason and details, and leaves a blank email out', async () => {
        mockReport({ status: 201, body: { status: 'received' } });
        await submitReport({ details: '  Looks wrong  ', email: '   ' });

        expect(await screen.findByText('Thanks, an administrator will review this link')).toBeInTheDocument();
        const [url, init] = reportCall();
        expect(url).toBe('http://localhost:3000/report');
        expect(init.method).toBe('POST');
        expect(init.headers).toEqual({ 'Content-Type': 'application/json' });
        expect(JSON.parse(String(init.body))).toEqual({
            code: 'abc123',
            reason: 'phishing',
            details: 'Looks wrong',
        });
    });

    it('moves focus to the success status', async () => {
        mockReport({ status: 201, body: { status: 'received' } });
        await submitReport();

        const status = within(await screen.findByRole('dialog')).getByRole('status');
        expect(status).toHaveTextContent('Thanks, an administrator will review this link');
        await waitFor(() => {
            expect(status).toHaveFocus();
        });
    });

    it('includes an email when the visitor typed one', async () => {
        mockReport({ status: 201, body: { status: 'received' } });
        await submitReport({ email: 'Bob@Example.com' });

        await screen.findByText('Thanks, an administrator will review this link');
        const [, init] = reportCall();
        expect(JSON.parse(String(init.body))).toEqual({
            code: 'abc123',
            reason: 'phishing',
            email: 'Bob@Example.com',
        });
    });

    it('says the link could not be found', async () => {
        mockReport({ status: 404, body: { error: 'Link not found' } });
        await submitReport();

        expect(await screen.findByRole('alert')).toHaveTextContent('This link could not be found.');
    });

    it('says how long to wait when the report is rate limited', async () => {
        mockReport({ status: 429, body: { error: 'Too many requests', retry_after: 30 } });
        await submitReport();

        expect(await screen.findByRole('alert')).toHaveTextContent(
            'Too many reports. Please try again in 30 seconds.',
        );
    });

    it('shows the validation error', async () => {
        mockReport({ status: 400, body: { error: 'reason must be phishing, malware, spam, illegal, or other' } });
        await submitReport();

        expect(await screen.findByRole('alert')).toHaveTextContent(
            'reason must be phishing, malware, spam, illegal, or other',
        );
    });

    it('says the report could not be sent when the network fails', async () => {
        mockReport('network');
        await submitReport();

        expect(await screen.findByRole('alert')).toHaveTextContent('Could not send the report. Please try again.');
    });

    it('keeps submit disabled until a reason is chosen', async () => {
        mockReport({ status: 201, body: { status: 'received' } });
        render(<Preview />);
        fireEvent.click(await screen.findByRole('button', { name: 'Report this link' }));

        const submit = screen.getByRole('button', { name: 'Submit report' });
        expect(submit).toBeDisabled();
        fireEvent.click(submit);

        const calls = (global.fetch as ReturnType<typeof vi.fn>).mock.calls;
        expect(calls.some((entry) => String(entry[0]).endsWith('/report'))).toBe(false);
    });
});
