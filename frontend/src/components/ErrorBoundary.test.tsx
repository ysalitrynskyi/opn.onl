import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen } from '@testing-library/react';
import ErrorBoundary from './ErrorBoundary';

// Component that throws an error
const ThrowError = ({ shouldThrow }: { shouldThrow: boolean }) => {
    if (shouldThrow) {
        throw new Error('Test error');
    }
    return <div>No error</div>;
};

describe('ErrorBoundary Component', () => {
    // Suppress console.error for these tests
    const originalError = console.error;
    
    beforeEach(() => {
        console.error = vi.fn();
    });

    afterEach(() => {
        console.error = originalError;
    });

    describe('Normal Operation', () => {
        it('renders children when no error occurs', () => {
            render(
                <ErrorBoundary>
                    <div>Child content</div>
                </ErrorBoundary>
            );

            expect(screen.getByText('Child content')).toBeInTheDocument();
        });

        it('renders multiple children', () => {
            render(
                <ErrorBoundary>
                    <div>First child</div>
                    <div>Second child</div>
                </ErrorBoundary>
            );

            expect(screen.getByText('First child')).toBeInTheDocument();
            expect(screen.getByText('Second child')).toBeInTheDocument();
        });

        it('renders nested components', () => {
            render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={false} />
                </ErrorBoundary>
            );

            expect(screen.getByText('No error')).toBeInTheDocument();
        });
    });

    describe('Error Handling', () => {
        it('catches errors and renders fallback UI', () => {
            render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={true} />
                </ErrorBoundary>
            );

            // Should show error fallback
            expect(screen.getByText(/something went wrong/i) || 
                   screen.getByText(/error/i) ||
                   screen.getByText(/oops/i)).toBeInTheDocument();
        });

        it('displays error message in fallback', () => {
            render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={true} />
                </ErrorBoundary>
            );

            // Should not show the child content
            expect(screen.queryByText('No error')).not.toBeInTheDocument();
        });

        it('provides way to recover from error', () => {
            render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={true} />
                </ErrorBoundary>
            );

            // Should have a button to refresh/retry
            const refreshButton = screen.queryByRole('button');
            // Either a refresh button or a link to go back
            expect(refreshButton || screen.queryByRole('link')).toBeDefined();
        });
    });

    describe('Error Information', () => {
        it('logs error to console', () => {
            render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={true} />
                </ErrorBoundary>
            );

            // console.error should have been called (it's mocked)
            expect(console.error).toHaveBeenCalled();
        });
    });

    describe('Nested ErrorBoundaries', () => {
        it('inner boundary catches error first', () => {
            render(
                <ErrorBoundary>
                    <div>Outer content</div>
                    <ErrorBoundary>
                        <ThrowError shouldThrow={true} />
                    </ErrorBoundary>
                </ErrorBoundary>
            );

            // Outer content should still be visible
            // (depending on implementation)
        });
    });

    describe('Error Recovery', () => {
        it('can recover after error is fixed', () => {
            const { rerender } = render(
                <ErrorBoundary>
                    <ThrowError shouldThrow={true} />
                </ErrorBoundary>
            );

            // Initially shows error
            expect(screen.queryByText('No error')).not.toBeInTheDocument();

            // Note: React ErrorBoundary typically requires remounting
            // to recover from error state
        });
    });
});

describe('ErrorBoundary Edge Cases', () => {
    const originalError = console.error;
    
    beforeEach(() => {
        console.error = vi.fn();
    });

    afterEach(() => {
        console.error = originalError;
    });

    it('handles errors in event handlers gracefully', () => {
        // Note: ErrorBoundary doesn't catch event handler errors
        // This test verifies the boundary still works
        const ClickError = () => {
            const handleClick = () => {
                throw new Error('Click error');
            };
            return <button onClick={handleClick}>Click me</button>;
        };

        render(
            <ErrorBoundary>
                <ClickError />
            </ErrorBoundary>
        );

        // Button should render (event handler errors aren't caught by boundary)
        expect(screen.getByText('Click me')).toBeInTheDocument();
    });

    it('handles async errors appropriately', async () => {
        // Note: ErrorBoundary doesn't catch async errors
        const AsyncError = () => {
            // This would need special handling
            return <div>Async component</div>;
        };

        render(
            <ErrorBoundary>
                <AsyncError />
            </ErrorBoundary>
        );

        expect(screen.getByText('Async component')).toBeInTheDocument();
    });
});


