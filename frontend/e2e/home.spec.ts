import { test, expect } from '@playwright/test';

test.describe('Home Page', () => {
  test.beforeEach(async ({ page }) => {
    await page.goto('/');
  });

  test('should display the hero section', async ({ page }) => {
    await expect(page.getByText('Shorten links,')).toBeVisible();
    await expect(page.getByText('expand your reach.')).toBeVisible();
  });

  test('should display the URL input form', async ({ page }) => {
    const input = page.getByPlaceholder('Paste your long link here...');
    await expect(input).toBeVisible();
    
    const button = page.getByRole('button', { name: /shorten/i });
    await expect(button).toBeVisible();
  });

  test('should display feature cards', async ({ page }) => {
    await expect(page.getByText('Lightning Fast')).toBeVisible();
    await expect(page.getByText('Privacy First')).toBeVisible();
    await expect(page.getByText('Detailed Analytics')).toBeVisible();
  });

  test('should navigate to Features page', async ({ page }) => {
    await page.getByRole('link', { name: /features/i }).first().click();
    await expect(page).toHaveURL('/features');
  });

  test('should navigate to Pricing page', async ({ page }) => {
    await page.getByRole('link', { name: /pricing/i }).first().click();
    await expect(page).toHaveURL('/pricing');
  });

  test('should navigate to Login page', async ({ page }) => {
    await page.getByRole('link', { name: /log in/i }).first().click();
    await expect(page).toHaveURL('/login');
  });

  test('should navigate to Register page', async ({ page }) => {
    await page.getByRole('link', { name: /sign up/i }).first().click();
    await expect(page).toHaveURL('/register');
  });

  test('should redirect to register when trying to shorten URL without auth', async ({ page }) => {
    const input = page.getByPlaceholder('Paste your long link here...');
    await input.fill('https://example.com/very-long-url');
    
    await page.getByRole('button', { name: /shorten/i }).click();
    
    await expect(page).toHaveURL('/register');
  });

  test('should display terms and privacy links', async ({ page }) => {
    await expect(page.getByText('Terms of Service')).toBeVisible();
    await expect(page.getByText('Privacy Policy')).toBeVisible();
  });

  test('should have working footer links', async ({ page }) => {
    await page.getByRole('link', { name: 'About' }).click();
    await expect(page).toHaveURL('/about');
    
    await page.goto('/');
    await page.getByRole('link', { name: 'Contact' }).click();
    await expect(page).toHaveURL('/contact');
  });
});

test.describe('Home Page - Mobile', () => {
  test.use({ viewport: { width: 375, height: 667 } });

  test('should display mobile navigation', async ({ page }) => {
    await page.goto('/');
    
    // Mobile menu button should be visible
    const menuButton = page.locator('button').filter({ has: page.locator('svg') }).first();
    await expect(menuButton).toBeVisible();
  });

  test('should open mobile menu', async ({ page }) => {
    await page.goto('/');
    
    // Click menu button
    await page.locator('header button').last().click();
    
    // Menu items should be visible
    await expect(page.getByText('Features').first()).toBeVisible();
    await expect(page.getByText('Pricing').first()).toBeVisible();
  });
});

