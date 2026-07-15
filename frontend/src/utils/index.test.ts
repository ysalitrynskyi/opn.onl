import { describe, expect, it } from 'vitest';
import { isSafeHttpUrl } from './index';

describe('isSafeHttpUrl', () => {
  it('allows http and https', () => {
    expect(isSafeHttpUrl('https://example.com/me')).toBe(true);
    expect(isSafeHttpUrl('http://example.com')).toBe(true);
  });

  it('rejects javascript and data schemes', () => {
    expect(isSafeHttpUrl('javascript:alert(1)')).toBe(false);
    expect(isSafeHttpUrl('data:text/html,hi')).toBe(false);
    expect(isSafeHttpUrl('file:///etc/passwd')).toBe(false);
  });

  it('rejects localhost-style hosts', () => {
    expect(isSafeHttpUrl('http://localhost/x')).toBe(false);
    expect(isSafeHttpUrl('https://app.local/')).toBe(false);
  });

  it('rejects empty/null', () => {
    expect(isSafeHttpUrl(null)).toBe(false);
    expect(isSafeHttpUrl(undefined)).toBe(false);
    expect(isSafeHttpUrl('')).toBe(false);
  });
});
