import { randomUUID } from 'node:crypto';
import { test, expect } from '@playwright/test';
import { API_URL, clientIpHeader, uniqueEmail } from './support/api';

/**
 * The short-link redirect page is a lazy route, compiled by the dev server on
 * first use; that can take well over the default 5s on a cold or busy machine.
 * Checks that it has done its work wait this long.
 */
const LAZY_PAGE = { timeout: 20_000 };

// Give every page its own client address. The backend rate-limits by
// CF-Connecting-IP (the contact form: 10 messages an hour); without this every
// browser in the suite would share 127.0.0.1's buckets.
test.beforeEach(async ({ page }) => {
    await page.setExtraHTTPHeaders(clientIpHeader());
});

test.describe('Static pages', () => {
    test.describe('Features page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/features');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('Features | opn.onl');
            await expect(page.getByRole('heading', { level: 1 })).toHaveText(
                /^Everything you need to\s*manage your links$/,
            );
        });

        test('shows the feature cards', async ({ page }) => {
            for (const title of [
                'Custom Short Links',
                'Advanced Analytics',
                'Branded QR Codes',
                'Password Protection',
                'Burn After Reading',
                'Smart Conditional Routing',
            ]) {
                await expect(page.getByRole('heading', { level: 3, name: title })).toBeVisible();
            }
        });

        test('shows the comparison section', async ({ page }) => {
            await expect(page.getByRole('heading', { level: 2, name: 'Why choose opn.onl?' })).toBeVisible();
            await expect(page.getByText('Privacy-first. Optional analytics is disclosed')).toBeVisible();
        });

        test('sends the call to action to sign up', async ({ page }) => {
            await page.getByRole('link', { name: 'Get started for free' }).click();
            await expect(page).toHaveURL('/register');
        });
    });

    test.describe('Pricing page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/pricing');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('Pricing | opn.onl');
            await expect(page.getByRole('heading', { level: 1 })).toHaveText(/^Simple pricing\.\s*Actually free\.$/);
        });

        test('shows the three plans', async ({ page }) => {
            for (const plan of ['Free', 'Pro', 'Self-Hosted']) {
                await expect(page.getByRole('heading', { level: 3, name: plan, exact: true })).toBeVisible();
            }
        });

        test('prices every plan at $0', async ({ page }) => {
            const prices = page.getByText('$0', { exact: true });
            await expect(prices).toHaveCount(3);
            for (const price of await prices.all()) {
                await expect(price).toBeVisible();
            }
        });

        test('has a FAQ section', async ({ page }) => {
            await expect(page.getByRole('heading', { level: 2, name: 'Frequently Asked Questions' })).toBeVisible();
            await expect(page.getByRole('heading', { level: 3, name: 'Is opn.onl really free?' })).toBeVisible();
        });
    });

    test.describe('About page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/about');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('About | opn.onl');
            await expect(page.getByRole('heading', { level: 1, name: 'About opn.onl' })).toBeVisible();
        });

        test('tells the story', async ({ page }) => {
            await expect(page.getByRole('heading', { level: 2, name: 'Our Story' })).toBeVisible();
        });

        test('lists the values', async ({ page }) => {
            await expect(page.getByRole('heading', { level: 2, name: 'Our Values' })).toBeVisible();
            for (const value of ['Privacy First', 'Performance', 'Open Source', 'Accessibility']) {
                await expect(page.getByRole('heading', { level: 3, name: value })).toBeVisible();
            }
        });

        test('links to the source on GitHub', async ({ page }) => {
            await expect(page.getByRole('link', { name: 'View on GitHub' })).toHaveAttribute(
                'href',
                'https://github.com/ysalitrynskyi/opn.onl',
            );
        });
    });

    test.describe('Privacy page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/privacy');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('Privacy Policy | opn.onl');
            await expect(page.getByRole('heading', { level: 1, name: 'Privacy Policy' })).toBeVisible();
        });

        test('shows the policy sections', async ({ page }) => {
            for (const section of ['Information We Collect', 'How We Use Your Information', 'Data Security']) {
                await expect(page.getByRole('heading', { level: 3, name: section })).toBeVisible();
            }
        });

        test('shows when it was last updated', async ({ page }) => {
            await expect(page.getByText(/^Last updated: \w+ \d{1,2}, \d{4}$/)).toBeVisible();
        });
    });

    test.describe('Terms page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/terms');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('Terms of Service | opn.onl');
            await expect(page.getByRole('heading', { level: 1, name: 'Terms of Service' })).toBeVisible();
        });

        test('shows the terms sections', async ({ page }) => {
            for (const section of ['1. Acceptance of Terms', '3. Acceptable Use', '7. Termination']) {
                await expect(page.getByRole('heading', { level: 3, name: section })).toBeVisible();
            }
        });
    });

    test.describe('Contact page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/contact');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('Contact Us | opn.onl');
            await expect(page.getByRole('heading', { level: 1, name: 'Contact Us' })).toBeVisible();
        });

        test('offers email and GitHub as contact options', async ({ page }) => {
            await expect(page.getByRole('link', { name: /Email Support/ })).toHaveAttribute(
                'href',
                'mailto:support@opn.onl',
            );
            await expect(page.getByRole('link', { name: /GitHub Issues/ })).toHaveAttribute(
                'href',
                'https://github.com/ysalitrynskyi/opn.onl/issues',
            );
        });

        test('has the contact form', async ({ page }) => {
            await expect(page.getByLabel('Your Name')).toBeVisible();
            await expect(page.getByLabel('Email Address')).toBeVisible();
            await expect(page.getByLabel('Subject')).toBeVisible();
            await expect(page.getByLabel('Message')).toBeVisible();
            await expect(page.getByRole('button', { name: 'Send Message' })).toBeEnabled();
        });

        test('says the message was not delivered when the instance cannot send email', async ({ page }) => {
            // The e2e backend has no SMTP, like a self-hosted instance without it.
            // The message has nowhere to go, so the page must not claim it was sent.
            const message = `Checking the contact form end to end (${randomUUID()}).`;
            await page.getByLabel('Your Name').fill('E2E Visitor');
            await page.getByLabel('Email Address').fill(uniqueEmail('contact'));
            await page.getByLabel('Subject').selectOption('feedback');
            await page.getByLabel('Message').fill(message);

            const sent = page.waitForResponse((r) => r.url() === `${API_URL}/contact`);
            await page.getByRole('button', { name: 'Send Message' }).click();
            const response = await sent;
            expect(response.status()).toBe(503);
            expect((await response.json()).success).toBe(false);

            await expect(page.getByRole('alert')).toContainText('not delivered');
            await expect(page.getByRole('heading', { name: 'Message Sent!' })).toHaveCount(0);
            // What the visitor wrote is still there to send another way.
            await expect(page.getByLabel('Message')).toHaveValue(message);
        });
    });

    test.describe('FAQ page', () => {
        test.beforeEach(async ({ page }) => {
            await page.goto('/faq');
        });

        test('shows the page title', async ({ page }) => {
            await expect(page).toHaveTitle('FAQ | opn.onl');
            await expect(page.getByRole('heading', { level: 1, name: 'Frequently Asked Questions' })).toBeVisible();
        });

        test('has a search box', async ({ page }) => {
            await expect(page.getByPlaceholder('Search questions...')).toBeVisible();
        });

        test('groups the questions by category', async ({ page }) => {
            for (const category of ['Getting Started', 'Features', 'Security & Privacy', 'Technical']) {
                await expect(page.getByRole('heading', { level: 2, name: category, exact: true })).toBeVisible();
            }
        });

        test('expands and collapses an answer', async ({ page }) => {
            const answer = page.getByText(/opn\.onl is completely free to use/);
            await expect(answer).toHaveCount(0);

            await page.getByRole('button', { name: 'Is opn.onl free to use?' }).click();
            await expect(answer).toBeVisible();

            await page.getByRole('button', { name: 'Is opn.onl free to use?' }).click();
            await expect(answer).toHaveCount(0);
        });

        test('filters the questions as you search', async ({ page }) => {
            const search = page.getByPlaceholder('Search questions...');
            await search.fill('password');
            await expect(page.getByRole('button', { name: 'Can I password-protect my links?' })).toBeVisible();
            await expect(page.getByRole('button', { name: 'Is opn.onl free to use?' })).toHaveCount(0);

            await search.fill('qwertyuiop');
            await expect(page.getByText('No questions found matching "qwertyuiop"')).toBeVisible();
            await page.getByRole('button', { name: 'Clear search' }).click();
            await expect(search).toHaveValue('');
            await expect(page.getByRole('button', { name: 'Is opn.onl free to use?' })).toBeVisible();
        });

        test('points to support for anything else', async ({ page }) => {
            await page.getByRole('link', { name: 'Contact Support' }).click();
            await expect(page).toHaveURL('/contact');
        });
    });
});

test.describe('Unknown pages', () => {
    test('a path that is not a short link shows the not-found page', async ({ page }) => {
        // One-segment paths are short-link codes to the SPA; it asks the
        // backend, gets a 404, and moves to /404.
        await page.goto(`/no-such-page-${randomUUID().slice(0, 8)}`);
        await expect(page).toHaveURL('/404', LAZY_PAGE);
        await expect(page.getByRole('heading', { level: 1, name: 'Page not found' })).toBeVisible();
    });

    test('a nested unknown path shows the not-found page in place', async ({ page }) => {
        await page.goto('/no/such/page');
        await expect(page).toHaveURL('/no/such/page');
        await expect(page.getByRole('heading', { level: 1, name: 'Page not found' })).toBeVisible();

        await page.getByRole('link', { name: 'Go to homepage' }).click();
        await expect(page).toHaveURL('/');
    });
});

test.describe('Navigation', () => {
    test('moves between pages from the header and back home from the logo', async ({ page }) => {
        await page.goto('/');
        const header = page.getByRole('banner');

        await header.getByRole('link', { name: 'Features' }).click();
        await expect(page).toHaveURL('/features');

        await header.getByRole('link', { name: 'Pricing' }).click();
        await expect(page).toHaveURL('/pricing');

        await header.getByRole('link', { name: 'FAQ' }).click();
        await expect(page).toHaveURL('/faq');

        await header.getByRole('link', { name: /opn\.onl logo/ }).click();
        await expect(page).toHaveURL('/');
    });

    test('opens each new page scrolled to the top', async ({ page }) => {
        await page.goto('/features');
        await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
        await expect.poll(() => page.evaluate(() => window.scrollY)).toBeGreaterThan(500);

        await page.getByRole('contentinfo').getByRole('link', { name: 'Pricing' }).click();
        await expect(page).toHaveURL('/pricing');
        await expect.poll(() => page.evaluate(() => window.scrollY)).toBe(0);
    });
});
