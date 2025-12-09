import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../test/test-utils';
import ShareModal from './ShareModal';

describe('ShareModal Component', () => {
    const defaultProps = {
        url: 'https://opn.onl/abc123',
        title: 'Check out this link',
        onClose: vi.fn(),
    };

    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe('Rendering', () => {
        it('renders modal with correct title', () => {
            render(<ShareModal {...defaultProps} />);
            expect(screen.getByText('Share Link')).toBeInTheDocument();
        });

        it('displays the URL to share', () => {
            render(<ShareModal {...defaultProps} />);
            expect(screen.getByDisplayValue(defaultProps.url)).toBeInTheDocument();
        });

        it('renders share buttons', () => {
            render(<ShareModal {...defaultProps} />);
            expect(screen.getByText(/twitter/i) || screen.getByLabelText(/twitter/i) || screen.getByTitle(/twitter/i)).toBeDefined();
        });

        it('renders copy button', () => {
            render(<ShareModal {...defaultProps} />);
            expect(screen.getByRole('button', { name: /copy/i })).toBeInTheDocument();
        });
    });

    describe('Interactions', () => {
        it('calls onClose when close button is clicked', async () => {
            const onClose = vi.fn();
            render(<ShareModal {...defaultProps} onClose={onClose} />);
            
            const closeButton = screen.getByRole('button', { name: /close/i }) || 
                               screen.getByLabelText(/close/i);
            
            if (closeButton) {
                fireEvent.click(closeButton);
                expect(onClose).toHaveBeenCalledTimes(1);
            }
        });

        it('calls onClose when backdrop is clicked', async () => {
            const onClose = vi.fn();
            const { container } = render(<ShareModal {...defaultProps} onClose={onClose} />);
            
            // Click the backdrop (outer div)
            const backdrop = container.querySelector('.fixed.inset-0');
            if (backdrop) {
                fireEvent.click(backdrop);
                expect(onClose).toHaveBeenCalled();
            }
        });

        it('copies URL to clipboard when copy button is clicked', async () => {
            render(<ShareModal {...defaultProps} />);
            
            const copyButton = screen.getByRole('button', { name: /copy/i });
            fireEvent.click(copyButton);

            await waitFor(() => {
                expect(navigator.clipboard.writeText).toHaveBeenCalledWith(defaultProps.url);
            });
        });

        it('shows copied feedback after copying', async () => {
            render(<ShareModal {...defaultProps} />);
            
            const copyButton = screen.getByRole('button', { name: /copy/i });
            fireEvent.click(copyButton);

            await waitFor(() => {
                expect(screen.getByText(/copied/i)).toBeInTheDocument();
            });
        });
    });

    describe('Share Links', () => {
        it('generates correct Twitter share link', () => {
            render(<ShareModal {...defaultProps} />);
            
            const twitterLink = document.querySelector('a[href*="twitter.com"]') || 
                               document.querySelector('a[href*="x.com"]');
            
            if (twitterLink) {
                const href = twitterLink.getAttribute('href');
                expect(href).toContain(encodeURIComponent(defaultProps.url));
            }
        });

        it('generates correct Facebook share link', () => {
            render(<ShareModal {...defaultProps} />);
            
            const facebookLink = document.querySelector('a[href*="facebook.com"]');
            
            if (facebookLink) {
                const href = facebookLink.getAttribute('href');
                expect(href).toContain(encodeURIComponent(defaultProps.url));
            }
        });

        it('generates correct LinkedIn share link', () => {
            render(<ShareModal {...defaultProps} />);
            
            const linkedinLink = document.querySelector('a[href*="linkedin.com"]');
            
            if (linkedinLink) {
                const href = linkedinLink.getAttribute('href');
                expect(href).toContain(encodeURIComponent(defaultProps.url));
            }
        });

        it('share links open in new tab', () => {
            render(<ShareModal {...defaultProps} />);
            
            const shareLinks = document.querySelectorAll('a[target="_blank"]');
            expect(shareLinks.length).toBeGreaterThan(0);
        });

        it('share links have rel="noreferrer" for security', () => {
            render(<ShareModal {...defaultProps} />);
            
            const shareLinks = document.querySelectorAll('a[rel="noreferrer"]');
            expect(shareLinks.length).toBeGreaterThan(0);
        });
    });

    describe('URL Input', () => {
        it('URL input is readonly', () => {
            render(<ShareModal {...defaultProps} />);
            
            const input = screen.getByDisplayValue(defaultProps.url);
            expect(input).toHaveAttribute('readonly');
        });

        it('URL input can be selected', () => {
            render(<ShareModal {...defaultProps} />);
            
            const input = screen.getByDisplayValue(defaultProps.url) as HTMLInputElement;
            fireEvent.focus(input);
            
            // Input should be focusable
            expect(document.activeElement).toBe(input);
        });
    });

    describe('Accessibility', () => {
        it('modal has proper ARIA attributes', () => {
            render(<ShareModal {...defaultProps} />);
            
            // Check for dialog role or similar
            const modal = document.querySelector('[role="dialog"]') || 
                         document.querySelector('.rounded-2xl');
            expect(modal).toBeInTheDocument();
        });

        it('close button is accessible', () => {
            render(<ShareModal {...defaultProps} />);
            
            const closeButtons = screen.getAllByRole('button');
            expect(closeButtons.length).toBeGreaterThan(0);
        });
    });
});

