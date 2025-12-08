import type { ReactElement } from 'react';
import React from 'react';
import { render } from '@testing-library/react';
import type { RenderOptions } from '@testing-library/react';
import { BrowserRouter } from 'react-router-dom';
import { HelmetProvider } from 'react-helmet-async';
import userEvent from '@testing-library/user-event';

// Custom render with providers
interface CustomRenderOptions extends Omit<RenderOptions, 'wrapper'> {
  route?: string;
}

function AllProviders({ children }: { children: React.ReactNode }) {
  return (
    <HelmetProvider>
      <BrowserRouter>
        {children}
      </BrowserRouter>
    </HelmetProvider>
  );
}

function customRender(
  ui: ReactElement,
  options?: CustomRenderOptions
) {
  const { route = '/', ...renderOptions } = options || {};
  
  window.history.pushState({}, 'Test page', route);
  
  return {
    user: userEvent.setup(),
    ...render(ui, { wrapper: AllProviders, ...renderOptions }),
  };
}

// Re-export commonly used items from testing-library
import { screen, waitFor, within, fireEvent } from '@testing-library/react';
export { screen, waitFor, within, fireEvent };
export { customRender as render };
export { userEvent };

// Helper to create mock fetch responses
export function mockFetchResponse(data: unknown, status = 200) {
  return Promise.resolve({
    ok: status >= 200 && status < 300,
    status,
    json: () => Promise.resolve(data),
  });
}

// Helper to create mock API error response
export function mockFetchError(error: string, status = 400) {
  return Promise.resolve({
    ok: false,
    status,
    json: () => Promise.resolve({ error }),
  });
}

// Helper for waiting
export const wait = (ms: number) => new Promise(resolve => setTimeout(resolve, ms));

// Mock token for authenticated tests
export const mockToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0QGV4YW1wbGUuY29tIiwidXNlcl9pZCI6MSwiZXhwIjoxOTk5OTk5OTk5fQ.test';

// Mock link data
export const mockLink = {
  id: 1,
  code: 'abc123',
  original_url: 'https://example.com/very-long-url',
  short_url: 'http://localhost:3000/abc123',
  click_count: 42,
  created_at: '2024-01-01T00:00:00Z',
  expires_at: null,
  has_password: false,
  tags: [],
  folder_id: null,
  notes: null,
  title: null,
};

// Mock analytics data
export const mockAnalytics = {
  total_clicks: 100,
  events: [
    {
      id: 1,
      created_at: '2024-01-01T12:00:00Z',
      ip_address: '192.168.1.1',
      user_agent: 'Mozilla/5.0 Chrome/91.0',
      referer: 'https://google.com',
      country: 'US',
    },
    {
      id: 2,
      created_at: '2024-01-02T12:00:00Z',
      ip_address: '192.168.1.2',
      user_agent: 'Mozilla/5.0 Firefox/89.0',
      referer: null,
      country: 'UK',
    },
  ],
};

