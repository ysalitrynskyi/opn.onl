import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Logo from './Logo';

describe('Logo', () => {
  it('renders the logo text by default', () => {
    render(<Logo />);
    expect(screen.getByText('opn.onl')).toBeInTheDocument();
  });

  it('hides text when iconOnly is true', () => {
    render(<Logo iconOnly />);
    expect(screen.queryByText('opn.onl')).not.toBeInTheDocument();
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



