import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import Privacy from './Privacy';

// The policy is prerendered with the default windows and corrected from
// /auth/settings once the page is open, so it quotes what the operator set.
function mockSettings(settings: Record<string, unknown>) {
    global.fetch = vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: () => Promise.resolve(settings),
    });
}

const bodyText = () => document.body.textContent?.replace(/\s+/g, ' ') ?? '';

describe('Privacy retention windows', () => {
    beforeEach(() => {
        mockSettings({});
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it('states the default windows before (and without) any answer from the API', () => {
        // Nothing resolved yet: this is what the prerendered HTML says.
        global.fetch = vi.fn().mockReturnValue(new Promise(() => {}));
        render(<Privacy />);

        expect(bodyText()).toContain('permanently removed 30 days later');
        expect(bodyText()).toContain('anonymized after 13 months by a scheduled job');
        expect(bodyText()).toContain('removed after 13 months');
        expect(bodyText()).toContain('permanently removed after the 30 days described above');
    });

    it('keeps the defaults when the API cannot be reached', async () => {
        global.fetch = vi.fn().mockRejectedValue(new Error('offline'));
        render(<Privacy />);

        await waitFor(() => expect(global.fetch).toHaveBeenCalled());
        expect(bodyText()).toContain('permanently removed 30 days later');
        expect(bodyText()).toContain('anonymized after 13 months by a scheduled job');
    });

    it("quotes the operator's windows once the settings arrive", async () => {
        mockSettings({ account_purge_days: 14, analytics_pii_retention_days: 90 });
        render(<Privacy />);

        await waitFor(() => expect(bodyText()).toContain('permanently removed 14 days later'));
        expect(bodyText()).toContain('permanently removed after the 14 days described above');
        expect(bodyText()).toContain('anonymized after 3 months by a scheduled job');
        expect(bodyText()).toContain('removed after 3 months');
        expect(bodyText()).not.toContain('30 days later');
        expect(bodyText()).not.toContain('13 months');
        // Answering a verified deletion request is a promise of the policy, not a setting.
        expect(bodyText()).toContain('verified deletion requests within 30 days');
    });

    it('says days when the window is not a whole number of months', async () => {
        mockSettings({ account_purge_days: 1, analytics_pii_retention_days: 400 });
        render(<Privacy />);

        await waitFor(() => expect(bodyText()).toContain('permanently removed 1 day later'));
        expect(bodyText()).toContain('anonymized after 400 days by a scheduled job');
    });

    it('does not promise removal that the operator switched off', async () => {
        mockSettings({ account_purge_days: null, analytics_pii_retention_days: null });
        render(<Privacy />);

        await waitFor(() =>
            expect(bodyText()).toContain('are kept until an administrator permanently removes them'),
        );
        expect(bodyText()).toContain('the data is kept until an administrator permanently removes it.');
        expect(bodyText()).toContain('not automatically anonymized on this deployment');
        expect(bodyText()).toContain('this deployment does not remove automatically');
        expect(bodyText()).not.toContain('30 days later');
        expect(bodyText()).not.toContain('13 months');
    });

    it('keeps a default for a field an older server does not send', async () => {
        mockSettings({ account_purge_days: 7 });
        render(<Privacy />);

        await waitFor(() => expect(bodyText()).toContain('permanently removed 7 days later'));
        expect(bodyText()).toContain('anonymized after 13 months by a scheduled job');
    });
});
