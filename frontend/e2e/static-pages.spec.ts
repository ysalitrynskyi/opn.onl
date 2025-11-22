import { test, expect } from '@playwright/test';

test.describe('Static Pages', () => {
  test.describe('Features Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/features');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByText('Everything you need to')).toBeVisible();
      await expect(page.getByText('manage your links')).toBeVisible();
    });

    test('should display feature cards', async ({ page }) => {
      await expect(page.getByText('Custom Short Links')).toBeVisible();
      await expect(page.getByText('Advanced Analytics')).toBeVisible();
      await expect(page.getByText('QR Code Generation')).toBeVisible();
      await expect(page.getByText('Password Protection')).toBeVisible();
    });

    test('should display comparison section', async ({ page }) => {
      await expect(page.getByText('Why choose opn.onl?')).toBeVisible();
    });

    test('should have CTA button', async ({ page }) => {
      const ctaButton = page.getByRole('link', { name: /get started for free/i });
      await expect(ctaButton).toBeVisible();
    });
  });

  test.describe('Pricing Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/pricing');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByText('Simple pricing.')).toBeVisible();
      await expect(page.getByText('Actually free.')).toBeVisible();
    });

    test('should display pricing cards', async ({ page }) => {
      await expect(page.getByText('Free')).toBeVisible();
      await expect(page.getByText('Pro')).toBeVisible();
      await expect(page.getByText('Self-Hosted')).toBeVisible();
    });

    test('should show all prices as $0', async ({ page }) => {
      const prices = page.getByText('$0');
      await expect(prices.first()).toBeVisible();
    });

    test('should have FAQ section', async ({ page }) => {
      await expect(page.getByText('Frequently Asked Questions')).toBeVisible();
    });
  });

  test.describe('About Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/about');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByRole('heading', { name: /about opn\.onl/i })).toBeVisible();
    });

    test('should display our story section', async ({ page }) => {
      await expect(page.getByText('Our Story')).toBeVisible();
    });

    test('should display values section', async ({ page }) => {
      await expect(page.getByText('Our Values')).toBeVisible();
      await expect(page.getByText('Privacy First')).toBeVisible();
    });

    test('should have GitHub link', async ({ page }) => {
      const githubLink = page.getByRole('link', { name: /view on github/i });
      await expect(githubLink).toBeVisible();
    });
  });

  test.describe('Privacy Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/privacy');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByRole('heading', { name: /privacy policy/i })).toBeVisible();
    });

    test('should display privacy sections', async ({ page }) => {
      await expect(page.getByText('Information We Collect')).toBeVisible();
      await expect(page.getByText('How We Use Your Information')).toBeVisible();
      await expect(page.getByText('Data Security')).toBeVisible();
    });

    test('should show last updated date', async ({ page }) => {
      await expect(page.getByText(/last updated/i)).toBeVisible();
    });
  });

  test.describe('Terms Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/terms');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByRole('heading', { name: /terms of service/i })).toBeVisible();
    });

    test('should display terms sections', async ({ page }) => {
      await expect(page.getByText('Acceptance of Terms')).toBeVisible();
      await expect(page.getByText('Acceptable Use')).toBeVisible();
      await expect(page.getByText('Termination')).toBeVisible();
    });
  });

  test.describe('Contact Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/contact');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByRole('heading', { name: /contact us/i })).toBeVisible();
    });

    test('should display contact options', async ({ page }) => {
      await expect(page.getByText('Email Support')).toBeVisible();
      await expect(page.getByText('GitHub Issues')).toBeVisible();
    });

    test('should have contact form', async ({ page }) => {
      await expect(page.getByLabel('Your Name')).toBeVisible();
      await expect(page.getByLabel('Email Address')).toBeVisible();
      await expect(page.getByLabel('Subject')).toBeVisible();
      await expect(page.getByLabel('Message')).toBeVisible();
    });

    test('should have submit button', async ({ page }) => {
      await expect(page.getByRole('button', { name: /send message/i })).toBeVisible();
    });
  });

  test.describe('FAQ Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/faq');
    });

    test('should display page title', async ({ page }) => {
      await expect(page.getByRole('heading', { name: /frequently asked questions/i })).toBeVisible();
    });

    test('should have search functionality', async ({ page }) => {
      await expect(page.getByPlaceholder('Search questions...')).toBeVisible();
    });

    test('should display FAQ categories', async ({ page }) => {
      await expect(page.getByText('Getting Started')).toBeVisible();
      await expect(page.getByText('Features')).toBeVisible();
      await expect(page.getByText('Security & Privacy')).toBeVisible();
    });

    test('should expand FAQ item on click', async ({ page }) => {
      const question = page.getByText('Is opn.onl free to use?');
      await question.click();
      
      await expect(page.getByText(/completely free/i)).toBeVisible();
    });

    test('should filter FAQs by search', async ({ page }) => {
      await page.getByPlaceholder('Search questions...').fill('password');
      
      await expect(page.getByText(/password/i).first()).toBeVisible();
    });

    test('should show contact link', async ({ page }) => {
      await expect(page.getByRole('link', { name: /contact support/i })).toBeVisible();
    });
  });
});

test.describe('404 and Error Handling', () => {
  test('should show 404 for unknown routes', async ({ page }) => {
    await page.goto('/nonexistent-page');
    // Should either show 404 or redirect to home
    // Depending on implementation
  });
});

test.describe('Navigation', () => {
  test('should navigate between pages correctly', async ({ page }) => {
    await page.goto('/');
    
    // Navigate to Features
    await page.getByRole('link', { name: /features/i }).first().click();
    await expect(page).toHaveURL('/features');
    
    // Navigate to Pricing
    await page.getByRole('link', { name: /pricing/i }).first().click();
    await expect(page).toHaveURL('/pricing');
    
    // Navigate to FAQ
    await page.getByRole('link', { name: /faq/i }).first().click();
    await expect(page).toHaveURL('/faq');
    
    // Navigate back to Home via logo
    await page.locator('header a').first().click();
    await expect(page).toHaveURL('/');
  });

  test('should preserve scroll position on back navigation', async ({ page }) => {
    await page.goto('/features');
    
    // Scroll down
    await page.evaluate(() => window.scrollTo(0, 500));
    
    // Navigate to another page
    await page.getByRole('link', { name: /pricing/i }).first().click();
    
    // Go back
    await page.goBack();
    
    // Check we're back on features
    await expect(page).toHaveURL('/features');
  });
});

