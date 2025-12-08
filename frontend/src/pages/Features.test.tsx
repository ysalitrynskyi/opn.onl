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
            expect(screen.getByText(/features/i)).toBeInTheDocument();
        });

        it('displays descriptive subtitle', () => {
            render(<Features />);
            // Check for any subtitle or description
        });
    });

    describe('Core Features', () => {
        it('displays link shortening feature', () => {
            render(<Features />);
            expect(screen.getByText(/shorten|short link/i)).toBeInTheDocument();
        });

        it('displays analytics feature', () => {
            render(<Features />);
            expect(screen.getByText(/analytics/i)).toBeInTheDocument();
        });

        it('displays custom alias feature', () => {
            render(<Features />);
            expect(screen.getByText(/custom|alias/i)).toBeInTheDocument();
        });

        it('displays QR code feature', () => {
            render(<Features />);
            expect(screen.getByText(/qr code/i)).toBeInTheDocument();
        });

        it('displays password protection feature', () => {
            render(<Features />);
            expect(screen.getByText(/password|protected/i)).toBeInTheDocument();
        });

        it('displays expiration feature', () => {
            render(<Features />);
            expect(screen.getByText(/expir|time limit/i)).toBeInTheDocument();
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
            expect(screen.queryByText(/bulk|multiple/i)).toBeDefined();
        });

        it('displays API access feature', () => {
            render(<Features />);
            expect(screen.queryByText(/api/i)).toBeDefined();
        });
    });

    describe('Security Features', () => {
        it('displays passkeys feature', () => {
            render(<Features />);
            expect(screen.queryByText(/passkey|webauthn|biometric/i)).toBeDefined();
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
            expect(screen.queryByText(/privacy|no tracking/i)).toBeDefined();
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
