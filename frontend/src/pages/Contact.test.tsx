import { describe, it, expect } from 'vitest';
import { render, screen } from '../test/test-utils';
import Contact from './Contact';

describe('Contact Page', () => {
  it('posts the form so a native submit cannot leak the message into the URL', () => {
    render(<Contact />);
    expect(screen.getByLabelText(/your name/i).closest('form')).toHaveAttribute('method', 'post');
  });
});
