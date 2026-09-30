import { test, expect } from '@playwright/test';

/**
 * Checks that hold for every public page at once. What a single page or
 * feature shows is tested in its own spec (home, auth, static-pages,
 * dashboard, analytics, ...); the end-to-end journey is comprehensive-flow.
 */

/** Every route a signed-out visitor can land on, plus the not-found page. */
const PUBLIC_PAGES = [
    '/',
    '/features',
    '/pricing',
    '/developers',
    '/docs',
    '/faq',
    '/about',
    '/contact',
    '/privacy',
    '/terms',
    '/login',
    '/register',
    '/forgot-password',
    '/404',
];

test('public pages render cleanly: one h1, alt text on every image, a name on every field, no console errors', async ({
    page,
}) => {
    test.slow(); // fourteen page loads
    const problems: string[] = [];
    const where = () => new URL(page.url()).pathname;
    page.on('console', (msg) => {
        if (msg.type() === 'error') problems.push(`${where()}: ${msg.text()}`);
    });
    page.on('pageerror', (err) => problems.push(`${where()}: uncaught ${err.message}`));

    for (const path of PUBLIC_PAGES) {
        await test.step(path, async () => {
            await page.goto(path);
            await expect(page.locator('h1')).toHaveCount(1);
            // Let lazy images and effects finish so anything they log is caught.
            await page.waitForLoadState('networkidle');

            const images = page.locator('img');
            // At least the header and footer logos, so the loop below is never empty.
            expect(await images.count()).toBeGreaterThanOrEqual(2);
            for (const image of await images.all()) {
                // Decorative images carry alt="", which is still an alt attribute.
                await expect(image).toHaveAttribute('alt');
            }

            for (const field of await page.locator('input:not([type="hidden"]), textarea, select').all()) {
                await expect(field).toHaveAccessibleName(/\S/);
            }
        });
    }

    expect(problems).toEqual([]);
});

test.describe('on a phone', () => {
    test.use({ viewport: { width: 375, height: 667 } });

    test('public pages fit a 375px-wide screen without scrolling sideways', async ({ page }) => {
        test.slow(); // fourteen page loads
        for (const path of PUBLIC_PAGES) {
            await test.step(path, async () => {
                await page.goto(path);
                await expect(page.locator('h1')).toBeVisible();
                // Measure the settled layout, after images and fonts arrive.
                await page.waitForLoadState('networkidle');
                const overflow = await page.evaluate(
                    () => document.documentElement.scrollWidth - document.documentElement.clientWidth,
                );
                expect(overflow, `${path} is wider than the screen by ${overflow}px`).toBe(0);
            });
        }
    });
});

test('keyboard users tab from the logo into the main navigation and follow it', async ({ page }) => {
    await page.goto('/');
    const header = page.getByRole('banner');

    await page.keyboard.press('Tab');
    await expect(header.locator('a[href="/"]')).toBeFocused();
    await page.keyboard.press('Tab');
    await expect(header.getByRole('link', { name: 'Features' })).toBeFocused();

    await page.keyboard.press('Enter');
    await expect(page).toHaveURL(/\/features$/);
    await expect(page.getByRole('heading', { level: 1 })).toBeVisible();
});
