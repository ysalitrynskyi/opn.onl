import { useEffect, useCallback } from 'react';

interface ShortcutHandler {
    key: string;
    ctrl?: boolean;
    alt?: boolean;
    shift?: boolean;
    handler: () => void;
    description: string;
}

export function useKeyboardShortcuts(shortcuts: ShortcutHandler[]) {
    const handleKeyDown = useCallback((event: KeyboardEvent) => {
        // Don't trigger shortcuts when typing in inputs
        const target = event.target as HTMLElement;
        if (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.isContentEditable) {
            return;
        }

        for (const shortcut of shortcuts) {
            const keyMatch = event.key.toLowerCase() === shortcut.key.toLowerCase();
            const ctrlMatch = shortcut.ctrl ? (event.ctrlKey || event.metaKey) : true;
            const altMatch = shortcut.alt ? event.altKey : !event.altKey;
            const shiftMatch = shortcut.shift ? event.shiftKey : !event.shiftKey;

            if (keyMatch && ctrlMatch && altMatch && shiftMatch) {
                event.preventDefault();
                shortcut.handler();
                return;
            }
        }
    }, [shortcuts]);

    useEffect(() => {
        window.addEventListener('keydown', handleKeyDown);
        return () => window.removeEventListener('keydown', handleKeyDown);
    }, [handleKeyDown]);
}

// Predefined shortcuts list for help modal
export const SHORTCUTS = [
    { key: 'n', description: 'Focus new link input' },
    { key: '/', description: 'Focus search' },
    { key: 'Escape', description: 'Close modal / Clear search' },
    { key: 'g d', description: 'Go to Dashboard' },
    { key: 'g s', description: 'Go to Settings' },
    { key: '?', description: 'Show keyboard shortcuts' },
];

export default useKeyboardShortcuts;


