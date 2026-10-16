import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import type { LiveStatus } from '../../hooks/useLiveClicks';
import LiveClickIndicator from './LiveClickIndicator';

describe('LiveClickIndicator', () => {
    it.each<[LiveStatus, string]>([
        ['connected', 'Live'],
        ['reconnecting', 'Reconnecting'],
        ['off', 'Off'],
    ])('names the %s state %s', (status, label) => {
        render(<LiveClickIndicator status={status} />);
        const chip = screen.getByRole('status', { name: label });
        expect(chip).toHaveTextContent(label);
    });
});
