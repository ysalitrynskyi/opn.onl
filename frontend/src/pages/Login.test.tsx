import { afterEach, describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Login from './Login';
import { mockFetchResponse, mockFetchError, mockToken } from '../test/test-utils';
import { blockSiteStorage } from '../test/helpers';

describe('Login Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.getItem).mockReturnValue(null);
    vi.mocked(localStorage.setItem).mockClear();
  });

  afterEach(() => {
    Object.defineProperty(window, 'PublicKeyCredential', {
      configurable: true,
      value: undefined,
    });
    Object.defineProperty(navigator, 'credentials', {
      configurable: true,
      value: undefined,
    });
  });

  it('renders login form', () => {
    render(<Login />);
    expect(screen.getByText('Welcome back')).toBeInTheDocument();
    expect(screen.getByLabelText(/email address/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /sign in/i })).toBeInTheDocument();
  });

  it('posts the form so a native submit cannot leak credentials into the URL', () => {
    render(<Login />);
    expect(screen.getByLabelText(/email address/i).closest('form')).toHaveAttribute('method', 'post');
  });

  it('shows link to register page', () => {
    render(<Login />);
    expect(screen.getByRole('link', { name: /create an account/i })).toBeInTheDocument();
  });

  it('validates email input', async () => {
    const { user } = render(<Login />);
    
    const emailInput = screen.getByLabelText(/email address/i);
    await user.type(emailInput, 'invalid-email');
    
    expect(emailInput).toHaveValue('invalid-email');
  });

  it('submits form with credentials', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken }) as any
    );

    const { user } = render(<Login />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/auth/login'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ email: 'test@example.com', password: 'password123' }),
        })
      );
    });
  });

  it('stores token on successful login', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken }) as any
    );

    const { user } = render(<Login />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    await waitFor(() => {
      expect(localStorage.setItem).toHaveBeenCalledWith('token', mockToken);
    });
  });

  it('stores admin metadata on successful passkey login', async () => {
    Object.defineProperty(window, 'PublicKeyCredential', {
      configurable: true,
      value: class PublicKeyCredentialMock {},
    });
    const credential = {
      id: 'credential-id',
      rawId: new Uint8Array([1, 2, 3]).buffer,
      type: 'public-key',
      response: {
        authenticatorData: new Uint8Array([4]).buffer,
        clientDataJSON: new Uint8Array([5]).buffer,
        signature: new Uint8Array([6]).buffer,
        userHandle: null,
      },
    } as unknown as PublicKeyCredential;
    Object.defineProperty(navigator, 'credentials', {
      configurable: true,
      value: {
        get: vi.fn().mockResolvedValue(credential),
      },
    });
    vi.mocked(global.fetch).mockImplementation(((url: string) => {
      if (url.includes('/passkey/login/start')) {
        return mockFetchResponse({
          options: {
            publicKey: {
              challenge: 'AQID',
              allowCredentials: [{ id: 'BAUG', type: 'public-key' }],
            },
          },
          login_id: 'attempt-1',
        });
      }
      if (url.includes('/passkey/login/finish')) {
        return mockFetchResponse({ token: mockToken, email_verified: true, is_admin: true });
      }
      return mockFetchResponse({ passkeys_enabled: true });
    }) as any);

    const { user } = render(<Login />);
    await user.type(screen.getByLabelText(/email address/i), 'admin@example.com');
    await user.click(screen.getByRole('button', { name: /sign in with passkey/i }));

    await waitFor(() => {
      expect(localStorage.setItem).toHaveBeenCalledWith('token', mockToken);
      expect(localStorage.setItem).toHaveBeenCalledWith('is_admin', 'true');
    });

    // The server keys the pending challenge by the id it handed out at start,
    // not by the email, so finish must send that id back.
    const finish = vi
      .mocked(global.fetch)
      .mock.calls.find(([url]) => String(url).includes('/passkey/login/finish'));
    expect(finish).toBeDefined();
    const sent = JSON.parse(String((finish![1] as RequestInit).body));
    expect(sent.login_id).toBe('attempt-1');
    expect(sent.credential.id).toBe('credential-id');
  });

  describe('passkey button', () => {
    const passkeyButton = () => screen.queryByRole('button', { name: /sign in with passkey/i });

    beforeEach(() => {
      Object.defineProperty(window, 'PublicKeyCredential', {
        configurable: true,
        value: class PublicKeyCredentialMock {},
      });
    });

    const settingsAnswer = (body: unknown) =>
      vi.mocked(global.fetch).mockImplementation((() => mockFetchResponse(body)) as any);

    it('is shown on an instance with passkeys enabled', async () => {
      settingsAnswer({ passkeys_enabled: true });
      render(<Login />);

      await waitFor(() => {
        expect(global.fetch).toHaveBeenCalledWith(expect.stringContaining('/auth/settings'));
      });
      expect(passkeyButton()).toBeInTheDocument();
    });

    it('is hidden, with its hint, once the instance reports passkeys disabled', async () => {
      settingsAnswer({ passkeys_enabled: false });
      render(<Login />);

      await waitFor(() => {
        expect(passkeyButton()).not.toBeInTheDocument();
      });
      expect(screen.queryByText(/use your passkey/i)).not.toBeInTheDocument();
      // The password form is all that is left, and it still works.
      expect(screen.getByRole('button', { name: /^sign in$/i })).toBeInTheDocument();
    });

    it('stays when the settings cannot be read, since the password form does not depend on them', async () => {
      vi.mocked(global.fetch).mockRejectedValue(new Error('offline'));
      render(<Login />);

      await waitFor(() => {
        expect(global.fetch).toHaveBeenCalledWith(expect.stringContaining('/auth/settings'));
      });
      expect(passkeyButton()).toBeInTheDocument();
    });
  });

  it('says why instead of bouncing back to the form when the browser blocks site storage', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken, email_verified: true }) as any
    );
    const unblock = blockSiteStorage();
    try {
      const { user } = render(<Login />);

      await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
      await user.type(screen.getByLabelText(/password/i), 'password123');
      await user.click(screen.getByRole('button', { name: /sign in/i }));

      expect(await screen.findByText(/blocking storage for this site/i)).toBeInTheDocument();
      // Without a stored token the dashboard would only send the visitor back here.
      expect(window.location.pathname).toBe('/');
    } finally {
      unblock();
    }
  });

  it('asks an unverified account to verify when the instance requires it', async () => {
    vi.mocked(global.fetch).mockImplementation(((url: string) => {
      if (url.includes('/auth/login')) {
        return mockFetchResponse({ token: mockToken, email_verified: false });
      }
      return mockFetchResponse({ require_email_verification: true, passkeys_enabled: true });
    }) as any);

    const { user } = render(<Login />, { route: '/login' });
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /^sign in$/i }));

    expect(await screen.findByRole('heading', { name: 'Verify your email' })).toBeInTheDocument();
    expect(window.location.pathname).toBe('/login');
    expect(global.fetch).toHaveBeenCalledWith(expect.stringContaining('/auth/settings'));
  });

  it('sends an unverified account to the dashboard when verification is not required', async () => {
    vi.mocked(global.fetch).mockImplementation(((url: string) => {
      if (url.includes('/auth/login')) {
        return mockFetchResponse({ token: mockToken, email_verified: false });
      }
      return mockFetchResponse({ require_email_verification: false, passkeys_enabled: true });
    }) as any);

    const { user } = render(<Login />, { route: '/login' });
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /^sign in$/i }));

    await waitFor(() => {
      expect(window.location.pathname).toBe('/dashboard');
    });
    expect(screen.queryByRole('heading', { name: 'Verify your email' })).not.toBeInTheDocument();
  });

  it('shows the server error when a verification email cannot be sent', async () => {
    vi.mocked(global.fetch).mockImplementation(((url: string) => {
      if (url.includes('/auth/resend-verification')) {
        return mockFetchResponse({ error: 'Email is not configured on this instance' }, 503);
      }
      if (url.includes('/auth/login')) {
        return mockFetchResponse({ token: mockToken, email_verified: false });
      }
      return mockFetchResponse({ require_email_verification: true, passkeys_enabled: true });
    }) as any);

    const { user } = render(<Login />, { route: '/login' });
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /^sign in$/i }));
    const resend = await screen.findByRole('button', { name: /resend verification email/i });
    // Sign-in stored the token; this mock's storage returns it for the resend only,
    // because a token present at mount sends the page to the dashboard.
    vi.mocked(localStorage.getItem).mockImplementationOnce((key: string) => (key === 'token' ? mockToken : null));
    await user.click(resend);

    expect(await screen.findByRole('alert')).toHaveTextContent('Email is not configured on this instance');
    expect(screen.queryByText('Email sent')).not.toBeInTheDocument();
    expect(global.fetch).toHaveBeenCalledWith(
      expect.stringContaining('/auth/resend-verification'),
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ email: 'test@example.com' }),
        // Signed in, so the server answers for this account, not anonymously.
        headers: expect.objectContaining({ Authorization: `Bearer ${mockToken}` }),
      }),
    );
  });

  it('shows error on failed login', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchError('Invalid credentials', 401) as any
    );

    const { user } = render(<Login />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'wrongpassword');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    await waitFor(() => {
      expect(screen.getByText(/invalid credentials/i)).toBeInTheDocument();
    });
  });

  // The middleware's 429: `error` names the limit, `message` says how long to wait.
  const rateLimited = () =>
    mockFetchResponse(
      {
        error: 'Too many requests',
        retry_after: 37,
        message: 'Rate limit exceeded. Please try again in 37 seconds.',
      },
      429,
    );

  it('says how long to wait when sign-in is rate limited', async () => {
    vi.mocked(global.fetch).mockImplementation(((url: string) =>
      String(url).includes('/auth/login') ? rateLimited() : mockFetchResponse({})) as any);

    const { user } = render(<Login />);
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    expect(await screen.findByText('Rate limit exceeded. Please try again in 37 seconds.')).toBeInTheDocument();
    expect(screen.queryByText('Too many requests')).not.toBeInTheDocument();
  });

  it('shows a plain failure, not a parser error, when the answer is not JSON', async () => {
    vi.mocked(global.fetch).mockImplementation(((url: string) =>
      String(url).includes('/auth/login')
        ? Promise.resolve({
            ok: false,
            status: 502,
            json: () => Promise.reject(new SyntaxError('Unexpected token < in JSON at position 0')),
          })
        : mockFetchResponse({})) as any);

    const { user } = render(<Login />);
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /sign in/i }));

    expect(await screen.findByText('Login failed')).toBeInTheDocument();
    expect(screen.queryByText(/unexpected token/i)).not.toBeInTheDocument();
  });

  it('says how long to wait when a passkey sign-in is rate limited', async () => {
    Object.defineProperty(window, 'PublicKeyCredential', {
      configurable: true,
      value: class PublicKeyCredentialMock {},
    });
    vi.mocked(global.fetch).mockImplementation(((url: string) =>
      String(url).includes('/passkey/login/start') ? rateLimited() : mockFetchResponse({})) as any);

    const { user } = render(<Login />);
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.click(await screen.findByRole('button', { name: /sign in with passkey/i }));

    expect(await screen.findByText('Rate limit exceeded. Please try again in 37 seconds.')).toBeInTheDocument();
  });

  it('disables button during submission', async () => {
    vi.mocked(global.fetch).mockImplementation(() => 
      new Promise(resolve => setTimeout(() => resolve(mockFetchResponse({ token: mockToken }) as any), 100))
    );

    const { user } = render(<Login />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    
    const button = screen.getByRole('button', { name: /sign in/i });
    await user.click(button);

    expect(button).toBeDisabled();
  });
});





