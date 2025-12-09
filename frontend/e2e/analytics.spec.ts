import { test, expect, Page } from '@playwright/test';

async function authenticateUser(page: Page) {
  const mockToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0QGV4YW1wbGUuY29tIiwidXNlcl9pZCI6MSwiZXhwIjoxOTk5OTk5OTk5fQ.test';
  
  await page.addInitScript((token) => {
    localStorage.setItem('token', token);
  }, mockToken);
}

const mockAnalyticsData = {
  total_clicks: 150,
  events: [
    {
      id: 1,
      created_at: new Date().toISOString(),
      ip_address: '192.168.1.1',
      user_agent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Chrome/91.0',
      referer: 'https://twitter.com',
      country: 'US',
    },
    {
      id: 2,
      created_at: new Date(Date.now() - 86400000).toISOString(), // Yesterday
      ip_address: '192.168.1.2',
      user_agent: 'Mozilla/5.0 (iPhone; CPU iPhone OS) Mobile Safari',
      referer: 'https://google.com',
      country: 'UK',
    },
    {
      id: 3,
      created_at: new Date(Date.now() - 172800000).toISOString(), // 2 days ago
      ip_address: '192.168.1.3',
      user_agent: 'Mozilla/5.0 Firefox/89.0',
      referer: null,
      country: 'DE',
    },
  ],
};

test.describe('Analytics Page', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links/1/stats', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(mockAnalyticsData),
      });
    });

    await page.goto('/analytics/1');
  });

  test('should display page title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /link analytics/i })).toBeVisible();
  });

  test('should display back button', async ({ page }) => {
    await expect(page.getByRole('link', { name: /back to dashboard/i })).toBeVisible();
  });

  test('should display total clicks', async ({ page }) => {
    await expect(page.getByText('150')).toBeVisible();
    await expect(page.getByText('Total Clicks')).toBeVisible();
  });

  test('should display today clicks', async ({ page }) => {
    await expect(page.getByText('Today')).toBeVisible();
  });

  test('should display 7-day clicks', async ({ page }) => {
    await expect(page.getByText('Last 7 Days')).toBeVisible();
  });

  test('should display click trend chart', async ({ page }) => {
    await expect(page.getByText('Click Trend')).toBeVisible();
  });

  test('should display devices breakdown', async ({ page }) => {
    await expect(page.getByText('Devices')).toBeVisible();
  });

  test('should display browsers breakdown', async ({ page }) => {
    await expect(page.getByText('Browsers')).toBeVisible();
  });

  test('should display top referrers', async ({ page }) => {
    await expect(page.getByText('Top Referrers')).toBeVisible();
    await expect(page.getByText('twitter.com')).toBeVisible();
  });

  test('should display recent clicks table', async ({ page }) => {
    await expect(page.getByText('Recent Clicks')).toBeVisible();
    await expect(page.getByText('Time')).toBeVisible();
    await expect(page.getByText('Device')).toBeVisible();
  });

  test('should navigate back to dashboard', async ({ page }) => {
    await page.getByRole('link', { name: /back to dashboard/i }).click();
    await expect(page).toHaveURL('/dashboard');
  });
});

test.describe('Analytics Page - Empty State', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links/1/stats', async (route) => {
      await route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify({
          total_clicks: 0,
          events: [],
        }),
      });
    });

    await page.goto('/analytics/1');
  });

  test('should show zero clicks', async ({ page }) => {
    await expect(page.getByText('0').first()).toBeVisible();
  });

  test('should show no clicks message', async ({ page }) => {
    await expect(page.getByText(/no clicks recorded/i)).toBeVisible();
  });
});

test.describe('Analytics Page - Error Handling', () => {
  test('should show error for non-existent link', async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links/999/stats', async (route) => {
      await route.fulfill({
        status: 404,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Link not found' }),
      });
    });

    await page.goto('/analytics/999');
    
    await expect(page.getByText(/link not found/i)).toBeVisible();
  });

  test('should show error for unauthorized access', async ({ page }) => {
    await authenticateUser(page);
    
    await page.route('**/links/1/stats', async (route) => {
      await route.fulfill({
        status: 403,
        contentType: 'application/json',
        body: JSON.stringify({ error: 'Forbidden' }),
      });
    });

    await page.goto('/analytics/1');
    
    await expect(page.getByText(/permission/i)).toBeVisible();
  });
});





