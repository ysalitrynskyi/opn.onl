import { beforeEach, describe, expect, it, vi } from 'vitest';
import { fireEvent, render, screen, waitFor } from '../../test/test-utils';
import EditModal from './EditModal';
import type { LinkData, RoutingRule } from './types';

const baseLink: LinkData = {
    id: 1,
    code: 'abc_123',
    original_url: 'https://example.com/original',
    short_url: 'https://opn.onl/abc_123',
    title: null,
    click_count: 0,
    created_at: '2026-01-01 00:00:00',
    expires_at: null,
    has_password: false,
    notes: null,
    is_active: true,
    is_pinned: false,
    tags: [],
};

const jsonResponse = (body: unknown, ok = true, status = 200) =>
    ({ ok, status, json: () => Promise.resolve(body) }) as Response;

type Call = { url: string; method: string; body?: string };
let calls: Call[] = [];

const isRulesCall = (c: Call) => c.url.endsWith('/rules');
const rulesGets = () => calls.filter(c => c.method === 'GET' && isRulesCall(c));
const rulePuts = () => calls.filter(c => c.method === 'PUT' && isRulesCall(c));
const putRules = () => JSON.parse(rulePuts()[0].body!).rules as RoutingRule[];

/** `rulesGet` answers GET /links/1/rules; every other request gets an empty list. */
function mockFetch(rulesGet: (() => Promise<Response>) | null) {
    calls = [];
    vi.mocked(global.fetch).mockImplementation((url, options) => {
        const method = (options?.method ?? 'GET').toUpperCase();
        calls.push({ url: String(url), method, body: options?.body as string | undefined });
        if (method === 'GET' && String(url).endsWith('/rules') && rulesGet) return rulesGet();
        return Promise.resolve(jsonResponse([]));
    });
}

const stored = (...rules: Partial<RoutingRule>[]) =>
    rules.map((r, i) => ({ id: i + 1, destination_url: '', weight: 1, priority: i, ...r }));

const destinationInputs = () =>
    screen.getAllByLabelText(/destination url/i).filter(
        el => (el as HTMLInputElement).placeholder === 'https://…',
    ) as HTMLInputElement[];

function renderModal(onSave = vi.fn().mockResolvedValue(undefined), props = {}) {
    const view = render(
        <EditModal link={baseLink} onClose={vi.fn()} onSave={onSave} routingEnabled burnEnabled {...props} />,
    );
    return { ...view, onSave };
}

describe('EditModal routing rules: loading', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it('does not replace the stored rules when the rules request failed', async () => {
        mockFetch(() => Promise.resolve(jsonResponse({ error: 'boom' }, false, 500)));
        const { user, onSave } = renderModal();
        await waitFor(() => expect(rulesGets()).toHaveLength(1));
        await screen.findByRole('alert');

        // The user changes something unrelated to routing and saves.
        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(rulePuts()).toEqual([]);
    });

    it('says the session ended, and offers no retry, when the rules request is rejected as unauthorized', async () => {
        mockFetch(() => Promise.resolve({ status: 401, ok: false } as Response));
        renderModal();

        const alert = await screen.findByRole('alert');
        expect(alert).toHaveTextContent('Session expired. Please log in again.');
        expect(alert).not.toHaveTextContent(/couldn't load/i);
        expect(screen.queryByRole('button', { name: /try again/i })).not.toBeInTheDocument();
    });

    it('does not replace the stored rules when the rules request never reached the server', async () => {
        mockFetch(() => Promise.reject(new TypeError('Failed to fetch')));
        const { user, onSave } = renderModal();
        await screen.findByRole('alert');

        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(rulePuts()).toEqual([]);
    });

    it('does not replace the stored rules when the response is not a list', async () => {
        mockFetch(() => Promise.resolve(jsonResponse({ rules: [] })));
        const { user, onSave } = renderModal();
        await screen.findByRole('alert');

        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(rulePuts()).toEqual([]);
    });

    it('disables the routing section and says why when the rules could not be loaded', async () => {
        mockFetch(() => Promise.resolve(jsonResponse({ error: 'boom' }, false, 500)));
        renderModal();

        const alert = await screen.findByRole('alert');
        expect(alert).toHaveTextContent(/couldn't load this link's routing rules/i);
        expect(screen.getByRole('button', { name: /smart routing/i })).toBeDisabled();
        expect(screen.queryByRole('button', { name: /add rule/i })).not.toBeInTheDocument();
    });

    it('can retry loading the rules, after which they are editable again', async () => {
        let attempt = 0;
        mockFetch(() => {
            attempt += 1;
            return Promise.resolve(
                attempt === 1
                    ? jsonResponse({ error: 'boom' }, false, 500)
                    : jsonResponse(stored({ destination_url: 'https://a.example' })),
            );
        });
        const { user } = renderModal();
        await screen.findByRole('alert');

        await user.click(screen.getByRole('button', { name: /try again/i }));

        await screen.findByText(/smart routing \(1\)/i);
        expect(screen.queryByRole('alert')).not.toBeInTheDocument();
        expect(screen.getByRole('button', { name: /smart routing/i })).toBeEnabled();
        expect(destinationInputs()[0]).toHaveValue('https://a.example');
    });

    it('does not send rules when Save is clicked before the stored rules arrived', async () => {
        // GET /rules is still in flight when the user clicks Save.
        let resolveGet: (r: Response) => void = () => {};
        mockFetch(() => new Promise<Response>(resolve => { resolveGet = resolve; }));
        const { user, onSave } = renderModal();

        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());

        resolveGet(jsonResponse(stored({ destination_url: 'https://a.example' })));
        await screen.findByText(/smart routing \(1\)/i);
        expect(rulePuts()).toEqual([]);
    });

    it('shows a loading note instead of an editable list while the rules are on their way', async () => {
        mockFetch(() => new Promise<Response>(() => {}));
        const { user } = renderModal();

        await user.click(screen.getByRole('button', { name: /smart routing/i }));

        expect(screen.getByText(/loading routing rules/i)).toBeInTheDocument();
        expect(screen.queryByRole('button', { name: /add rule/i })).not.toBeInTheDocument();
    });

    it('lists the stored rules in priority order whatever order the API sent them in', async () => {
        mockFetch(() => Promise.resolve(jsonResponse([
            { id: 2, destination_url: 'https://second.example', weight: 1, priority: 5 },
            { id: 1, destination_url: 'https://first.example', weight: 1, priority: 2 },
        ])));
        renderModal();

        await screen.findByText(/smart routing \(2\)/i);
        expect(destinationInputs().map(i => i.value)).toEqual([
            'https://first.example',
            'https://second.example',
        ]);
    });
});

describe('EditModal routing rules: saving', () => {
    beforeEach(() => {
        vi.mocked(global.fetch).mockReset();
    });

    it('sends the rules with a priority per position, so the first match wins', async () => {
        mockFetch(() => Promise.resolve(jsonResponse([])));
        const { user, onSave } = renderModal();
        await waitFor(() => expect(rulesGets()).toHaveLength(1));

        await user.click(screen.getByRole('button', { name: /smart routing/i }));
        await user.click(await screen.findByRole('button', { name: /add rule/i }));
        await user.click(screen.getByRole('button', { name: /add rule/i }));
        const [first, second] = destinationInputs();
        fireEvent.change(first, { target: { value: 'https://app.example/x' } });
        fireEvent.change(second, { target: { value: 'https://web.example/' } });
        // Rule 1: Device = Mobile. Rule 2: the catch-all.
        fireEvent.change(screen.getAllByRole('combobox')[0], { target: { value: 'Mobile' } });

        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        expect(onSave).toHaveBeenCalledOnce();
        const rules = putRules();
        expect(rules.map(r => r.destination_url)).toEqual(['https://app.example/x', 'https://web.example/']);
        expect(rules.map(r => r.match_device)).toEqual(['Mobile', null]);
        expect(rules.map(r => r.priority)).toEqual([0, 1]);
    });

    it('does not send rules that were loaded and left alone', async () => {
        // Stored with a tie, as the old editor saved them: nothing was edited, so nothing is rewritten.
        mockFetch(() => Promise.resolve(jsonResponse(stored(
            { destination_url: 'https://a.example', match_device: 'Mobile', priority: 0 },
            { destination_url: 'https://b.example', priority: 0 },
        ))));
        const { user, onSave } = renderModal();
        await screen.findByText(/smart routing \(2\)/i);

        await user.click(screen.getByRole('checkbox', { name: /burn after reading/i }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(rulePuts()).toEqual([]);
    });

    it('sends the new order and priorities after a rule is moved up', async () => {
        mockFetch(() => Promise.resolve(jsonResponse(stored(
            { destination_url: 'https://web.example', priority: 0 },
            { destination_url: 'https://app.example', match_device: 'Mobile', priority: 1 },
        ))));
        const { user } = renderModal();
        await screen.findByText(/smart routing \(2\)/i);

        await user.click(screen.getByRole('button', { name: 'Move rule 2 up' }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        const rules = putRules();
        expect(rules.map(r => r.destination_url)).toEqual(['https://app.example', 'https://web.example']);
        expect(rules.map(r => r.priority)).toEqual([0, 1]);
    });

    it('sends the rest of the list with fresh priorities after a rule is removed', async () => {
        mockFetch(() => Promise.resolve(jsonResponse(stored(
            { destination_url: 'https://a.example', match_country: 'DE', priority: 0 },
            { destination_url: 'https://b.example', match_country: 'FR', priority: 3 },
            { destination_url: 'https://c.example', priority: 9 },
        ))));
        const { user } = renderModal();
        await screen.findByText(/smart routing \(3\)/i);

        await user.click(screen.getAllByRole('button', { name: /remove/i })[0]);
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        const rules = putRules();
        expect(rules.map(r => r.destination_url)).toEqual(['https://b.example', 'https://c.example']);
        expect(rules.map(r => r.priority)).toEqual([0, 1]);
    });

    it('keeps an A/B split together: rules with identical conditions share a priority', async () => {
        mockFetch(() => Promise.resolve(jsonResponse(stored(
            { destination_url: 'https://de.example', match_country: 'DE', priority: 0 },
            { destination_url: 'https://a.example', priority: 1, weight: 3 },
            { destination_url: 'https://b.example', priority: 1, weight: 1 },
        ))));
        const { user } = renderModal();
        await screen.findByText(/smart routing \(3\)/i);

        // Editing the weight of one side of the split is a change worth saving.
        fireEvent.change(screen.getAllByLabelText(/weight/i)[1], { target: { value: '2' } });
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        const rules = putRules();
        expect(rules.map(r => r.priority)).toEqual([0, 1, 1]);
        expect(rules.map(r => r.weight)).toEqual([1, 3, 2]);
    });

    it('numbers the priorities after the rules without a destination are dropped', async () => {
        mockFetch(() => Promise.resolve(jsonResponse([])));
        const { user } = renderModal();
        await waitFor(() => expect(rulesGets()).toHaveLength(1));

        await user.click(screen.getByRole('button', { name: /smart routing/i }));
        await user.click(await screen.findByRole('button', { name: /add rule/i }));
        await user.click(screen.getByRole('button', { name: /add rule/i }));
        await user.click(screen.getByRole('button', { name: /add rule/i }));
        const inputs = destinationInputs();
        fireEvent.change(inputs[0], { target: { value: 'https://one.example' } });
        // The second rule is left blank.
        fireEvent.change(inputs[2], { target: { value: 'https://three.example' } });
        fireEvent.change(screen.getAllByRole('combobox')[0], { target: { value: 'Desktop' } });

        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        const rules = putRules();
        expect(rules.map(r => r.destination_url)).toEqual(['https://one.example', 'https://three.example']);
        expect(rules.map(r => r.priority)).toEqual([0, 1]);
    });

    it('clears the stored rules when the user removes all of them', async () => {
        mockFetch(() => Promise.resolve(jsonResponse(stored({ destination_url: 'https://a.example' }))));
        const { user } = renderModal();
        await screen.findByText(/smart routing \(1\)/i);

        await user.click(screen.getByRole('button', { name: /remove/i }));
        await user.click(screen.getByRole('button', { name: /save changes/i }));
        await waitFor(() => expect(rulePuts()).toHaveLength(1));

        expect(putRules()).toEqual([]);
    });

    it('does not touch the rules endpoint at all when routing is off', async () => {
        mockFetch(null);
        const { user, onSave } = renderModal(undefined, { routingEnabled: false });

        await user.click(screen.getByRole('button', { name: /save changes/i }));

        await waitFor(() => expect(onSave).toHaveBeenCalledOnce());
        expect(calls.filter(isRulesCall)).toEqual([]);
    });
});
