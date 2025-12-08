import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Logo from './Logo';

describe('Logo', () => {
  it('renders the logo text by default (full version: OPeN.ONLine)', () => {
    render(<Logo />);
    // Default is showFull=true, so it shows OPeN.ONLine
    expect(screen.getByText('OPeN')).toBeInTheDocument();
    expect(screen.getByText('ONLine')).toBeInTheDocument();
  });

  it('renders short version when showFull is false', () => {
    render(<Logo showFull={false} />);
    expect(screen.getByText('opn')).toBeInTheDocument();
    expect(screen.getByText('onl')).toBeInTheDocument();
  });

  it('hides text when iconOnly is true', () => {
    render(<Logo iconOnly />);
    // With iconOnly, no text should be visible
    expect(screen.queryByText('OPeN')).not.toBeInTheDocument();
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



