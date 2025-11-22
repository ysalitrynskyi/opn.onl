import { test, expect, Page } from '@playwright/test';

// Helper to set up authenticated state
async function authenticateUser(page: Page) {
  // Create a mock JWT token
  const mockToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0QGV4YW1wbGUuY29tIiwidXNlcl9pZCI6MSwiZXhwIjoxOTk5OTk5OTk5fQ.test';
  
  await page.addInitScript((token) => {
    localStorage.setItem('token', token);
  }, mockToken);
}

test.describe('Dashboard Page', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    
    // Mock the API responses
    await page.route('**/links', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify([
            {
              id: 1,
              code: 'abc123',
              original_url: 'https://example.com/very-long-url',
              short_url: 'http://localhost:3000/abc123',
              click_count: 42,
              created_at: '2024-01-01T00:00:00Z',
              expires_at: null,
              has_password: false,
            },
            {
              id: 2,
              code: 'xyz789',
              original_url: 'https://another-example.com',
              short_url: 'http://localhost:3000/xyz789',
              click_count: 15,
              created_at: '2024-01-02T00:00:00Z',
              expires_at: '2024-12-31T00:00:00Z',
              has_password: true,
            },
          ]),
        });
      } else if (route.request().method() === 'POST') {
        await route.fulfill({
          status: 201,
          contentType: 'application/json',
          body: JSON.stringify({
            id: 3,
            code: 'new123',
            original_url: 'https://new-link.com',
            short_url: 'http://localhost:3000/new123',
            click_count: 0,
            created_at: new Date().toISOString(),
            expires_at: null,
            has_password: false,
          }),
        });
      }
    });

    await page.goto('/dashboard');
  });

  test('should display dashboard header', async ({ page }) => {
    await expect(page.getByRole('heading', { name: 'Dashboard' })).toBeVisible();
  });

  test('should display link statistics', async ({ page }) => {
    await expect(page.getByText('2 links')).toBeVisible();
    await expect(page.getByText('57 total clicks')).toBeVisible();
  });

  test('should display user links', async ({ page }) => {
    await expect(page.getByText('abc123')).toBeVisible();
    await expect(page.getByText('xyz789')).toBeVisible();
  });

  test('should display click counts', async ({ page }) => {
    await expect(page.getByText('42 clicks')).toBeVisible();
    await expect(page.getByText('15 clicks')).toBeVisible();
  });

  test('should show password protected badge', async ({ page }) => {
    await expect(page.getByText('Protected')).toBeVisible();
  });

  test('should show expiration badge', async ({ page }) => {
    await expect(page.getByText(/expires/i)).toBeVisible();
  });

  test('should have create link form', async ({ page }) => {
    await expect(page.getByPlaceholder(/example.com/i)).toBeVisible();
    await expect(page.getByPlaceholder(/alias/i)).toBeVisible();
    await expect(page.getByRole('button', { name: /create/i })).toBeVisible();
  });

  test('should toggle advanced options', async ({ page }) => {
    await page.getByText('Advanced options').click();
    
    await expect(page.getByText('Password Protection')).toBeVisible();
    await expect(page.getByText('Expiration Date')).toBeVisible();
  });

  test('should filter links by search', async ({ page }) => {
    const searchInput = page.getByPlaceholder('Search links...');
    await searchInput.fill('abc');
    
    await expect(page.getByText('abc123')).toBeVisible();
    await expect(page.getByText('xyz789')).not.toBeVisible();
  });

  test('should clear search filter', async ({ page }) => {
    const searchInput = page.getByPlaceholder('Search links...');
    await searchInput.fill('abc');
    await searchInput.clear();
    
    await expect(page.getByText('abc123')).toBeVisible();
    await expect(page.getByText('xyz789')).toBeVisible();
  });

  test('should copy link to clipboard', async ({ page }) => {
    // Grant clipboard permissions
    await page.context().grantPermissions(['clipboard-write', 'clipboard-read']);
    
    await page.getByTitle('Copy to clipboard').first().click();
    
    // Check icon should appear briefly
    await expect(page.locator('svg').first()).toBeVisible();
  });

  test('should have export CSV button', async ({ page }) => {
    await expect(page.getByRole('button', { name: /export csv/i })).toBeVisible();
  });

  test('should create new link', async ({ page }) => {
    await page.getByPlaceholder(/example.com/i).fill('https://new-link.com');
    await page.getByRole('button', { name: /create/i }).click();
    
    // Wait for the link to be created (API call)
    await page.waitForResponse(response => 
      response.url().includes('/links') && response.request().method() === 'POST'
    );
  });

  test('should create link with custom alias', async ({ page }) => {
    await page.getByPlaceholder(/example.com/i).fill('https://new-link.com');
    await page.getByPlaceholder(/alias/i).fill('myalias');
    await page.getByRole('button', { name: /create/i }).click();
    
    await page.waitForResponse(response => 
      response.url().includes('/links') && response.request().method() === 'POST'
    );
  });
});

test.describe('Dashboard - Empty State', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links', async (route) => {
      if (route.request().method() === 'GET') {
        await route.fulfill({
          status: 200,
          contentType: 'application/json',
          body: JSON.stringify([]),
        });
      }
    });

    await page.goto('/dashboard');
  });

  test('should show empty state', async ({ page }) => {
    await expect(page.getByText('No links yet')).toBeVisible();
    await expect(page.getByText('Create your first shortened link')).toBeVisible();
  });
});

test.describe('Dashboard - Link Actions', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify([{
          id: 1,
          code: 'abc123',
          original_url: 'https://example.com',
          short_url: 'http://localhost:3000/abc123',
          click_count: 10,
          created_at: '2024-01-01T00:00:00Z',
          expires_at: null,
          has_password: false,
        }]),
      });
    });

    await page.route('**/links/1', async (route) => {
      if (route.request().method() === 'DELETE') {
        await route.fulfill({ status: 200, body: JSON.stringify({ message: 'Deleted' }) });
      }
    });

    await page.goto('/dashboard');
  });

  test('should navigate to analytics', async ({ page }) => {
    await page.getByText('10 clicks').click();
    await expect(page).toHaveURL('/analytics/1');
  });

  test('should open QR code modal', async ({ page }) => {
    await page.getByTitle('View QR Code').click();
    await expect(page.getByText('QR Code')).toBeVisible();
  });

  test('should open edit modal', async ({ page }) => {
    await page.getByTitle('Edit').click();
    await expect(page.getByText('Edit Link')).toBeVisible();
  });
});

