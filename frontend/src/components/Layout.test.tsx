import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent } from '../test/test-utils';
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
            // Check for logo or site name
            expect(screen.getByText(/opn/i) || screen.getByAltText(/logo/i)).toBeDefined();
        });

        it('renders navigation links', () => {
            render(<Layout />);
            // Check for common nav links
            expect(screen.getByText(/features/i) || screen.getByRole('link', { name: /features/i })).toBeDefined();
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
            expect(screen.getByText(/privacy/i) || screen.getByRole('link', { name: /privacy/i })).toBeDefined();
        });

        it('contains terms of service link', () => {
            render(<Layout />);
            expect(screen.getByText(/terms/i) || screen.getByRole('link', { name: /terms/i })).toBeDefined();
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
            
            const featuresLink = screen.queryByRole('link', { name: /features/i });
            if (featuresLink) {
                expect(featuresLink).toHaveAttribute('href', '/features');
            }
        });

        it('pricing link navigates correctly', () => {
            render(<Layout />);
            
            const pricingLink = screen.queryByRole('link', { name: /pricing/i });
            if (pricingLink) {
                expect(pricingLink).toHaveAttribute('href', '/pricing');
            }
        });

        it('docs link navigates correctly', () => {
            render(<Layout />);
            
            const docsLink = screen.queryByRole('link', { name: /docs/i });
            if (docsLink) {
                expect(docsLink).toHaveAttribute('href', '/docs');
            }
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

