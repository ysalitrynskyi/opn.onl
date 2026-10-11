import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import ForgotPassword from './ForgotPassword';

function respond(status: number, body: unknown) {
  vi.mocked(global.fetch).mockResolvedValue({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(body),
  } as Response);
}

async function submit(address = 'person@example.com') {
  const { user } = render(<ForgotPassword />);
  await user.type(screen.getByLabelText(/email address/i), address);
  await user.click(screen.getByRole('button', { name: /send reset link/i }));
}

describe('ForgotPassword Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
  });

  it('tells the person to check their email when the request was accepted', async () => {
    respond(200, { message: 'If account exists, password reset email sent' });
    await submit();

    expect(await screen.findByRole('heading', { name: /check your email/i })).toBeInTheDocument();
    expect(screen.getByText('If account exists, password reset email sent')).toBeInTheDocument();
  });

  it('does not claim an email is on its way when the request was rate limited', async () => {
    respond(429, { error: 'Too many requests', retry_after: 42, message: 'Rate limit exceeded.' });
    await submit();

    expect(await screen.findByRole('alert')).toHaveTextContent(/too many attempts/i);
    expect(screen.queryByRole('heading', { name: /check your email/i })).not.toBeInTheDocument();
    // The form is still there to try again.
    expect(screen.getByRole('button', { name: /send reset link/i })).toBeEnabled();
  });

  it('does not claim an email is on its way when the server failed', async () => {
    respond(500, { error: 'Internal error' });
    await submit();

    await waitFor(() => expect(screen.getByRole('alert')).toBeInTheDocument());
    expect(screen.queryByRole('heading', { name: /check your email/i })).not.toBeInTheDocument();
  });

  it('shows an error when the server cannot be reached', async () => {
    vi.mocked(global.fetch).mockRejectedValue(new Error('offline'));
    await submit();

    expect(await screen.findByRole('alert')).toHaveTextContent(/try again later/i);
  });
});
