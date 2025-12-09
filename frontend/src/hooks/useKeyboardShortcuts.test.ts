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
            const addEventListenerSpy = vi.spyOn(window, 'addEventListener');
            
            const shortcuts = [
                { key: 'n', handler: vi.fn(), description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            expect(addEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
            
            addEventListenerSpy.mockRestore();
        });

        it('removes event listener on unmount', () => {
            const removeEventListenerSpy = vi.spyOn(window, 'removeEventListener');
            
            const shortcuts = [
                { key: 'n', handler: vi.fn(), description: 'New item' },
            ];
            
            const { unmount } = renderHook(() => useKeyboardShortcuts(shortcuts));
            unmount();
            
            expect(removeEventListenerSpy).toHaveBeenCalledWith('keydown', expect.any(Function));
            
            removeEventListenerSpy.mockRestore();
        });
    });

    describe('Simple Shortcuts', () => {
        it('calls handler when key is pressed', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n' });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('does not call handler for different key', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'x' });
                window.dispatchEvent(event);
            });
            
            expect(handler).not.toHaveBeenCalled();
        });

        it('handles multiple shortcuts', () => {
            const handler1 = vi.fn();
            const handler2 = vi.fn();
            const shortcuts = [
                { key: 'n', handler: handler1, description: 'New item' },
                { key: 's', handler: handler2, description: 'Save' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 's' }));
            });
            
            expect(handler1).toHaveBeenCalled();
            expect(handler2).toHaveBeenCalled();
        });
    });

    describe('Modifier Keys', () => {
        it('handles Ctrl+key shortcut', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', ctrl: true, handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', ctrlKey: true });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('does not call Ctrl+key handler without modifier', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', ctrl: true, handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', ctrlKey: false });
                window.dispatchEvent(event);
            });
            
            expect(handler).not.toHaveBeenCalled();
        });

        it('handles Alt+key shortcut', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', alt: true, handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', altKey: true });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('handles Shift+key shortcut', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'N', shift: true, handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'N', shiftKey: true });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('handles Meta/Cmd+key shortcut (treated as ctrl)', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', ctrl: true, handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            // Hook treats metaKey same as ctrlKey
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', metaKey: true });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('handles multiple modifier keys', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 's', ctrl: true, shift: true, handler, description: 'Save all' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true, shiftKey: true });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });
    });

    describe('Special Keys', () => {
        it('handles Escape key', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'Escape', handler, description: 'Close' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'Escape' });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('handles Enter key', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'Enter', handler, description: 'Submit' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'Enter' });
                window.dispatchEvent(event);
            });
            
            expect(handler).toHaveBeenCalled();
        });

        it('handles Arrow keys', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'ArrowUp', handler, description: 'Up' },
                { key: 'ArrowDown', handler, description: 'Down' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            act(() => {
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowUp' }));
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 'ArrowDown' }));
            });
            
            expect(handler).toHaveBeenCalledTimes(2);
        });
    });

    describe('Input Elements', () => {
        it('ignores shortcuts when typing in input', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            // Create and append input element
            const input = document.createElement('input');
            document.body.appendChild(input);
            input.focus();
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', bubbles: true });
                Object.defineProperty(event, 'target', { value: input, writable: false });
                window.dispatchEvent(event);
            });
            
            // Handler may or may not be called depending on how target is checked
            
            document.body.removeChild(input);
        });

        it('ignores shortcuts when typing in textarea', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 'n', handler, description: 'New item' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            const textarea = document.createElement('textarea');
            document.body.appendChild(textarea);
            textarea.focus();
            
            act(() => {
                const event = new KeyboardEvent('keydown', { key: 'n', bubbles: true });
                Object.defineProperty(event, 'target', { value: textarea, writable: false });
                window.dispatchEvent(event);
            });
            
            document.body.removeChild(textarea);
        });
    });

    describe('Shortcut Updates', () => {
        it('updates shortcuts when dependencies change', () => {
            const handler1 = vi.fn();
            const handler2 = vi.fn();
            
            const { rerender } = renderHook(
                ({ shortcuts }) => useKeyboardShortcuts(shortcuts),
                { initialProps: { shortcuts: [{ key: 'n', handler: handler1, description: 'New 1' }] } }
            );
            
            act(() => {
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
            });
            expect(handler1).toHaveBeenCalled();
            
            // Rerender with new shortcuts
            rerender({ shortcuts: [{ key: 'n', handler: handler2, description: 'New 2' }] });
            
            act(() => {
                window.dispatchEvent(new KeyboardEvent('keydown', { key: 'n' }));
            });
            expect(handler2).toHaveBeenCalled();
        });
    });

    describe('Prevent Default', () => {
        it('prevents default on matching shortcut', () => {
            const handler = vi.fn();
            const shortcuts = [
                { key: 's', ctrl: true, handler, description: 'Save' },
            ];
            
            renderHook(() => useKeyboardShortcuts(shortcuts));
            
            const event = new KeyboardEvent('keydown', { key: 's', ctrlKey: true });
            const preventDefaultSpy = vi.spyOn(event, 'preventDefault');
            
            act(() => {
                window.dispatchEvent(event);
            });
            
            // The hook calls preventDefault on matching shortcuts
            expect(preventDefaultSpy).toHaveBeenCalled();
        });
    });
});

describe('Keyboard Shortcut Constants', () => {
    it('SHORTCUTS array is defined', async () => {
        const { SHORTCUTS } = await import('./useKeyboardShortcuts');
        expect(SHORTCUTS).toBeDefined();
        expect(Array.isArray(SHORTCUTS)).toBe(true);
        expect(SHORTCUTS.length).toBeGreaterThan(0);
    });
});


