import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;
const fullBrowsers = process.env.E2E_FULL === '1';

/**
 * Playwright configuration for E2E tests
 * Run with: npm run test:e2e
 *
 * Default project is chromium so `npx playwright install chromium` is enough.
 * Set E2E_FULL=1 to also run firefox, webkit, and the mobile device projects.
 * webServer starts Vite on :5173 (matches baseURL). The backend on :3000 is
 * started by CI (or by you locally) — Playwright does not boot Postgres.
 */
export default defineConfig({
  testDir: './e2e',
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: isCI ? 1 : undefined,
  reporter: isCI
    ? [['github'], ['html', { open: 'never' }], ['list']]
    : [['html', { open: 'never' }]],

  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL || 'http://localhost:5173',
    trace: 'on-first-retry',
    screenshot: 'only-on-failure',
    video: 'on-first-retry',
  },

  projects: fullBrowsers
    ? [
        { name: 'chromium', use: { ...devices['Desktop Chrome'] } },
        { name: 'firefox', use: { ...devices['Desktop Firefox'] } },
        { name: 'webkit', use: { ...devices['Desktop Safari'] } },
        { name: 'Mobile Chrome', use: { ...devices['Pixel 5'] } },
        { name: 'Mobile Safari', use: { ...devices['iPhone 12'] } },
      ]
    : [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],

  /* Vite. 120s is a cold `npm run dev` (deps + first compile), not a warm HMR. */
  webServer: {
    command: 'npm run dev',
    url: 'http://localhost:5173',
    reuseExistingServer: !isCI,
    timeout: 120 * 1000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
