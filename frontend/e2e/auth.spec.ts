import { test, expect } from '@playwright/test';

test.describe('Authentication Flow', () => {
  test.describe('Login Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/login');
    });

    test('should display login form', async ({ page }) => {
      await expect(page.getByText('Welcome back')).toBeVisible();
      await expect(page.getByLabel('Email address')).toBeVisible();
      await expect(page.getByLabel('Password')).toBeVisible();
      await expect(page.getByRole('button', { name: /sign in/i })).toBeVisible();
    });

    test('should show link to register', async ({ page }) => {
      const signUpLink = page.getByRole('link', { name: /sign up/i });
      await expect(signUpLink).toBeVisible();
      await signUpLink.click();
      await expect(page).toHaveURL('/register');
    });

    test('should require email', async ({ page }) => {
      await page.getByLabel('Password').fill('password123');
      await page.getByRole('button', { name: /sign in/i }).click();
      
      // HTML5 validation should prevent submission
      const emailInput = page.getByLabel('Email address');
      await expect(emailInput).toBeFocused();
    });

    test('should require password', async ({ page }) => {
      await page.getByLabel('Email address').fill('test@example.com');
      await page.getByRole('button', { name: /sign in/i }).click();
      
      // HTML5 validation should prevent submission
      const passwordInput = page.getByLabel('Password');
      await expect(passwordInput).toBeFocused();
    });

    test('should validate email format', async ({ page }) => {
      await page.getByLabel('Email address').fill('invalid-email');
      await page.getByLabel('Password').fill('password123');
      await page.getByRole('button', { name: /sign in/i }).click();
      
      // Email input should have validation error
      const emailInput = page.getByLabel('Email address');
      const validationMessage = await emailInput.evaluate((el: HTMLInputElement) => el.validationMessage);
      expect(validationMessage).toBeTruthy();
    });

    test('should show passkey option if supported', async ({ page }) => {
      // Check if passkey button exists (may not show on all browsers)
      const passkeyButton = page.getByRole('button', { name: /passkey/i });
      // This test is informational - passkeys may not be available
      const isVisible = await passkeyButton.isVisible().catch(() => false);
      console.log(`Passkey button visible: ${isVisible}`);
    });
  });

  test.describe('Register Page', () => {
    test.beforeEach(async ({ page }) => {
      await page.goto('/register');
    });

    test('should display registration form', async ({ page }) => {
      await expect(page.getByText('Create an account')).toBeVisible();
      await expect(page.getByLabel('Email address')).toBeVisible();
      await expect(page.getByLabel('Password')).toBeVisible();
      await expect(page.getByRole('button', { name: /create account/i })).toBeVisible();
    });

    test('should show link to login', async ({ page }) => {
      const loginLink = page.getByRole('link', { name: /log in/i });
      await expect(loginLink).toBeVisible();
      await loginLink.click();
      await expect(page).toHaveURL('/login');
    });

    test('should show password requirements', async ({ page }) => {
      await page.getByLabel('Password').fill('test');
      await expect(page.getByText('At least 6 characters')).toBeVisible();
    });

    test('should indicate when password meets requirements', async ({ page }) => {
      await page.getByLabel('Password').fill('password123');
      // The check icon should be visible for met requirements
      await expect(page.getByText('At least 6 characters')).toBeVisible();
    });

    test('should show terms and privacy links', async ({ page }) => {
      await expect(page.getByRole('link', { name: /terms of service/i })).toBeVisible();
      await expect(page.getByRole('link', { name: /privacy policy/i })).toBeVisible();
    });

    test('should disable button for short password', async ({ page }) => {
      await page.getByLabel('Email address').fill('test@example.com');
      await page.getByLabel('Password').fill('abc');
      
      const button = page.getByRole('button', { name: /create account/i });
      await expect(button).toBeDisabled();
    });

    test('should enable button for valid password', async ({ page }) => {
      await page.getByLabel('Email address').fill('test@example.com');
      await page.getByLabel('Password').fill('password123');
      
      const button = page.getByRole('button', { name: /create account/i });
      await expect(button).toBeEnabled();
    });
  });
});

test.describe('Protected Routes', () => {
  test('should redirect to login from dashboard when not authenticated', async ({ page }) => {
    await page.goto('/dashboard');
    await expect(page).toHaveURL('/login');
  });

  test('should redirect to login from settings when not authenticated', async ({ page }) => {
    await page.goto('/settings');
    await expect(page).toHaveURL('/login');
  });

  test('should redirect to login from analytics when not authenticated', async ({ page }) => {
    await page.goto('/analytics/1');
    await expect(page).toHaveURL('/login');
  });
});




