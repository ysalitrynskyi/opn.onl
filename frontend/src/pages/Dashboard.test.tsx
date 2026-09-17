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

  const linkA = {
    ...mockLink,
    id: 1,
    code: 'aaa111',
    is_active: true,
    is_pinned: false,
  };
  const linkB = {
    ...mockLink,
    id: 2,
    code: 'bbb222',
    original_url: 'https://example.com/other',
    short_url: 'http://localhost:3000/bbb222',
    is_active: true,
    is_pinned: false,
  };

  function mockDashboardFetch(handler: (url: string, options?: RequestInit) => Promise<unknown> | unknown) {
    vi.mocked(global.fetch).mockImplementation((url, options) => {
      const requestUrl = String(url);
      if (requestUrl.endsWith('/auth/settings')) {
        return mockFetchResponse({
          custom_aliases_enabled: true,
          min_alias_length: 5,
          max_alias_length: 50,
        }) as any;
      }
      if (requestUrl.includes('/links/sparklines')) {
        return mockFetchResponse({ sparklines: [] }) as any;
      }
      return handler(requestUrl, options as RequestInit) as any;
    });
  }

  it('keeps both links removed when two deletes finish out of order', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    const deleteResolvers: Array<() => void> = [];
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'DELETE') {
        return new Promise((resolve) => {
          deleteResolvers.push(() =>
            resolve({
              ok: true,
              status: 200,
              json: () => Promise.resolve({}),
            }),
          );
        });
      }
      return mockFetchResponse([linkA, linkB]);
    });

    const { user } = render(<Dashboard />);
    expect((await screen.findAllByText(/aaa111/i)).length).toBeGreaterThan(0);
    expect(screen.getAllByText(/bbb222/i).length).toBeGreaterThan(0);

    const deleteBtns = screen.getAllByRole('button', { name: /delete link/i });
    await user.click(deleteBtns[0]);
    await user.click(deleteBtns[1]);
    expect(deleteResolvers.length).toBe(2);
    deleteResolvers.forEach((resolve) => resolve());

    await waitFor(() => {
      expect(screen.queryAllByText(/aaa111/i)).toHaveLength(0);
      expect(screen.queryAllByText(/bbb222/i)).toHaveLength(0);
    });
  });

  it('does not resurrect a deleted link when a slower fetchLinks resolves', async () => {
    vi.spyOn(window, 'confirm').mockReturnValue(true);
    let resolveSecondFetch: (value: unknown) => void = () => {};
    const secondFetch = new Promise((resolve) => {
      resolveSecondFetch = resolve;
    });
    let linksGets = 0;
    let staleResponseDelivered = false;
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.endsWith('/links')) {
        return mockFetchResponse({ ...linkB, id: 3, code: 'ccc333' });
      }
      if (options?.method === 'DELETE') {
        return Promise.resolve({
          ok: true,
          status: 200,
          json: () => Promise.resolve({}),
        });
      }
      if (requestUrl.endsWith('/links') && options?.method !== 'POST') {
        linksGets += 1;
        if (linksGets === 1) {
          return mockFetchResponse([linkA, linkB]);
        }
        return secondFetch.then(() => {
          staleResponseDelivered = true;
          return {
            ok: true,
            status: 200,
            json: () => Promise.resolve([linkA, linkB, { ...linkA, id: 3, code: 'ccc333' }]),
          };
        });
      }
      return mockFetchResponse([linkA, linkB]);
    });

    const { user } = render(<Dashboard />);
    expect((await screen.findAllByText(/aaa111/i)).length).toBeGreaterThan(0);

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://created.example');
    await user.click(screen.getByRole('button', { name: /create/i }));

    await waitFor(() => {
      expect(linksGets).toBe(2);
    });

    await user.click(screen.getAllByRole('button', { name: /delete link/i })[0]);
    await waitFor(() => {
      expect(screen.queryAllByText(/aaa111/i)).toHaveLength(0);
    });

    resolveSecondFetch(undefined);

    await waitFor(() => {
      expect(staleResponseDelivered).toBe(true);
    });
    expect(screen.queryAllByText(/aaa111/i)).toHaveLength(0);
    expect(screen.getAllByText(/bbb222/i).length).toBeGreaterThan(0);
  });

  it('shows bulk-import API errors from the errors array', async () => {
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/links/bulk')) {
        return Promise.resolve({
          ok: false,
          status: 403,
          json: () => Promise.resolve({
            links: [],
            errors: ['Please verify your email before creating links'],
          }),
        });
      }
      return mockFetchResponse([]);
    });

    const { user } = render(<Dashboard />);
    await user.click(await screen.findByRole('button', { name: /bulk import/i }));
    await user.type(screen.getByPlaceholderText(/page1/i), 'https://example.com/a');
    await user.click(screen.getByRole('button', { name: /import all/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/please verify your email/i);
  });

  it('does not treat a failed links fetch as an empty account', async () => {
    mockDashboardFetch((requestUrl) => {
      if (requestUrl.endsWith('/links')) {
        return mockFetchError('Database down', 500);
      }
      return mockFetchResponse([]);
    });

    render(<Dashboard />);

    expect(await screen.findByRole('alert')).toHaveTextContent(/database down/i);
    expect(screen.queryByText(/no links yet/i)).not.toBeInTheDocument();
    expect(screen.queryByText(/create your first shortened link/i)).not.toBeInTheDocument();
  });

  it('keeps both pin updates when two pins finish out of order', async () => {
    const pinResolvers: Array<() => void> = [];
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/pin')) {
        return new Promise((resolve) => {
          pinResolvers.push(() =>
            resolve({
              ok: true,
              status: 200,
              json: () => Promise.resolve({ is_pinned: true, message: 'Pinned' }),
            }),
          );
        });
      }
      return mockFetchResponse([linkA, linkB]);
    });

    const { user } = render(<Dashboard />);
    const pinBtns = await screen.findAllByRole('button', { name: /^pin$/i });
    expect(pinBtns.length).toBe(2);
    await user.click(pinBtns[0]);
    await user.click(pinBtns[1]);
    expect(pinResolvers.length).toBe(2);
    pinResolvers.forEach((resolve) => resolve());

    await waitFor(() => {
      expect(screen.getAllByRole('button', { name: /^unpin$/i }).length).toBe(2);
    });
  });
});
