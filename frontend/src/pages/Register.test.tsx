import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Home from './Home';
import Register from './Register';
import Dashboard from './Dashboard';
import { mockFetchResponse, mockFetchError, mockToken } from '../test/test-utils';
import { blockSiteStorage } from '../test/helpers';

describe('Register Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.setItem).mockClear();
  });

  it('renders registration form', () => {
    render(<Register />);
    expect(screen.getByRole('heading', { name: /create your account/i })).toBeInTheDocument();
    expect(screen.getByLabelText(/email address/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /create account/i })).toBeInTheDocument();
  });

  it('posts the form so a native submit cannot leak credentials into the URL', () => {
    render(<Register />);
    expect(screen.getByLabelText(/email address/i).closest('form')).toHaveAttribute('method', 'post');
  });

  it('shows link to login page', () => {
    render(<Register />);
    expect(screen.getByText(/log in/i)).toBeInTheDocument();
  });

  it('shows password requirements', async () => {
    const { user } = render(<Register />);
    
    const passwordInput = screen.getByLabelText(/password/i);
    await user.type(passwordInput, 'test');

    expect(screen.getByText(/at least 8 characters/i)).toBeInTheDocument();
  });

  it('indicates when password meets requirements', async () => {
    const { user } = render(<Register />);
    
    const passwordInput = screen.getByLabelText(/password/i);
    await user.type(passwordInput, 'password123');

    // The requirement text should show as met
    const requirement = screen.getByText(/at least 8 characters/i);
    expect(requirement).toBeInTheDocument();
  });

  it('submits form with valid data', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken }) as any
    );

    const { user } = render(<Register />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /create account/i }));

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/auth/register'),
        expect.objectContaining({
          method: 'POST',
          body: JSON.stringify({ email: 'test@example.com', password: 'password123' }),
        })
      );
    });
  });

  it('stores token on successful registration', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken }) as any
    );

    const { user } = render(<Register />);
    
    await user.type(screen.getByLabelText(/email address/i), 'test@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /create account/i }));

    await waitFor(() => {
      expect(localStorage.setItem).toHaveBeenCalledWith('token', mockToken);
    });
  });

  it('shows error for duplicate email', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchError('Email already exists', 409) as any
    );

    const { user } = render(<Register />);
    
    await user.type(screen.getByLabelText(/email address/i), 'existing@example.com');
    await user.type(screen.getByLabelText(/password/i), 'password123');
    await user.click(screen.getByRole('button', { name: /create account/i }));

    await waitFor(() => {
      expect(screen.getByText(/email already exists/i)).toBeInTheDocument();
    });
  });

  it('says why the new account is not signed in when the browser blocks site storage', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken, email_verified: true }) as any
    );
    const unblock = blockSiteStorage();
    try {
      const { user } = render(<Register />);

      await user.type(screen.getByLabelText(/email address/i), 'new@example.com');
      await user.type(screen.getByLabelText(/password/i), 'password123');
      await user.click(screen.getByRole('button', { name: /create account/i }));

      expect(await screen.findByText(/blocking storage for this site/i)).toBeInTheDocument();
      expect(window.location.pathname).toBe('/');
    } finally {
      unblock();
    }
  });

  it('shows terms and privacy links', () => {
    render(<Register />);
    expect(screen.getByRole('link', { name: /terms/i })).toBeInTheDocument();
    expect(screen.getByRole('link', { name: /privacy policy/i })).toBeInTheDocument();
  });

  it('carries a logged-out shorten URL through registration onto the dashboard', async () => {
    sessionStorage.clear();

    const home = render(<Home />);
    await home.user.type(
      screen.getByPlaceholderText(/your-very-long-link/i),
      'https://example.com/long'
    );
    await home.user.click(screen.getByRole('button', { name: /shorten/i }));
    expect(sessionStorage.getItem('opn.pendingUrl')).toBe('https://example.com/long');
    home.unmount();

    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse({ token: mockToken, email_verified: true }) as any
    );

    const register = render(<Register />);
    await register.user.type(screen.getByLabelText(/email address/i), 'new@example.com');
    await register.user.type(screen.getByLabelText(/password/i), 'password123');
    await register.user.click(screen.getByRole('button', { name: /create account/i }));

    await waitFor(() => {
      expect(localStorage.setItem).toHaveBeenCalledWith('token', mockToken);
    });
    expect(sessionStorage.getItem('opn.pendingUrl')).toBe('https://example.com/long');
    register.unmount();

    vi.mocked(localStorage.getItem).mockReturnValue(mockToken);
    vi.mocked(global.fetch).mockImplementation((url) => {
      if (typeof url === 'string' && url.includes('/settings')) {
        return mockFetchResponse({
          custom_aliases_enabled: true,
          min_alias_length: 5,
          max_alias_length: 25,
        }) as any;
      }
      return mockFetchResponse([]) as any;
    });

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/example.com/i)).toHaveValue(
        'https://example.com/long'
      );
    });
    expect(sessionStorage.getItem('opn.pendingUrl')).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });
});

