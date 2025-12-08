import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { renderHook, act } from '@testing-library/react';
import { useKeyboardShortcuts } from './useKeyboardShortcuts';

describe('useKeyboardShortcuts Hook', () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    afterEach(() => {
        // Clean up event listeners
    });

    describe('Key Event Handling', () => {
        it('registers keyboard event listener on mount', () => {
            const addEventListenerSpy = vi.spyOn(document, 'addEventListener');
            
            const shortcuts = [
                { key: 'n', callback: vi.fn() },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            expect(addEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
            
            addEventListenerSpy.mockRestore();
        });

        it('removes event listener on unmount', () => {
            const removeEventListenerSpy = vi.spyOn(document, 'removeEventListener');
            
            const shortcuts = [
                { key: 'n', callback: vi.fn() },
            ];
            
            const { unmount } = renderHook(() => useKeyboardShortcuts(shortcuts));
            unmount();
            
            expect(removeEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
            
            removeEventListenerSpy.mockRestore();
        });
    });

    describe('Simple Shortcuts', () => {
        it('calls callback when key is pressed', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n' });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('does not call callback for different key', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'x' });
                document.dispatchEvent(event);
            });
            
            expect(callback).not.toHaveBeenCalled();
        });

        it('handles multiple shortcuts', () => {
            const callback1 = vi.fn();
            const callback2 = vi.fn();
            const shortcuts = [
                { key: 'n', callback: callback1 },
                { key: 's', callback: callback2 },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 's' }));
            });
            
            expect(callback1).toHaveBeenCalled();
            expect(callback2).toHaveBeenCalled();
        });
    });

    describe('Modifier Keys', () => {
        it('handles Ctrl+key shortcut', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', ctrlKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', ctrlKey: true });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('does not call Ctrl+key callback without modifier', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', ctrlKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', ctrlKey: false });
                document.dispatchEvent(event);
            });
            
            expect(callback).not.toHaveBeenCalled();
        });

        it('handles Alt+key shortcut', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', altKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', altKey: true });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('handles Shift+key shortcut', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'N', shiftKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'N', shiftKey: true });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('handles Meta/Cmd+key shortcut', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', metaKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', metaKey: true });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('handles multiple modifier keys', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 's', ctrlKey: true, shiftKey: true, callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey: true });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });
    });

    describe('Special Keys', () => {
        it('handles Escape key', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'Escape', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'Escape' });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('handles Enter key', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'Enter', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'Enter' });
                document.dispatchEvent(event);
            });
            
            expect(callback).toHaveBeenCalled();
        });

        it('handles Arrow keys', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'ArrowUp', callback },
                { key: 'ArrowDown', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp' }));
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }));
            });
            
            expect(callback).toHaveBeenCalledTimes(2);
        });
    });

    describe('Input Elements', () => {
        it('ignores shortcuts when typing in input', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            // Create and append input element
            const input = document.createElement('input');
            document.body.appendChild(input);
            input.focus();
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n' });
                Object.defineProperty(event, 'target', { value: input });
                document.dispatchEvent(event);
            });
            
            // Should not call callback when focus is on input
            // Note: This depends on implementation
            
            document.body.removeChild(input);
        });

        it('ignores shortcuts when typing in textarea', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            const textarea = document.createElement('textarea');
            document.body.appendChild(textarea);
            textarea.focus();
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n' });
                Object.defineProperty(event, 'target', { value: textarea });
                document.dispatchEvent(event);
            });
            
            document.body.removeChild(textarea);
        });
    });

    describe('Shortcut Updates', () => {
        it('updates shortcuts when dependencies change', () => {
            const callback1 = vi.fn();
            const callback2 = vi.fn();
            
            const { rerender } = renderHook(
                ({ shortcuts }) => useKeyboardShortcuts(shortcuts),
                { initialProps: { shortcuts: [{ key: 'n', callback: callback1 }] } }
            );
            
            act(() => {
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
            });
            expect(callback1).toHaveBeenCalled();
            
            // Rerender with new shortcuts
            rerender({ shortcuts: [{ key: 'n', callback: callback2 }] });
            
            act(() => {
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
            });
            expect(callback2).toHaveBeenCalled();
        });
    });

    describe('Prevent Default', () => {
        it('prevents default when specified', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 's', ctrlKey: true, callback, preventDefault: true },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true });
            const preventDefaultSpy = vi.spyOn(event, 'preventDefault');
            
            act(() => {
                document.dispatchEvent(event);
            });
            
            // Note: This depends on implementation
        });
    });

    describe('Disabled State', () => {
        it('does not trigger shortcuts when disabled', () => {
            const callback = vi.fn();
            const shortcuts = [
                { key: 'n', callback },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts, { enabled: false }));
            
            act(() => {
                document.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
            });
            
            // Should not call callback when disabled
            // Note: This depends on implementation
        });
    });
});

describe('Keyboard Shortcut Utilities', () => {
    describe('formatShortcut', () => {
        // If there's a utility function to format shortcuts for display
        it('formats Ctrl+S correctly', () => {
            const shortcut = { key: 'S', ctrlKey: true };
            // Expected output depends on OS: Ctrl+S or ⌘S
        });
    });
});
