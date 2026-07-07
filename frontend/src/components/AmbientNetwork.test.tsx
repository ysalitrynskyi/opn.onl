import { describe, it, expect } from 'vitest';
import { render } from '@testing-library/react';
import AmbientNetwork from './AmbientNetwork';

describe('AmbientNetwork', () => {
    it('renders a decorative canvas (aria-hidden) without crashing in jsdom', () => {
        // jsdom has no 2D canvas context; the component must bail gracefully
        // and leave the static background art to carry the design.
        const { container } = render(<AmbientNetwork className="test-class" />);
        const canvas = container.querySelector('canvas');
        expect(canvas).not.toBeNull();
        expect(canvas!.getAttribute('aria-hidden')).toBe('true');
        expect(canvas!.className).toBe('test-class');
    });

    it('unmounts cleanly', () => {
        const { unmount } = render(<AmbientNetwork />);
        expect(() => unmount()).not.toThrow();
    });
});
