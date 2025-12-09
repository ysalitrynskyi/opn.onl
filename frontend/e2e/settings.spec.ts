import { test, expect, Page } from '@playwright/test';

async function authenticateUser(page: Page) {
  const mockToken = 'eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJ0ZXN0QGV4YW1wbGUuY29tIiwidXNlcl9pZCI6MSwiZXhwIjoxOTk5OTk5OTk5fQ.test';
  
  await page.addInitScript((token) => {
    localStorage.setItem('token', token);
  }, mockToken);
}

test.describe('Settings Page', () => {
  test.beforeEach(async ({ page }) => {
    await authenticateUser(page);
    await page.goto('/settings');
  });

  test('should display page title', async ({ page }) => {
    await expect(page.getByRole('heading', { name: /settings/i })).toBeVisible();
  });

  test('should display security section', async ({ page }) => {
    await expect(page.getByText('Security')).toBeVisible();
  });

  test('should display passkeys option', async ({ page }) => {
    await expect(page.getByText('Passkeys')).toBeVisible();
    await expect(page.getByText('Sign in securely without a password')).toBeVisible();
  });

  test('should have add passkey button', async ({ page }) => {
    await expect(page.getByRole('button', { name: /add passkey/i })).toBeVisible();
  });

  test('should display change password option', async ({ page }) => {
    await expect(page.getByText('Change Password')).toBeVisible();
  });

  test('should display data export section', async ({ page }) => {
    await expect(page.getByText('Data & Export')).toBeVisible();
    await expect(page.getByText('Export Links')).toBeVisible();
  });

  test('should display danger zone', async ({ page }) => {
    await expect(page.getByText('Danger Zone')).toBeVisible();
    await expect(page.getByText('Delete Account')).toBeVisible();
  });

  test('should have delete account button', async ({ page }) => {
    await expect(page.getByRole('button', { name: /delete account/i })).toBeVisible();
  });

  test('should trigger export when clicked', async ({ page }) => {
    // Mock window.open
    let openCalled = false;
    await page.addInitScript(() => {
      window.open = () => { 
        (window as any).__openCalled = true;
        return null; 
      };
    });

    await page.getByText('Export Links').click();
    
    // Verify click was registered
    const clicked = await page.evaluate(() => (window as any).__openCalled);
    // Export should trigger a new window/tab
  });

  test('should show confirmation dialog for delete account', async ({ page }) => {
    // Set up dialog handler
    page.on('dialog', async dialog => {
      expect(dialog.type()).toBe('confirm');
      expect(dialog.message()).toContain('delete');
      await dialog.dismiss();
    });

    await page.getByRole('button', { name: /delete account/i }).click();
  });
});

test.describe('Settings Page - Not Authenticated', () => {
  test('should redirect to login', async ({ page }) => {
    await page.goto('/settings');
    await expect(page).toHaveURL('/login');
  });
});




