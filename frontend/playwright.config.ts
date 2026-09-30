import { defineConfig, devices } from '@playwright/test';

const isCI = !!process.env.CI;
const fullBrowsers = process.env.E2E_FULL === '1';

// Ports are configurable so several stacks can run side by side locally.
// E2E_API_URL is the backend origin the dev server is pointed at (VITE_API_URL)
// and the one e2e/support/api.ts calls directly.
const webPort = process.env.E2E_WEB_PORT || '5173';
const apiUrl = process.env.E2E_API_URL || 'http://localhost:3000';

/**
 * Playwright configuration for E2E tests
 * Run with: npm run test:e2e
 *
 * Default project is chromium so `npx playwright install chromium` is enough.
 * Set E2E_FULL=1 to also run firefox, webkit, and the mobile device projects.
 * webServer starts Vite on :5173 (matches baseURL). The backend on :3000 is
 * started by CI (or by you locally) — Playwright does not boot Postgres.
 * globalSetup registers the admin that e2e/support/api.ts uses to verify test
 * accounts, so the backend needs a fresh database and TRUST_PROXY_HEADERS=true.
 */
export default defineConfig({
  testDir: './e2e',
  globalSetup: './e2e/support/global-setup.ts',
  fullyParallel: true,
  forbidOnly: isCI,
  retries: isCI ? 2 : 0,
  workers: isCI ? 1 : undefined,
  reporter: isCI
    ? [['github'], ['html', { open: 'never' }], ['list']]
    : [['html', { open: 'never' }]],

  use: {
    baseURL: process.env.PLAYWRIGHT_BASE_URL || `http://localhost:${webPort}`,
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
    url: `http://localhost:${webPort}`,
    env: { PORT: webPort, VITE_API_URL: apiUrl },
    reuseExistingServer: !isCI,
    timeout: 120 * 1000,
    stdout: 'pipe',
    stderr: 'pipe',
  },
});
