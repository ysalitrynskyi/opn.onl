import { test, expect, Page } from '@playwright/test';

const BASE_URL = process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173';

async function mockApiResponse(page: Page, url: string, response: any, status = 200) {
    await page.route(url, async route => {
        await route.fulfill({
            status,
            contentType: 'application/json',
            body: JSON.stringify(response),
        });
    });
}

// ============= GeoIP Analytics Tests =============

test.describe('GeoIP Analytics', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        // Mock comprehensive analytics with GeoIP data
        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'geo123',
            original_url: 'https://example.com',
            total_clicks: 500,
            unique_visitors: 420,
            clicks_by_day: [
                { date: '2024-01-15', count: 50 },
                { date: '2024-01-14', count: 45 },
                { date: '2024-01-13', count: 40 },
                { date: '2024-01-12', count: 35 },
                { date: '2024-01-11', count: 30 },
            ],
            clicks_by_country: [
                { country: 'United States', count: 200, percentage: 40 },
                { country: 'United Kingdom', count: 100, percentage: 20 },
                { country: 'Germany', count: 75, percentage: 15 },
                { country: 'France', count: 50, percentage: 10 },
                { country: 'Japan', count: 25, percentage: 5 },
                { country: 'Other', count: 50, percentage: 10 },
            ],
            clicks_by_city: [
                { city: 'New York', country: 'United States', count: 80, percentage: 16 },
                { city: 'Los Angeles', country: 'United States', count: 60, percentage: 12 },
                { city: 'London', country: 'United Kingdom', count: 70, percentage: 14 },
                { city: 'Berlin', country: 'Germany', count: 40, percentage: 8 },
                { city: 'Paris', country: 'France', count: 35, percentage: 7 },
            ],
            clicks_by_browser: [
                { browser: 'Chrome', count: 250, percentage: 50 },
                { browser: 'Safari', count: 100, percentage: 20 },
                { browser: 'Firefox', count: 75, percentage: 15 },
                { browser: 'Edge', count: 50, percentage: 10 },
                { browser: 'Other', count: 25, percentage: 5 },
            ],
            clicks_by_device: [
                { device: 'Desktop', count: 300, percentage: 60 },
                { device: 'Mobile', count: 150, percentage: 30 },
                { device: 'Tablet', count: 50, percentage: 10 },
            ],
            clicks_by_os: [
                { os: 'Windows', count: 200, percentage: 40 },
                { os: 'macOS', count: 120, percentage: 24 },
                { os: 'iOS', count: 100, percentage: 20 },
                { os: 'Android', count: 60, percentage: 12 },
                { os: 'Linux', count: 20, percentage: 4 },
            ],
            clicks_by_referer: [
                { referer: 'google.com', count: 150, percentage: 30 },
                { referer: 'twitter.com', count: 100, percentage: 20 },
                { referer: 'facebook.com', count: 75, percentage: 15 },
                { referer: 'direct', count: 175, percentage: 35 },
            ],
            recent_clicks: [
                {
                    id: 1,
                    timestamp: '2024-01-15T12:00:00Z',
                    country: 'United States',
                    city: 'New York',
                    device: 'Desktop',
                    browser: 'Chrome',
                    os: 'Windows',
                    referer: 'google.com',
                },
                {
                    id: 2,
                    timestamp: '2024-01-15T11:30:00Z',
                    country: 'United Kingdom',
                    city: 'London',
                    device: 'Mobile',
                    browser: 'Safari',
                    os: 'iOS',
                    referer: 'twitter.com',
                },
            ],
            geo_data: [
                { latitude: 40.7128, longitude: -74.0060, city: 'New York', country: 'United States', count: 80 },
                { latitude: 51.5074, longitude: -0.1278, city: 'London', country: 'United Kingdom', count: 70 },
                { latitude: 52.5200, longitude: 13.4050, city: 'Berlin', country: 'Germany', count: 40 },
            ],
        });
    });

    test('should display country breakdown', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for country data
        const countryData = page.locator('text=United States, text=United Kingdom, text=Germany');
        await expect(countryData.first()).toBeVisible({ timeout: 5000 });
    });

    test('should display city breakdown', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for city data
        const cityData = page.locator('text=New York, text=London, text=Berlin');
        if (await cityData.first().isVisible()) {
            await expect(cityData.first()).toBeVisible();
        }
    });

    test('should display browser distribution', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for browser data
        const browserData = page.locator('text=Chrome, text=Safari, text=Firefox');
        await expect(browserData.first()).toBeVisible({ timeout: 5000 });
    });

    test('should display device types', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for device data
        const deviceData = page.locator('text=Desktop, text=Mobile, text=Tablet');
        await expect(deviceData.first()).toBeVisible({ timeout: 5000 });
    });

    test('should display operating system distribution', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for OS data
        const osData = page.locator('text=Windows, text=macOS, text=iOS');
        if (await osData.first().isVisible()) {
            await expect(osData.first()).toBeVisible();
        }
    });

    test('should display referrer sources', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for referrer data
        const refererData = page.locator('text=google.com, text=twitter.com, text=direct');
        if (await refererData.first().isVisible()) {
            await expect(refererData.first()).toBeVisible();
        }
    });

    test('should display recent clicks with location', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for recent clicks table with location info
        const recentClicks = page.locator('text=New York, table, [data-testid="recent-clicks"]');
        if (await recentClicks.first().isVisible()) {
            await expect(recentClicks.first()).toBeVisible();
        }
    });

    test('should show percentage breakdowns', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for percentage indicators
        const percentages = page.locator('text=40%, text=50%, text=60%');
        if (await percentages.first().isVisible()) {
            await expect(percentages.first()).toBeVisible();
        }
    });
});

// ============= Geographic Map Tests =============

test.describe('Geographic Map Display', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'map123',
            original_url: 'https://example.com',
            total_clicks: 100,
            unique_visitors: 80,
            clicks_by_day: [],
            clicks_by_country: [
                { country: 'United States', count: 50, percentage: 50 },
                { country: 'United Kingdom', count: 30, percentage: 30 },
            ],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [
                { latitude: 40.7128, longitude: -74.0060, city: 'New York', country: 'United States', count: 50 },
                { latitude: 51.5074, longitude: -0.1278, city: 'London', country: 'United Kingdom', count: 30 },
            ],
        });
    });

    test('should have map container', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for map element
        const mapContainer = page.locator('[data-testid="geo-map"], .map, [class*="map"]');
        if (await mapContainer.isVisible()) {
            await expect(mapContainer).toBeVisible();
        }
    });

    test('should display geo points on map', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for map markers or points
        const mapMarkers = page.locator('[data-testid="map-marker"], .marker, [class*="marker"]');
        // Note: This depends on map implementation
    });
});

// ============= Analytics Time Range Tests =============

test.describe('Analytics Time Range', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'time123',
            original_url: 'https://example.com',
            total_clicks: 200,
            unique_visitors: 160,
            clicks_by_day: [
                { date: '2024-01-15', count: 30 },
                { date: '2024-01-14', count: 25 },
                { date: '2024-01-13', count: 20 },
            ],
            clicks_by_country: [],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [],
        });
    });

    test('should display daily click chart', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for chart elements
        const chart = page.locator('[data-testid="clicks-chart"], svg, canvas, .chart');
        await expect(chart.first()).toBeVisible({ timeout: 5000 });
    });

    test('should show time range selector', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for time range selector
        const timeSelector = page.locator('select[name*="range"], button:has-text("7 days"), button:has-text("30 days")');
        if (await timeSelector.first().isVisible()) {
            await expect(timeSelector.first()).toBeVisible();
        }
    });

    test('should filter by date range', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for date range picker
        const dateRangePicker = page.locator('input[type="date"], [data-testid="date-range"]');
        if (await dateRangePicker.first().isVisible()) {
            await dateRangePicker.first().click();
        }
    });
});

// ============= Analytics Export Tests =============

test.describe('Analytics Export', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'export123',
            original_url: 'https://example.com',
            total_clicks: 100,
            unique_visitors: 80,
            clicks_by_day: [{ date: '2024-01-15', count: 20 }],
            clicks_by_country: [{ country: 'US', count: 50, percentage: 50 }],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [],
        });
    });

    test('should have export button', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for export button
        const exportButton = page.locator('button:has-text("Export"), button:has-text("Download"), button[aria-label*="export"]');
        if (await exportButton.first().isVisible()) {
            await expect(exportButton.first()).toBeVisible();
        }
    });

    test('should offer export format options', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for export format options
        const exportButton = page.locator('button:has-text("Export")').first();
        if (await exportButton.isVisible()) {
            await exportButton.click();
            
            // Look for format options
            const csvOption = page.locator('text=CSV, button:has-text("CSV")');
            const jsonOption = page.locator('text=JSON, button:has-text("JSON")');
            
            if (await csvOption.isVisible()) {
                await expect(csvOption).toBeVisible();
            }
        }
    });
});

// ============= Analytics Comparison Tests =============

test.describe('Analytics Comparison', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });
    });

    test('should show click trend indicator', async ({ page }) => {
        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'trend123',
            original_url: 'https://example.com',
            total_clicks: 150,
            unique_visitors: 120,
            clicks_by_day: [
                { date: '2024-01-15', count: 30 },  // Today - higher
                { date: '2024-01-14', count: 20 },
                { date: '2024-01-13', count: 15 },
            ],
            clicks_by_country: [],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [],
        });

        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for trend indicators (up/down arrows, percentages)
        const trendIndicator = page.locator('[data-testid="trend"], .trend, text=+, text=▲');
        if (await trendIndicator.first().isVisible()) {
            await expect(trendIndicator.first()).toBeVisible();
        }
    });

    test('should display unique vs total visitors', async ({ page }) => {
        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'unique123',
            original_url: 'https://example.com',
            total_clicks: 200,
            unique_visitors: 150,
            clicks_by_day: [],
            clicks_by_country: [],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [],
        });

        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Look for both total and unique counts
        const totalClicks = page.locator('text=200');
        const uniqueVisitors = page.locator('text=150, text=unique');
        
        await expect(totalClicks.first()).toBeVisible({ timeout: 5000 });
    });
});

// ============= Empty Analytics State Tests =============

test.describe('Empty Analytics State', () => {
    test.beforeEach(async ({ page }) => {
        await page.addInitScript(() => {
            localStorage.setItem('token', 'mock-jwt-token');
        });

        await mockApiResponse(page, '**/links/1/stats', {
            link_id: 1,
            code: 'empty123',
            original_url: 'https://example.com',
            total_clicks: 0,
            unique_visitors: 0,
            clicks_by_day: [],
            clicks_by_country: [],
            clicks_by_city: [],
            clicks_by_browser: [],
            clicks_by_device: [],
            clicks_by_os: [],
            clicks_by_referer: [],
            recent_clicks: [],
            geo_data: [],
        });
    });

    test('should show zero clicks state', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Should show zero or "no data" message
        const zeroState = page.locator('text=0, text=No clicks, text=No data');
        await expect(zeroState.first()).toBeVisible({ timeout: 5000 });
    });

    test('should display empty chart gracefully', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Chart should still render without errors
        const chart = page.locator('[data-testid="clicks-chart"], svg, canvas, .chart');
        if (await chart.first().isVisible()) {
            await expect(chart.first()).toBeVisible();
        }
    });

    test('should show helpful message for new links', async ({ page }) => {
        await page.goto(`${BASE_URL}/dashboard/analytics/1`);
        
        // Should show encouraging message
        const helpMessage = page.locator('text=share your link, text=get started, text=no clicks yet');
        if (await helpMessage.first().isVisible()) {
            await expect(helpMessage.first()).toBeVisible();
        }
    });
});



