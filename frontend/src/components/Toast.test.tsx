import { describe, it, expect, vi, afterEach } from 'vitest';
import type { ReactNode } from 'react';
import { render, screen } from '../test/test-utils';
import { act } from '@testing-library/react';
import { ToastContainer, toast } from './Toast';

// framer-motion's <AnimatePresence> keeps exiting elements mounted until their
// exit animation (driven by requestAnimationFrame) completes, which never happens
// under fake timers. Stub it out so removal from state immediately unmounts the
// node — this lets us assert the auto-dismiss logic deterministically.
vi.mock('framer-motion', () => ({
    AnimatePresence: ({ children }: { children: ReactNode }) => <>{children}</>,
    motion: new Proxy({} as Record<string, unknown>, {
        get: (_t, tag: string) =>
            ({ children, ...props }: Record<string, unknown>) => {
                const Tag = tag as keyof JSX.IntrinsicElements;
                // Drop framer-only props that aren't valid DOM attributes.
                const { initial, animate, exit, transition, whileInView, whileHover, whileTap, viewport, variants, ...rest } = props;
                void initial; void animate; void exit; void transition; void whileInView;
                void whileHover; void whileTap; void viewport; void variants;
                return <Tag {...rest}>{children}</Tag>;
            },
    }),
}));

// NOTE: testing-library's `waitFor` polls with real timers, so it deadlocks when
// global fake timers are installed. `toast()` updates state synchronously, so we
// flush it with `act()` and assert immediately — no `waitFor` needed. Fake timers
// are scoped to the single test that needs to advance the auto-dismiss timeout.

describe('Toast Component', () => {
    afterEach(() => {
        vi.useRealTimers();
    });

    describe('ToastContainer', () => {
        it('renders without crashing', () => {
            render(<ToastContainer />);
            // Container should exist but be empty initially
            expect(document.body).toBeInTheDocument();
        });

        it('displays toast when toast() is called', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Test message', 'success');
            });

            expect(screen.getByText('Test message')).toBeInTheDocument();
        });

        it('displays success toast with correct styling', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Success!', 'success');
            });

            expect(screen.getByText('Success!')).toBeInTheDocument();
        });

        it('displays error toast with correct styling', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Error occurred', 'error');
            });

            expect(screen.getByText('Error occurred')).toBeInTheDocument();
        });

        it('displays info toast', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Information', 'info');
            });

            expect(screen.getByText('Information')).toBeInTheDocument();
        });

        it('displays warning toast', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Warning!', 'warning');
            });

            expect(screen.getByText('Warning!')).toBeInTheDocument();
        });

        it('auto-dismisses toast after timeout', () => {
            vi.useFakeTimers();
            render(<ToastContainer />);

            act(() => {
                toast('Temporary message', 'success');
            });

            expect(screen.getByText('Temporary message')).toBeInTheDocument();

            // Fast-forward past the toast duration (default 3000ms).
            act(() => {
                vi.advanceTimersByTime(5000);
            });

            expect(screen.queryByText('Temporary message')).not.toBeInTheDocument();
        });

        it('displays multiple toasts', () => {
            render(<ToastContainer />);

            act(() => {
                toast('First toast', 'success');
                toast('Second toast', 'error');
            });

            expect(screen.getByText('First toast')).toBeInTheDocument();
            expect(screen.getByText('Second toast')).toBeInTheDocument();
        });
    });

    describe('toast function', () => {
        it('can be called with default type', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Default toast');
            });

            expect(screen.getByText('Default toast')).toBeInTheDocument();
        });

        it('handles all toast types', () => {
            render(<ToastContainer />);

            act(() => {
                toast('Success', 'success');
                toast('Error', 'error');
                toast('Info', 'info');
                toast('Warning', 'warning');
            });

            expect(screen.getByText('Success')).toBeInTheDocument();
            expect(screen.getByText('Error')).toBeInTheDocument();
            expect(screen.getByText('Info')).toBeInTheDocument();
            expect(screen.getByText('Warning')).toBeInTheDocument();
        });
    });
});
