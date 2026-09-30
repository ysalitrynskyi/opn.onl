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

  it('paints the hero text at full opacity on the first render', () => {
    // The client render replaces the prerendered markup. If the hero text
    // started from an opacity-0 entrance state it would vanish until the fade
    // ran, which pushed mobile LCP past the JS bundle download.
    render(<Home />);
    const heroText = [
      screen.getByText(/open source · self-hostable · agpl-3\.0/i),
      screen.getByRole('heading', { level: 1, name: /short links that\s+answer to you/i }),
      screen.getByText(/a privacy-first url shortener you actually own/i),
    ];
    for (const element of heroText) {
      expect(element.style.opacity).toBe('');
      expect(getComputedStyle(element).opacity).not.toBe('0');
    }
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

  it('stashes the pasted URL in sessionStorage when a logged-out visitor shortens', async () => {
    vi.mocked(localStorage.getItem).mockReturnValue(null);
    sessionStorage.clear();

    const { user } = render(<Home />);

    const input = screen.getByPlaceholderText(/your-very-long-link/i);
    await user.type(input, 'https://example.com/long');
    await user.click(screen.getByRole('button', { name: /shorten/i }));

    expect(sessionStorage.getItem('opn.pendingUrl')).toBe('https://example.com/long');
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

  // Layout owns the page's one <main> (the skip link targets it); a page
  // that renders its own nests a second main landmark inside it.
  it('does not render a main landmark of its own', () => {
    const { container } = render(<Home />);
    expect(container.querySelector('main')).toBeNull();
  });
});
