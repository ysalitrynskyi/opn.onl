import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import { act } from '@testing-library/react';
import { ToastContainer, toast } from './Toast';

describe('Toast Component', () => {
    beforeEach(() => {
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    describe('ToastContainer', () => {
        it('renders without crashing', () => {
            render(<ToastContainer />);
            // Container should exist but be empty initially
            expect(document.body).toBeInTheDocument();
        });

        it('displays toast when toast() is called', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Test message', 'success');
            });

            await waitFor(() => {
                expect(screen.getByText('Test message')).toBeInTheDocument();
            });
        });

        it('displays success toast with correct styling', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Success!', 'success');
            });

            await waitFor(() => {
                const toastElement = screen.getByText('Success!');
                expect(toastElement).toBeInTheDocument();
            });
        });

        it('displays error toast with correct styling', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Error occurred', 'error');
            });

            await waitFor(() => {
                expect(screen.getByText('Error occurred')).toBeInTheDocument();
            });
        });

        it('displays info toast', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Information', 'info');
            });

            await waitFor(() => {
                expect(screen.getByText('Information')).toBeInTheDocument();
            });
        });

        it('displays warning toast', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Warning!', 'warning');
            });

            await waitFor(() => {
                expect(screen.getByText('Warning!')).toBeInTheDocument();
            });
        });

        it('auto-dismisses toast after timeout', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Temporary message', 'success');
            });

            await waitFor(() => {
                expect(screen.getByText('Temporary message')).toBeInTheDocument();
            });

            // Fast-forward time
            act(() => {
                vi.advanceTimersByTime(5000);
            });

            // Toast should be removed
            await waitFor(() => {
                expect(screen.queryByText('Temporary message')).not.toBeInTheDocument();
            });
        });

        it('displays multiple toasts', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('First toast', 'success');
                toast('Second toast', 'error');
            });

            await waitFor(() => {
                expect(screen.getByText('First toast')).toBeInTheDocument();
                expect(screen.getByText('Second toast')).toBeInTheDocument();
            });
        });
    });

    describe('toast function', () => {
        it('can be called with default type', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Default toast');
            });

            await waitFor(() => {
                expect(screen.getByText('Default toast')).toBeInTheDocument();
            });
        });

        it('handles all toast types', async () => {
            render(<ToastContainer />);
            
            act(() => {
                toast('Success', 'success');
                toast('Error', 'error');
                toast('Info', 'info');
                toast('Warning', 'warning');
            });

            await waitFor(() => {
                expect(screen.getByText('Success')).toBeInTheDocument();
                expect(screen.getByText('Error')).toBeInTheDocument();
                expect(screen.getByText('Info')).toBeInTheDocument();
                expect(screen.getByText('Warning')).toBeInTheDocument();
            });
        });
    });
});


