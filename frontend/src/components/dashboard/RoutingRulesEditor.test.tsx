import { useState } from 'react';
import { describe, expect, it } from 'vitest';
import { render, screen } from '../../test/test-utils';
import RoutingRulesEditor from './RoutingRulesEditor';
import type { RoutingRule } from './types';

function Harness({ initial }: { initial: RoutingRule[] }) {
    const [rules, setRules] = useState(initial);
    return <RoutingRulesEditor rules={rules} onChange={setRules} />;
}

const three: RoutingRule[] = [
    { destination_url: 'https://one.example', match_country: 'DE', weight: 1 },
    { destination_url: 'https://two.example', match_country: 'FR', weight: 1 },
    { destination_url: 'https://three.example', weight: 1 },
];

const destinations = () =>
    (screen.getAllByPlaceholderText('https://…') as HTMLInputElement[]).map(i => i.value);

describe('RoutingRulesEditor', () => {
    it('says that the first matching rule wins', () => {
        render(<Harness initial={three} />);
        expect(screen.getByText('First matching rule wins.')).toBeInTheDocument();
    });

    it('numbers the rules in the order they are checked', () => {
        render(<Harness initial={three} />);
        expect(screen.getByText('Rule 1')).toBeInTheDocument();
        expect(screen.getByText('Rule 3')).toBeInTheDocument();
    });

    it('moves a rule up and down', async () => {
        const { user } = render(<Harness initial={three} />);

        await user.click(screen.getByRole('button', { name: 'Move rule 3 up' }));
        expect(destinations()).toEqual(['https://one.example', 'https://three.example', 'https://two.example']);

        await user.click(screen.getByRole('button', { name: 'Move rule 1 down' }));
        expect(destinations()).toEqual(['https://three.example', 'https://one.example', 'https://two.example']);
    });

    it('cannot move the first rule up or the last rule down', () => {
        render(<Harness initial={three} />);
        expect(screen.getByRole('button', { name: 'Move rule 1 up' })).toBeDisabled();
        expect(screen.getByRole('button', { name: 'Move rule 3 down' })).toBeDisabled();
        expect(screen.getByRole('button', { name: 'Move rule 2 up' })).toBeEnabled();
        expect(screen.getByRole('button', { name: 'Move rule 2 down' })).toBeEnabled();
    });

    it('keeps the focus on the arrow of the rule that was moved', async () => {
        const { user } = render(<Harness initial={three} />);

        await user.click(screen.getByRole('button', { name: 'Move rule 3 up' }));
        expect(screen.getByRole('button', { name: 'Move rule 2 up' })).toHaveFocus();

        // Reaching the top disables that arrow, so focus goes to the other one.
        await user.click(screen.getByRole('button', { name: 'Move rule 2 up' }));
        expect(screen.getByRole('button', { name: 'Move rule 1 down' })).toHaveFocus();
    });

    it('offers a weight only on rules that split traffic with a neighbour', () => {
        render(
            <Harness
                initial={[
                    { destination_url: 'https://de.example', match_country: 'DE', weight: 1 },
                    { destination_url: 'https://a.example', weight: 3 },
                    { destination_url: 'https://b.example', weight: 1 },
                ]}
            />,
        );

        const weights = screen.getAllByLabelText(/weight/i);
        expect(weights).toHaveLength(2);
        expect(weights[0]).toHaveValue(3);
        expect(screen.getAllByText('A/B split')).toHaveLength(2);
    });

    it('does not show a weight for rules that are on their own', () => {
        render(<Harness initial={three} />);
        expect(screen.queryByLabelText(/weight/i)).not.toBeInTheDocument();
        expect(screen.queryByText('A/B split')).not.toBeInTheDocument();
    });

    it('turns two rules into an A/B split once their conditions match, and back when they differ', async () => {
        const { user } = render(
            <Harness
                initial={[
                    { destination_url: 'https://app.example', match_device: 'Mobile', weight: 1 },
                    { destination_url: 'https://web.example', weight: 1 },
                ]}
            />,
        );
        expect(screen.queryByText('A/B split')).not.toBeInTheDocument();
        expect(screen.queryByLabelText(/weight/i)).not.toBeInTheDocument();

        await user.selectOptions(screen.getAllByRole('combobox')[1], 'Mobile');
        expect(screen.getAllByText('A/B split')).toHaveLength(2);
        expect(screen.getAllByLabelText(/weight/i)).toHaveLength(2);

        await user.selectOptions(screen.getAllByRole('combobox')[1], 'Desktop');
        expect(screen.queryByText('A/B split')).not.toBeInTheDocument();
        expect(screen.queryByLabelText(/weight/i)).not.toBeInTheDocument();
    });

    it('limits the language to a primary code', () => {
        render(<Harness initial={three} />);
        for (const input of screen.getAllByLabelText('Language')) {
            expect(input).toHaveAttribute('maxlength', '3');
        }
    });
});
