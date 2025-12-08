import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Logo from './Logo';

describe('Logo', () => {
  it('renders the logo text by default (full version: OPeN.ONLine)', () => {
    render(<Logo />);
    // Default is showFull=true, so it shows OPeN.ONLine
    // Check for the presence of "OP" which is part of "OPeN"
    expect(screen.getByText('OP')).toBeInTheDocument();
    expect(screen.getByText('ONL')).toBeInTheDocument();
  });

  it('hides text when iconOnly is true', () => {
    render(<Logo iconOnly />);
    // With iconOnly, no text should be visible
    expect(screen.queryByText('OP')).not.toBeInTheDocument();
  });

  it('applies custom className', () => {
    const { container } = render(<Logo className="custom-class" />);
    expect(container.firstChild).toHaveClass('custom-class');
  });

  it('contains SVG icon', () => {
    const { container } = render(<Logo />);
    const svg = container.querySelector('svg');
    expect(svg).toBeInTheDocument();
  });
});



