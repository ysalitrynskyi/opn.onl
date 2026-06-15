import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Logo from './Logo';

describe('Logo', () => {
  // The wordmark splits the dot into its own <span>, so the visible text is
  // assembled from multiple nodes. Match against the wordmark span's textContent.
  const wordmark = (text: string) => (_content: string, el: Element | null) =>
    el?.tagName === 'SPAN' && el.textContent === text;

  it('renders the short logo text by default (opn.onl)', () => {
    render(<Logo />);
    // Default is showFull=false, so it shows opn.onl (dot is a separate span)
    expect(screen.getByText(wordmark('opn.onl'))).toBeInTheDocument();
  });

  it('renders full version when showFull is true (OPeN.ONLine)', () => {
    render(<Logo showFull />);
    expect(screen.getByText(wordmark('OPeN.ONLine'))).toBeInTheDocument();
  });

  it('hides text when iconOnly is true', () => {
    render(<Logo iconOnly />);
    // With iconOnly, no wordmark text should be visible
    expect(screen.queryByText(wordmark('opn.onl'))).not.toBeInTheDocument();
  });

  it('applies custom className', () => {
    const { container } = render(<Logo className="custom-class" />);
    expect(container.firstChild).toHaveClass('custom-class');
  });

  it('contains the logo image', () => {
    render(<Logo />);
    const img = screen.getByAltText('opn.onl logo');
    expect(img).toBeInTheDocument();
  });
});



