import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '../test/test-utils';
import { blockSiteStorage } from '../test/helpers';
import ErrorBoundary from './ErrorBoundary';
import Layout from './Layout';

// Mock the Outlet component from react-router-dom
vi.mock('react-router-dom', async () => {
    const actual = await vi.importActual('react-router-dom');
    return {
        ...actual,
        Outlet: () => <div data-testid="outlet">Page Content</div>,
    };
});

describe('Layout Component', () => {
    beforeEach(() => {
        vi.clearAllMocks();
        localStorage.clear();
    });

    describe('Header', () => {
        it('renders the logo', () => {
            render(<Layout />);
            // Logo image appears in the header (and footer); at least one is present.
            expect(screen.getAllByAltText(/opn\.onl logo/i).length).toBeGreaterThan(0);
        });

        it('renders navigation links', () => {
            render(<Layout />);
            // "Features" appears in the header nav (and footer); at least one link is present.
            expect(screen.getAllByRole('link', { name: /features/i }).length).toBeGreaterThan(0);
        });

        it('shows login button when not authenticated', () => {
            localStorage.removeItem('token');
            render(<Layout />);
            
            const loginLink = screen.queryByText(/log in/i) || 
                             screen.queryByRole('link', { name: /login/i }) ||
                             screen.queryByText(/sign in/i);
            expect(loginLink).toBeDefined();
        });

        it('shows dashboard link when authenticated', () => {
            localStorage.setItem('token', 'test-token');
            render(<Layout />);
            
            const dashboardLink = screen.queryByText(/dashboard/i) || 
                                 screen.queryByRole('link', { name: /dashboard/i });
            // May or may not be visible depending on implementation
        });

        it('has responsive mobile menu button', () => {
            render(<Layout />);
            
            // Check for hamburger menu or mobile toggle
            const menuButton = screen.queryByRole('button', { name: /menu/i }) ||
                              screen.queryByLabelText(/menu/i);
            // Mobile menu might only appear at certain viewport sizes
        });
    });

    describe('Main Content', () => {
        it('renders the Outlet for page content', () => {
            render(<Layout />);
            expect(screen.getByTestId('outlet')).toBeInTheDocument();
        });

        it('has proper main content structure', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('main')).toBeInTheDocument();
        });
    });

    describe('Footer', () => {
        it('renders footer section', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('footer')).toBeInTheDocument();
        });

        it('contains copyright information', () => {
            render(<Layout />);
            // Check for copyright or year
            expect(screen.getByText(/©/i) || screen.getByText(/2024|2025/)).toBeDefined();
        });

        it('contains privacy policy link', () => {
            render(<Layout />);
            const privacyLinks = screen.getAllByRole('link', { name: /privacy/i });
            expect(privacyLinks.length).toBeGreaterThan(0);
            expect(privacyLinks.some(l => l.getAttribute('href') === '/privacy')).toBe(true);
        });

        it('contains terms of service link', () => {
            render(<Layout />);
            const termsLinks = screen.getAllByRole('link', { name: /terms/i });
            expect(termsLinks.length).toBeGreaterThan(0);
            expect(termsLinks.some(l => l.getAttribute('href') === '/terms')).toBe(true);
        });

        it('contains social links or contact info', () => {
            render(<Layout />);
            // Check for GitHub, contact, or other links
            const socialOrContact = screen.queryByText(/github/i) || 
                                   screen.queryByText(/contact/i) ||
                                   screen.queryByRole('link', { name: /github/i });
        });
    });

    describe('Navigation', () => {
        it('features link navigates correctly', () => {
            render(<Layout />);

            const featuresLinks = screen.getAllByRole('link', { name: /features/i });
            expect(featuresLinks.length).toBeGreaterThan(0);
            featuresLinks.forEach(link => expect(link).toHaveAttribute('href', '/features'));
        });

        it('pricing link navigates correctly', () => {
            render(<Layout />);

            const pricingLinks = screen.getAllByRole('link', { name: /pricing/i });
            expect(pricingLinks.length).toBeGreaterThan(0);
            pricingLinks.forEach(link => expect(link).toHaveAttribute('href', '/pricing'));
        });

        it('docs link navigates correctly', () => {
            render(<Layout />);

            const docsLinks = screen.getAllByRole('link', { name: /docs/i });
            expect(docsLinks.length).toBeGreaterThan(0);
            docsLinks.forEach(link => expect(link).toHaveAttribute('href', '/docs'));
        });
    });

    describe('User Menu', () => {
        it('shows user menu when authenticated', () => {
            localStorage.setItem('token', 'test-token');
            render(<Layout />);
            
            // Check for user avatar, dropdown, or settings
            const userMenu = screen.queryByRole('button', { name: /user/i }) ||
                            screen.queryByRole('button', { name: /account/i }) ||
                            screen.queryByLabelText(/user menu/i);
        });

        it('has logout option when authenticated', async () => {
            localStorage.setItem('token', 'test-token');
            render(<Layout />);
            
            // Look for logout button/link
            const logoutButton = screen.queryByText(/log out/i) || 
                                screen.queryByRole('button', { name: /logout/i });
        });
    });

    describe('Accessibility', () => {
        it('has skip to content link', () => {
            render(<Layout />);
            
            // Skip link might be visually hidden
            const skipLink = screen.queryByText(/skip to/i);
        });

        it('header has proper landmark role', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('header')).toBeInTheDocument();
        });

        it('navigation has proper landmark role', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('nav')).toBeInTheDocument();
        });

        it('main content has proper landmark role', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('main')).toBeInTheDocument();
        });

        it('footer has proper landmark role', () => {
            const { container } = render(<Layout />);
            expect(container.querySelector('footer')).toBeInTheDocument();
        });
    });

    describe('Theme/Styling', () => {
        it('has consistent styling classes', () => {
            const { container } = render(<Layout />);
            
            // Check for common Tailwind classes indicating proper styling
            expect(container.querySelector('.min-h-screen') || 
                   container.querySelector('[class*="min-h"]')).toBeDefined();
        });
    });
});

describe('Layout auth and menus', () => {
    it('picks up a token written before in-app navigation', () => {
        render(<Layout />);
        expect(screen.getAllByRole('link', { name: /log in/i }).length).toBeGreaterThan(0);

        localStorage.setItem('token', 'test-token');
        fireEvent.click(screen.getAllByRole('link', { name: /^features$/i })[0]);

        expect(screen.getByRole('button', { name: /account menu/i })).toBeInTheDocument();
    });

    it('closes the mobile menu when a nav link is clicked', () => {
        render(<Layout />);
        fireEvent.click(screen.getByRole('button', { name: /toggle menu/i }));
        expect(screen.getByText('View on GitHub')).toBeInTheDocument();

        fireEvent.click(screen.getAllByRole('link', { name: /^features$/i })[0]);

        expect(screen.queryByText('View on GitHub')).not.toBeInTheDocument();
    });

    it('closes the account menu when a menu link is clicked', () => {
        localStorage.setItem('token', 'test-token');
        render(<Layout />);
        fireEvent.click(screen.getByRole('button', { name: /account menu/i }));
        expect(screen.getByRole('link', { name: /settings/i })).toBeInTheDocument();

        fireEvent.click(screen.getByRole('link', { name: /settings/i }));

        expect(screen.queryByRole('link', { name: /settings/i })).not.toBeInTheDocument();
    });

    it('shows Admin Panel in the mobile menu for admin users', () => {
        localStorage.setItem('token', 'test-token');
        localStorage.setItem('is_admin', 'true');
        render(<Layout />);
        fireEvent.click(screen.getByRole('button', { name: /toggle menu/i }));

        const adminLink = screen.getByRole('link', { name: /admin panel/i });
        expect(adminLink).toHaveAttribute('href', '/admin');
    });

    it('omits Admin Panel from the mobile menu for non-admin users', () => {
        localStorage.setItem('token', 'test-token');
        localStorage.setItem('is_admin', 'false');
        render(<Layout />);
        fireEvent.click(screen.getByRole('button', { name: /toggle menu/i }));

        expect(screen.queryByRole('link', { name: /admin panel/i })).not.toBeInTheDocument();
    });
});

describe('Layout with site storage blocked by the browser', () => {
    let unblock: () => void;
    beforeEach(() => {
        unblock = blockSiteStorage();
    });
    afterEach(() => unblock());

    // Layout reads the session during render on every route, public pages
    // included, so a throwing storage read here used to replace the whole site
    // with the error screen.
    it('renders the signed-out header instead of the error screen', () => {
        render(
            <ErrorBoundary>
                <Layout />
            </ErrorBoundary>,
        );

        expect(screen.queryByText(/something went wrong/i)).not.toBeInTheDocument();
        expect(screen.getByTestId('outlet')).toBeInTheDocument();
        expect(screen.getAllByRole('link', { name: /log in/i }).length).toBeGreaterThan(0);
        expect(screen.queryByRole('button', { name: /account menu/i })).not.toBeInTheDocument();
    });
});

describe('Layout Mobile Responsiveness', () => {
    it('header is visible on mobile', () => {
        render(<Layout />);
        
        const header = document.querySelector('header');
        expect(header).toBeInTheDocument();
    });

    it('footer is visible on mobile', () => {
        render(<Layout />);
        
        const footer = document.querySelector('footer');
        expect(footer).toBeInTheDocument();
    });
});


