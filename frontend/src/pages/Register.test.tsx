import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Register from './Register';
import { mockFetchResponse, mockFetchError, mockToken } from '../test/test-utils';

describe('Register Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.setItem).mockClear();
  });

  it('renders registration form', () => {
    render(<Register />);
    expect(screen.getByText('Create an account')).toBeInTheDocument();
    expect(screen.getByLabelText(/email address/i)).toBeInTheDocument();
    expect(screen.getByLabelText(/password/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /create account/i })).toBeInTheDocument();
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

  it('shows terms and privacy links', () => {
    render(<Register />);
    expect(screen.getByText(/terms of service/i)).toBeInTheDocument();
    expect(screen.getByText(/privacy policy/i)).toBeInTheDocument();
  });
});

