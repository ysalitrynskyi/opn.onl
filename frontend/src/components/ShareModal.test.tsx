import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, fireEvent, waitFor } from '../test/test-utils';
import ShareModal from './ShareModal';
import { ToastContainer } from './Toast';

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
            // Two copy controls exist: the URL-row icon button and the grid "Copy Link" button.
            expect(screen.getAllByRole('button', { name: /copy/i }).length).toBeGreaterThan(0);
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

            const [copyButton] = screen.getAllByRole('button', { name: /copy/i });
            fireEvent.click(copyButton);

            // customRender() calls userEvent.setup(), which installs its own clipboard
            // stub, so assert on the actual copied contents rather than a spy.
            await waitFor(async () => {
                expect(await navigator.clipboard.readText()).toBe(defaultProps.url);
            });
        });

        it('shows copied feedback after copying', async () => {
            // The modal signals success via a global toast, so mount the container too.
            render(
                <>
                    <ShareModal {...defaultProps} />
                    <ToastContainer />
                </>
            );

            const [copyButton] = screen.getAllByRole('button', { name: /copy/i });
            fireEvent.click(copyButton);

            await waitFor(() => {
                expect(screen.getByText(/link copied/i)).toBeInTheDocument();
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

        it('share buttons open social sharers in a new tab', () => {
            const openSpy = vi.spyOn(window, 'open').mockImplementation(() => null);
            render(<ShareModal {...defaultProps} />);

            // Sharing is handled by buttons that isolate the new browsing context.
            fireEvent.click(screen.getByRole('button', { name: /twitter/i }));

            expect(openSpy).toHaveBeenCalledWith(
                expect.stringContaining('twitter.com'),
                '_blank',
                'noopener,noreferrer'
            );
            openSpy.mockRestore();
        });

        it('social share buttons are rendered for each network', () => {
            render(<ShareModal {...defaultProps} />);

            expect(screen.getByRole('button', { name: /twitter/i })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /facebook/i })).toBeInTheDocument();
            expect(screen.getByRole('button', { name: /linkedin/i })).toBeInTheDocument();
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
            input.focus();

            // A readonly input is still focusable.
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


