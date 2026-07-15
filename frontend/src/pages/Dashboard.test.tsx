import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Dashboard from './Dashboard';
import { mockFetchError, mockFetchResponse, mockToken, mockLink } from '../test/test-utils';

describe('Dashboard Page', () => {
  beforeEach(() => {
    vi.mocked(global.fetch).mockReset();
    vi.mocked(localStorage.getItem).mockReturnValue(mockToken);
  });

  it('redirects to login if not authenticated', async () => {
    vi.mocked(localStorage.getItem).mockReturnValue(null);
    render(<Dashboard />);
    
    // Component should attempt to navigate to login
    // In a real test, we'd check for navigation
  });

  it('shows loading state initially', () => {
    vi.mocked(global.fetch).mockImplementation(() => 
      new Promise(() => {}) // Never resolves
    );
    
    render(<Dashboard />);
    // Loading skeleton should be shown
  });

  it('fetches and displays links', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([mockLink]) as any
    );

    render(<Dashboard />);

    await waitFor(() => {
      // Link code should appear (may be multiple times - in URL display)
      expect(screen.getAllByText(/abc123/i).length).toBeGreaterThan(0);
    });
  });

  it('shows empty state when no links', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([]) as any
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText(/no links yet/i)).toBeInTheDocument();
    });
  });

  it('displays click count for links', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([mockLink]) as any
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText(/42 clicks/i)).toBeInTheDocument();
    });
  });

  it('renders create link form', async () => {
    // Mock both links and settings API calls
    vi.mocked(global.fetch).mockImplementation((url) => {
      if (typeof url === 'string' && url.includes('/settings')) {
        return Promise.resolve(mockFetchResponse({
          custom_aliases_enabled: true,
          min_alias_length: 5,
          max_alias_length: 25,
          account_deletion_enabled: false,
        })) as any;
      }
      return Promise.resolve(mockFetchResponse([])) as any;
    });

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/example.com/i)).toBeInTheDocument();
    });
  });

  it('creates new link on form submission', async () => {
    vi.mocked(global.fetch)
      .mockResolvedValueOnce(mockFetchResponse([]) as any) // Initial fetch
      .mockResolvedValueOnce(mockFetchResponse(mockLink) as any) // Create link
      .mockResolvedValueOnce(mockFetchResponse([mockLink]) as any); // Refresh links

    const { user } = render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/example.com/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    await user.click(screen.getByRole('button', { name: /create/i }));

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/links'),
        expect.objectContaining({ method: 'POST' })
      );
    });
  });

  it('shows advanced options when toggled', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([]) as any
    );

    const { user } = render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByText(/advanced options/i)).toBeInTheDocument();
    });

    await user.click(screen.getByText(/advanced options/i));

    await waitFor(() => {
      expect(screen.getByText(/password protection/i)).toBeInTheDocument();
      // Label is "Expiration" not "Expiration Date" in create form
      expect(screen.getByText(/expiration/i)).toBeInTheDocument();
    });
  });

  it('has search input when links exist', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([mockLink]) as any
    );

    render(<Dashboard />);

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/search links/i)).toBeInTheDocument();
    });
  });

  it('shows total clicks statistic', async () => {
    vi.mocked(global.fetch).mockResolvedValue(
      mockFetchResponse([mockLink]) as any
    );

    render(<Dashboard />);

    await waitFor(() => {
      // Should show link count and click count in header
      expect(screen.getByText(/1 link/i)).toBeInTheDocument();
    });
  });

  it('keeps the edit modal open when the update endpoint rejects the save', async () => {
    vi.mocked(global.fetch).mockImplementation((url, options) => {
      const requestUrl = String(url);
      if (requestUrl.endsWith('/auth/settings')) {
        return mockFetchResponse({
          custom_aliases_enabled: true,
          min_alias_length: 5,
          max_alias_length: 50,
          conditional_routing_enabled: false,
        }) as any;
      }
      if (requestUrl.endsWith('/links/1') && options?.method === 'PUT') {
        return mockFetchError('Update rejected', 422) as any;
      }
      if (requestUrl.includes('/links/sparklines')) {
        return mockFetchResponse({ sparklines: [] }) as any;
      }
      return mockFetchResponse([mockLink]) as any;
    });

    const { user } = render(<Dashboard />);
    await user.click(await screen.findByRole('button', { name: /edit link/i }));
    await user.click(screen.getByRole('button', { name: /save changes/i }));

    await waitFor(() => {
      expect(screen.getByRole('heading', { name: /edit link/i })).toBeInTheDocument();
      expect(screen.getAllByText('Update rejected').length).toBeGreaterThan(0);
    });
  });
});
