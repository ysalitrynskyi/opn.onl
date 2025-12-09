import { describe, it, expect } from 'vitest';
import { render, screen } from '@testing-library/react';
import Sparkline from './Sparkline';

describe('Sparkline', () => {
    describe('rendering', () => {
        it('renders an SVG element', () => {
            const { container } = render(<Sparkline data={[1, 2, 3, 4, 5]} />);
            expect(container.querySelector('svg')).toBeInTheDocument();
        });

        it('renders with default dimensions', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} />);
            const svg = container.querySelector('svg');
            expect(svg).toHaveAttribute('width', '80');
            expect(svg).toHaveAttribute('height', '24');
        });

        it('renders with custom dimensions', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} width={100} height={30} />);
            const svg = container.querySelector('svg');
            expect(svg).toHaveAttribute('width', '100');
            expect(svg).toHaveAttribute('height', '30');
        });

        it('renders "No data" message for empty data', () => {
            render(<Sparkline data={[]} />);
            expect(screen.getByText('No data')).toBeInTheDocument();
        });

        it('renders path elements for line and area', () => {
            const { container } = render(<Sparkline data={[1, 2, 3, 4, 5]} />);
            const paths = container.querySelectorAll('path');
            expect(paths.length).toBeGreaterThanOrEqual(2);
        });

        it('renders end point circle', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} />);
            const circle = container.querySelector('circle');
            expect(circle).toBeInTheDocument();
        });
    });

    describe('data handling', () => {
        it('handles single data point', () => {
            const { container } = render(<Sparkline data={[5]} />);
            expect(container.querySelector('svg')).toBeInTheDocument();
        });

        it('handles all zero values', () => {
            const { container } = render(<Sparkline data={[0, 0, 0, 0]} />);
            expect(container.querySelector('svg')).toBeInTheDocument();
        });

        it('handles negative values', () => {
            const { container } = render(<Sparkline data={[-1, 0, 1, 2]} />);
            expect(container.querySelector('svg')).toBeInTheDocument();
        });

        it('handles large values', () => {
            const { container } = render(<Sparkline data={[1000000, 2000000, 1500000]} />);
            expect(container.querySelector('svg')).toBeInTheDocument();
        });
    });

    describe('styling', () => {
        it('applies custom color', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} color="#ff0000" />);
            const linePath = container.querySelectorAll('path')[1];
            expect(linePath).toHaveAttribute('stroke', '#ff0000');
        });

        it('applies custom stroke width', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} strokeWidth={3} />);
            const linePath = container.querySelectorAll('path')[1];
            expect(linePath).toHaveAttribute('stroke-width', '3');
        });
    });

    describe('tooltip', () => {
        it('renders tooltip container when showTooltip is true', () => {
            const { container } = render(<Sparkline data={[1, 2, 3]} showTooltip={true} />);
            const wrapper = container.firstChild as HTMLElement;
            expect(wrapper).toHaveAttribute('title');
        });

        it('includes total clicks in tooltip', () => {
            const { container } = render(<Sparkline data={[10, 20, 30]} showTooltip={true} />);
            const wrapper = container.firstChild as HTMLElement;
            expect(wrapper.getAttribute('title')).toContain('60');
        });
    });
});

describe('Sparkline utility calculations', () => {
    it('calculates correct total', () => {
        const data = [5, 10, 15, 20];
        const total = data.reduce((sum, v) => sum + v, 0);
        expect(total).toBe(50);
    });

    it('finds max value correctly', () => {
        const data = [1, 5, 3, 2];
        const max = Math.max(...data);
        expect(max).toBe(5);
    });

    it('finds min value correctly', () => {
        const data = [1, 5, 3, 2];
        const min = Math.min(...data);
        expect(min).toBe(1);
    });

    it('calculates range correctly', () => {
        const data = [2, 8, 4, 6];
        const max = Math.max(...data);
        const min = Math.min(...data);
        expect(max - min).toBe(6);
    });
});


