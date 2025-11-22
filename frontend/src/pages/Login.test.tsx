import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Login from './Login';
import { mockFetchResponse, mockFetchError, mockToken } from '../test/test-utils';

describe('Login Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.getItem).mockReturnValue(null);
    vi.mocked(localStorage.setItem).mockClear();
  });

  it('renders login form', () => {
    render(<Login />);
    expect(screen.getByText('Welcome back')).toBeInTheDocument();
    expect(screen.getByLabelText(/email address/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /sign in/i })).toBeInTheDocument();
  });

  it('shows link to register page', () => {
    render(<Login />);
    expect(screen.getByText(/sign up/i)).toBeInTheDocument();
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

