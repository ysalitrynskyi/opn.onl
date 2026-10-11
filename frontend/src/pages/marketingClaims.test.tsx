import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Features from './Features';
import Pricing from './Pricing';

// Claims the product has no mechanism behind. Each used to be on the page.
describe('marketing copy', () => {
    it('does not offer support tiers or per-instance custom domains', () => {
        render(<Pricing />);

        // There is one contact address and one FRONTEND_URL; nothing is
        // prioritised and nothing maps several domains onto one instance.
        expect(screen.queryByText(/priority support/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/early access/i)).not.toBeInTheDocument();
        expect(screen.queryByText(/custom domain support/i)).not.toBeInTheDocument();
    });

    it('does not say A/B splits are measured', () => {
        render(<Features />);

        // Clicks are not attributed to a routing rule, so no variant is measured.
        const routing = screen.getByText(/one link, many destinations/i);
        expect(routing).toHaveTextContent(/weighted a\/b split/i);
        expect(routing).not.toHaveTextContent(/measured/i);
    });
});
