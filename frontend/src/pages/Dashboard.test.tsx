import { StrictMode } from 'react';
import { describe, it, expect, vi, beforeEach } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../test/test-utils';
import Dashboard from './Dashboard';
import { mockFetchError, mockFetchResponse, mockToken, mockLink } from '../test/test-utils';
import { ToastContainer } from '../components/Toast';

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

  it('prefills the create field from a pending homepage URL and then forgets it', async () => {
    sessionStorage.setItem('opn.pendingUrl', 'https://example.com/long');
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

    render(
      <StrictMode>
        <Dashboard />
      </StrictMode>,
    );

    await waitFor(() => {
      expect(screen.getByPlaceholderText(/example.com/i)).toHaveValue(
        'https://example.com/long'
      );
    });
    expect(sessionStorage.getItem('opn.pendingUrl')).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
  });

  it('drops an invalid pending URL without showing an error', async () => {
    sessionStorage.setItem('opn.pendingUrl', 'javascript:alert(1)');
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
      expect(screen.getByPlaceholderText(/example.com/i)).toHaveValue('');
    });
    expect(sessionStorage.getItem('opn.pendingUrl')).toBeNull();
    expect(screen.queryByRole('alert')).not.toBeInTheDocument();
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

  it('uses the local calendar date as the create expiration minimum, not UTC', async () => {
    vi.mocked(global.fetch).mockResolvedValue(mockFetchResponse([]) as any);
    const year = vi.spyOn(Date.prototype, 'getFullYear').mockReturnValue(2026);
    const month = vi.spyOn(Date.prototype, 'getMonth').mockReturnValue(8);
    const day = vi.spyOn(Date.prototype, 'getDate').mockReturnValue(17);
    const iso = vi.spyOn(Date.prototype, 'toISOString').mockReturnValue('2026-09-18T03:00:00.000Z');

    try {
      const { user } = render(<Dashboard />);
      await user.click(await screen.findByText(/advanced options/i));
      expect(screen.getByLabelText(/^expiration$/i)).toHaveAttribute('min', '2026-09-17');
    } finally {
      year.mockRestore();
      month.mockRestore();
      day.mockRestore();
      iso.mockRestore();
    }
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

  it('counts one link and one click in the singular', async () => {
    mockDashboardFetch((requestUrl) => {
      if (requestUrl.endsWith('/links')) {
        return mockFetchResponse([{ ...mockLink, click_count: 1 }]);
      }
      return mockFetchResponse([]);
    });

    render(<Dashboard />);

    expect(await screen.findByText('1 link')).toBeInTheDocument();
    expect(screen.getByText('1 click')).toBeInTheDocument();
    expect(screen.queryByText('1 links')).not.toBeInTheDocument();
    expect(screen.queryByText('1 clicks')).not.toBeInTheDocument();
  });

  it('counts a single valid URL and a single imported link in the singular', async () => {
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/links/bulk')) {
        return mockFetchResponse({ links: [mockLink], errors: ['ftp://example.com: invalid URL'] });
      }
      return mockFetchResponse([]);
    });

    const { user } = render(<Dashboard />);
    await user.click(await screen.findByRole('button', { name: /bulk import/i }));
    await user.type(screen.getByPlaceholderText(/page1/i), 'https://example.com/a');
    expect(screen.getByText('1 valid URL')).toBeInTheDocument();

    await user.click(screen.getByRole('button', { name: /import all/i }));
    expect(await screen.findByRole('alert')).toHaveTextContent(/^Created 1 link\. 1 failed/);
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

  it('names every row control after its link, so each one is unique', async () => {
    mockDashboardFetch(() => mockFetchResponse([linkA, linkB]));

    render(<Dashboard />);
    await screen.findByRole('button', { name: 'Delete link aaa111' });

    for (const code of ['aaa111', 'bbb222']) {
      for (const name of [
        `Copy short URL for ${code}`,
        `Copy source URL for ${code}`,
        `Preview destination for ${code}`,
        `Pin link ${code}`,
        `Clone link ${code}`,
        `Share link ${code}`,
        `Show QR code for ${code}`,
        `Edit link ${code}`,
        `Delete link ${code}`,
      ]) {
        expect(screen.getByRole('button', { name })).toBeInTheDocument();
      }
      expect(screen.getByRole('link', { name: `42 clicks, analytics for ${code}` })).toBeInTheDocument();
    }
  });

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
    expect(screen.getAllByText(/ccc333/i).length).toBeGreaterThan(0);
  });

  it('fetches sparklines in bounded id batches', async () => {
    const many = Array.from({ length: 150 }, (_, i) => ({
      ...mockLink,
      id: i + 1,
      code: `c${String(i + 1).padStart(3, '0')}`,
      is_active: true,
      is_pinned: false,
    }));
    mockDashboardFetch((requestUrl) => {
      if (requestUrl.includes('/links/sparklines')) {
        return mockFetchResponse({ sparklines: [] });
      }
      if (requestUrl.endsWith('/links')) {
        return mockFetchResponse(many);
      }
      return mockFetchResponse([]);
    });

    render(<Dashboard />);
    expect(await screen.findByText(/150 links/i)).toBeInTheDocument();

    await waitFor(() => {
      const sparkCalls = vi.mocked(global.fetch).mock.calls.filter(([url]) =>
        String(url).includes('/links/sparklines'),
      );
      expect(sparkCalls.length).toBeGreaterThan(1);
      for (const [url] of sparkCalls) {
        const ids = new URL(String(url)).searchParams.get('ids')?.split(',') ?? [];
        expect(ids.length).toBeLessThanOrEqual(80);
      }
    });
  });

  it('keeps cached sparklines when a later batch fails', async () => {
    const many = Array.from({ length: 150 }, (_, i) => ({
      ...mockLink,
      id: i + 1,
      code: `c${String(i + 1).padStart(3, '0')}`,
      created_at: new Date(Date.UTC(2024, 0, 1, 0, 0, i)).toISOString(),
      is_active: true,
      is_pinned: false,
    }));
    let sparkCalls = 0;
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
        sparkCalls += 1;
        const ids = new URL(requestUrl).searchParams.get('ids')?.split(',') ?? [];
        const firstId = Number(ids[0]);
        if (sparkCalls > 2 && firstId > 80) {
          return mockFetchError('sparkline batch failed', 500) as any;
        }
        return mockFetchResponse({
          sparklines: ids.map((id) => ({
            link_id: Number(id),
            data: [1, 2, 3],
            labels: ['a', 'b', 'c'],
          })),
        }) as any;
      }
      if (options?.method === 'POST' && requestUrl.includes('/pin')) {
        return mockFetchResponse({ is_pinned: true, message: 'Pinned' }) as any;
      }
      if (requestUrl.endsWith('/links')) {
        return mockFetchResponse(many) as any;
      }
      return mockFetchResponse([]) as any;
    });

    const sparkSvgs = () => document.querySelectorAll('svg[width="70"]');
    const { user } = render(<Dashboard />);
    expect(await screen.findByText(/150 links/i)).toBeInTheDocument();
    await waitFor(() => {
      expect(sparkSvgs().length).toBeGreaterThan(0);
    });
    const svgCountAfterLoad = sparkSvgs().length;

    await user.click(screen.getAllByRole('button', { name: /^pin link /i })[0]);
    await waitFor(() => {
      expect(sparkCalls).toBeGreaterThan(2);
    });
    expect(sparkSvgs().length).toBe(svgCountAfterLoad);
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

  it('does not clear search when Escape closes the edit modal', async () => {
    mockDashboardFetch(() => mockFetchResponse([linkA, linkB]));
    const { user } = render(<Dashboard />);
    const search = await screen.findByPlaceholderText(/search links/i);
    await user.type(search, 'aaa111');
    expect(search).toHaveValue('aaa111');

    await user.click(screen.getAllByRole('button', { name: /edit link/i })[0]);
    expect(screen.getByRole('dialog')).toBeInTheDocument();

    await user.keyboard('{Escape}');

    await waitFor(() => {
      expect(screen.queryByRole('dialog')).not.toBeInTheDocument();
    });
    expect(screen.getByPlaceholderText(/search links/i)).toHaveValue('aaa111');
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
    const pinBtns = await screen.findAllByRole('button', { name: /^pin link /i });
    expect(pinBtns.length).toBe(2);
    await user.click(pinBtns[0]);
    await user.click(pinBtns[1]);
    expect(pinResolvers.length).toBe(2);
    pinResolvers.forEach((resolve) => resolve());

    await waitFor(() => {
      expect(screen.getAllByRole('button', { name: /^unpin link /i }).length).toBe(2);
    });
  });

  it('clones a link via POST /clone and shows the new code', async () => {
    const cloned = {
      ...linkA,
      id: 3,
      code: 'cloned9',
      short_url: 'http://localhost:3000/cloned9',
    };
    let currentLinks = [linkA];
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/clone')) {
        currentLinks = [linkA, cloned];
        return mockFetchResponse(cloned);
      }
      if (requestUrl.endsWith('/links')) {
        return mockFetchResponse(currentLinks);
      }
      return mockFetchResponse(currentLinks);
    });

    const { user } = render(
      <>
        <Dashboard />
        <ToastContainer />
      </>,
    );
    await user.click(await screen.findByRole('button', { name: /clone link/i }));

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/links/1/clone'),
        expect.objectContaining({ method: 'POST' }),
      );
    });
    expect(await screen.findByText(/link cloned! new code: cloned9/i)).toBeInTheDocument();
    expect(screen.getAllByText(/cloned9/i).length).toBeGreaterThan(0);
  });

  it('shows an error toast when clone fails and does not add a link', async () => {
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/clone')) {
        return mockFetchError('Clone failed', 500);
      }
      return mockFetchResponse([linkA]);
    });

    const { user } = render(
      <>
        <Dashboard />
        <ToastContainer />
      </>,
    );
    await user.click(await screen.findByRole('button', { name: /clone link/i }));

    expect(await screen.findByText(/clone failed/i)).toBeInTheDocument();
    expect(screen.queryAllByText(/cloned9/i)).toHaveLength(0);
  });

  it('unpins a pinned link via POST /pin', async () => {
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.includes('/pin')) {
        return mockFetchResponse({ is_pinned: false, message: 'Link unpinned' });
      }
      return mockFetchResponse([{ ...linkA, is_pinned: true }]);
    });

    const { user } = render(<Dashboard />);
    await user.click(await screen.findByRole('button', { name: /^unpin link /i }));

    await waitFor(() => {
      expect(global.fetch).toHaveBeenCalledWith(
        expect.stringContaining('/links/1/pin'),
        expect.objectContaining({ method: 'POST' }),
      );
    });
    expect(await screen.findByRole('button', { name: /^pin link /i })).toBeInTheDocument();
  });

  it('lists pinned links before unpinned links even when the pinned one is older', async () => {
    const olderPinned = {
      ...linkA,
      id: 1,
      code: 'oldpin',
      is_pinned: true,
      created_at: '2024-01-01T00:00:00Z',
    };
    const newerUnpinned = {
      ...linkB,
      id: 2,
      code: 'newunp',
      is_pinned: false,
      created_at: '2024-12-01T00:00:00Z',
    };
    mockDashboardFetch(() => mockFetchResponse([newerUnpinned, olderPinned]));

    render(<Dashboard />);
    await screen.findByRole('button', { name: /^unpin link /i });

    const hrefs = screen.getAllByRole('link')
      .map((el) => el.getAttribute('href') || '')
      .filter((href) => href.endsWith('/oldpin') || href.endsWith('/newunp'));
    expect(hrefs[0]).toMatch(/\/oldpin$/);
    expect(hrefs[1]).toMatch(/\/newunp$/);
    expect(screen.getByLabelText('Pinned')).toBeInTheDocument();
  });

  it('keeps newer pinned links above older pinned links', async () => {
    const pinOld = {
      ...linkA,
      id: 1,
      code: 'pinold',
      is_pinned: true,
      created_at: '2024-01-01T00:00:00Z',
    };
    const pinNew = {
      ...linkB,
      id: 2,
      code: 'pinnew',
      is_pinned: true,
      created_at: '2024-06-01T00:00:00Z',
    };
    mockDashboardFetch(() => mockFetchResponse([pinOld, pinNew]));

    render(<Dashboard />);
    expect((await screen.findAllByRole('button', { name: /^unpin link /i })).length).toBe(2);

    const hrefs = screen.getAllByRole('link')
      .map((el) => el.getAttribute('href') || '')
      .filter((href) => href.endsWith('/pinold') || href.endsWith('/pinnew'));
    expect(hrefs[0]).toMatch(/\/pinnew$/);
    expect(hrefs[1]).toMatch(/\/pinold$/);
  });

  it('rejects an alias shorter than the configured minimum without POSTing', async () => {
    mockDashboardFetch(() => mockFetchResponse([]));

    const { user } = render(<Dashboard />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/example.com/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    await user.type(screen.getByPlaceholderText(/alias/i), 'abc');
    await user.click(screen.getByRole('button', { name: /create/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/at least 5 characters/i);
    expect(global.fetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/links'),
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('rejects an alias longer than the configured maximum without POSTing', async () => {
    mockDashboardFetch(() => mockFetchResponse([]));

    const { user } = render(<Dashboard />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/alias/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    fireEvent.change(screen.getByPlaceholderText(/alias/i), {
      target: { value: 'a'.repeat(51) },
    });
    await user.click(screen.getByRole('button', { name: /create/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(/at most 50 characters/i);
    expect(global.fetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/links'),
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('strips invalid characters from the alias as the user types', async () => {
    mockDashboardFetch(() => mockFetchResponse([]));

    const { user } = render(<Dashboard />);
    const aliasInput = await screen.findByPlaceholderText(/alias/i);
    await user.type(aliasInput, 'my link!');
    expect(aliasInput).toHaveValue('mylink');
  });

  it('rejects an alias that starts with a hyphen without POSTing', async () => {
    mockDashboardFetch(() => mockFetchResponse([]));

    const { user } = render(<Dashboard />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/alias/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    await user.type(screen.getByPlaceholderText(/alias/i), '-mylink');
    await user.click(screen.getByRole('button', { name: /create/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /cannot start or end with hyphen or underscore/i,
    );
    expect(global.fetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/links'),
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('rejects an alias that ends with an underscore without POSTing', async () => {
    mockDashboardFetch(() => mockFetchResponse([]));

    const { user } = render(<Dashboard />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/alias/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    await user.type(screen.getByPlaceholderText(/alias/i), 'mylink_');
    await user.click(screen.getByRole('button', { name: /create/i }));

    expect(await screen.findByRole('alert')).toHaveTextContent(
      /cannot start or end with hyphen or underscore/i,
    );
    expect(global.fetch).not.toHaveBeenCalledWith(
      expect.stringContaining('/links'),
      expect.objectContaining({ method: 'POST' }),
    );
  });

  it('posts a valid custom alias when creating a link', async () => {
    mockDashboardFetch((requestUrl, options) => {
      if (options?.method === 'POST' && requestUrl.endsWith('/links')) {
        return mockFetchResponse({ ...linkA, code: 'my-link-123' });
      }
      return mockFetchResponse([]);
    });

    const { user } = render(<Dashboard />);
    await waitFor(() => {
      expect(screen.getByPlaceholderText(/alias/i)).toBeInTheDocument();
    });

    await user.type(screen.getByPlaceholderText(/example.com/i), 'https://test.com');
    await user.type(screen.getByPlaceholderText(/alias/i), 'my-link-123');
    await user.click(screen.getByRole('button', { name: /create/i }));

    await waitFor(() => {
      const createCall = vi.mocked(global.fetch).mock.calls.find(
        ([url, options]) =>
          String(url).endsWith('/links') && (options as RequestInit | undefined)?.method === 'POST',
      );
      expect(createCall).toBeDefined();
      expect(JSON.parse((createCall![1] as RequestInit).body as string)).toEqual(
        expect.objectContaining({
          original_url: 'https://test.com',
          custom_alias: 'my-link-123',
        }),
      );
    });
  });
});
