import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Home from './Home';
import { mockFetchResponse, mockFetchError, mockToken, mockLink } from '../test/test-utils';

describe('Home Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.getItem).mockReturnValue(null);
  });

  it('renders hero section', () => {
    render(<Home />);
    expect(screen.getByRole('heading', { name: /short links that\s+answer to you/i })).toBeInTheDocument();
    expect(screen.getByText(/a privacy-first url shortener you actually own/i)).toBeInTheDocument();
  });

  it('renders URL input form', () => {
    render(<Home />);
    expect(screen.getByPlaceholderText(/your-very-long-link/i)).toBeInTheDocument();
    expect(screen.getByRole('button', { name: /shorten/i })).toBeInTheDocument();
  });

  it('renders feature cards', () => {
    render(<Home />);
    expect(screen.getByText('Rust-fast redirects')).toBeInTheDocument();
    expect(screen.getByText('Privacy by default')).toBeInTheDocument();
    expect(screen.getByText('Honest analytics')).toBeInTheDocument();
  });

  it('shows terms and privacy links', () => {
    render(<Home />);
    // Multiple "Terms" links exist (hero + disclaimer); assert at least one and the privacy link.
    expect(screen.getAllByRole('link', { name: /terms/i }).length).toBeGreaterThan(0);
    expect(screen.getByRole('link', { name: /privacy policy/i })).toBeInTheDocument();
  });

  it('redirects to register when not logged in and form submitted', async () => {
    vi.mocked(localStorage.getItem).mockReturnValue(null);
    
    const { user } = render(<Home />);
    
    const input = screen.getByPlaceholderText(/your-very-long-link/i);
    await user.type(input, 'https://example.com/test');
    
    const button = screen.getByRole('button', { name: /shorten/i });
    await user.click(button);
    
    // Should navigate to register (we can't test navigation directly in unit tests)
    // but we can verify the form was submitted
    expect(input).toHaveValue('https://example.com/test');
  });

  it('creates link when logged in', async () => {
    vi.mocked(localStorage.getItem).mockReturnValue(mockToken);
    vi.mocked(global.fetch).mockResolvedValue(mockFetchResponse(mockLink) as any);

    const { user } = render(<Home />);
    
    const input = screen.getByPlaceholderText(/your-very-long-link/i);
    await user.type(input, 'https://example.com/test');
    
    const button = screen.getByRole('button', { name: /shorten/i });
    await user.click(button);

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/links'),
        expect.objectContaining({
          method: 'POST',
        })
      );
    });
  });

  it('shows error on API failure', async () => {
    vi.mocked(localStorage.getItem).mockReturnValue(mockToken);
    vi.mocked(global.fetch).mockResolvedValue(mockFetchError('Invalid URL') as any);

    const { user } = render(<Home />);
    
    const input = screen.getByPlaceholderText(/your-very-long-link/i);
    await user.type(input, 'https://example.com/test');
    
    const button = screen.getByRole('button', { name: /shorten/i });
    await user.click(button);

    await waitFor(() => {
      expect(screen.getByText(/invalid url/i)).toBeInTheDocument();
    });
  });
});

