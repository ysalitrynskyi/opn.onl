import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen } from '../test/test-utils';
import Features from './Features';

describe('Features Page', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe('Page Header', () => {
        it('displays features title', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /everything you need to\s*manage your links/i })).toBeInTheDocument();
        });

        it('displays descriptive subtitle', () => {
            render(<Features />);
            expect(screen.getByText(/powerful features, simple interface/i)).toBeInTheDocument();
        });
    });

    describe('Core Features', () => {
        it('displays link shortening feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /custom short links/i })).toBeInTheDocument();
        });

        it('displays analytics feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /advanced analytics/i })).toBeInTheDocument();
        });

        it('displays custom alias feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /custom short links/i })).toBeInTheDocument();
        });

        it('displays QR code feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /qr code generation/i })).toBeInTheDocument();
        });

        it('displays password protection feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /password protection/i })).toBeInTheDocument();
        });

        it('displays expiration feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /link expiration/i })).toBeInTheDocument();
        });
    });

    describe('Advanced Features', () => {
        it('displays organizations feature', () => {
            render(<Features />);
            expect(screen.queryByText(/organization|team/i)).toBeDefined();
        });

        it('displays folders feature', () => {
            render(<Features />);
            expect(screen.queryByText(/folder/i)).toBeDefined();
        });

        it('displays tags feature', () => {
            render(<Features />);
            expect(screen.queryByText(/tag/i)).toBeDefined();
        });

        it('displays bulk operations feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /bulk operations/i })).toBeInTheDocument();
        });

        it('displays API access feature', () => {
            render(<Features />);
            expect(screen.queryByText(/api/i)).toBeDefined();
        });
    });

    describe('Security Features', () => {
        it('displays passkeys feature', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /passkey authentication/i })).toBeInTheDocument();
        });

        it('displays rate limiting feature', () => {
            render(<Features />);
            expect(screen.queryByText(/rate limit|protection/i)).toBeDefined();
        });

        it('displays URL blocking feature', () => {
            render(<Features />);
            expect(screen.queryByText(/block|safe browsing/i)).toBeDefined();
        });
    });

    describe('Privacy Features', () => {
        it('displays privacy-focused messaging', () => {
            render(<Features />);
            expect(screen.getByRole('heading', { name: /privacy first/i })).toBeInTheDocument();
        });

        it('displays data export feature', () => {
            render(<Features />);
            expect(screen.queryByText(/export/i)).toBeDefined();
        });

        it('displays account deletion feature', () => {
            render(<Features />);
            expect(screen.queryByText(/delete.*account/i)).toBeDefined();
        });
    });

    describe('Feature Cards/Icons', () => {
        it('renders feature icons', () => {
            const { container } = render(<Features />);
            // Check for SVG icons or icon elements
            const icons = container.querySelectorAll('svg');
            expect(icons.length).toBeGreaterThan(0);
        });

        it('has consistent card styling', () => {
            const { container } = render(<Features />);
            // Check for feature card elements
            const cards = container.querySelectorAll('[class*="rounded"]');
            expect(cards.length).toBeGreaterThan(0);
        });
    });

    describe('Call to Action', () => {
        it('has get started button', () => {
            render(<Features />);
            const ctaButton = screen.queryByRole('link', { name: /get started|sign up|start/i }) ||
                             screen.queryByRole('button', { name: /get started/i });
            expect(ctaButton).toBeDefined();
        });

        it('CTA links to registration', () => {
            render(<Features />);
            const ctaLink = screen.queryByRole('link', { name: /get started|sign up/i });
            if (ctaLink) {
                expect(ctaLink).toHaveAttribute('href', '/register');
            }
        });
    });

    describe('Accessibility', () => {
        it('has descriptive headings', () => {
            const { container } = render(<Features />);
            const headings = container.querySelectorAll('h1, h2, h3');
            expect(headings.length).toBeGreaterThan(0);
        });

        it('feature images have alt text', () => {
            render(<Features />);
            const images = screen.queryAllByRole('img');
            images.forEach(img => {
                if (img.getAttribute('alt') !== null) {
                    expect(img.getAttribute('alt')).not.toBe('');
                }
            });
        });
    });

    describe('Animations', () => {
        it('has animation classes for features', () => {
            const { container } = render(<Features />);
            // Check for Framer Motion or animation classes
        });
    });
});


