import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Logo from './Logo';

describe('Logo', () => {
  it('renders the logo text by default', () => {
    render(<Logo />);
    // Logo text is split into colored spans: opn.onl or OPeN.ONLine
    // Check for the presence of individual letter groups
    expect(screen.getByText('opn')).toBeInTheDocument();
  });

  it('hides text when iconOnly is true', () => {
    render(<Logo iconOnly />);
    // With iconOnly, no text should be visible
    expect(screen.queryByText('opn')).not.toBeInTheDocument();
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



