import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '@testing-library/react';
import LinkPreviewCard from './LinkPreviewCard';

// Mock fetch
const mockFetch = vi.fn();
global.fetch = mockFetch;

describe('LinkPreviewCard', () => {
    beforeEach(() => {
        mockFetch.mockClear();
    });

    describe('loading state', () => {
        it('shows loading state initially', () => {
            mockFetch.mockImplementation(() => new Promise(() => {})); // Never resolves
            render(<LinkPreviewCard url="https://example.com" />);
            expect(screen.getByText('Loading preview...')).toBeInTheDocument();
        });
    });

    describe('successful fetch', () => {
        it('displays title from preview data', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                json: () => Promise.resolve({
                    url: 'https://example.com',
                    title: 'Example Website',
                    description: 'An example website',
                    image: null,
                    site_name: 'Example',
                    favicon: 'https://example.com/favicon.ico',
                }),
            });

            render(<LinkPreviewCard url="https://example.com" />);
            
            await waitFor(() => {
                expect(screen.getByText('Example Website')).toBeInTheDocument();
            });
        });

        it('displays description from preview data', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                json: () => Promise.resolve({
                    url: 'https://example.com',
                    title: 'Example',
                    description: 'This is a description',
                    image: null,
                    site_name: null,
                    favicon: null,
                }),
            });

            render(<LinkPreviewCard url="https://example.com" />);
            
            await waitFor(() => {
                expect(screen.getByText('This is a description')).toBeInTheDocument();
            });
        });

        it('displays site name when available', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                json: () => Promise.resolve({
                    url: 'https://example.com',
                    title: 'Page Title',
                    description: null,
                    image: null,
                    site_name: 'My Website',
                    favicon: null,
                }),
            });

            render(<LinkPreviewCard url="https://example.com" />);
            
            await waitFor(() => {
                expect(screen.getByText('My Website')).toBeInTheDocument();
            });
        });
    });

    describe('error handling', () => {
        it('shows domain on fetch error', async () => {
            mockFetch.mockRejectedValueOnce(new Error('Network error'));

            render(<LinkPreviewCard url="https://example.com/page" />);
            
            await waitFor(() => {
                expect(screen.getByText('example.com')).toBeInTheDocument();
            });
        });

        it('shows domain on non-ok response', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: false,
            });

            render(<LinkPreviewCard url="https://test.com" />);
            
            await waitFor(() => {
                expect(screen.getByText('test.com')).toBeInTheDocument();
            });
        });
    });

    describe('compact mode', () => {
        it('renders compact version', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                json: () => Promise.resolve({
                    url: 'https://example.com',
                    title: 'Example',
                    description: 'A description',
                    image: null,
                    site_name: null,
                    favicon: null,
                }),
            });

            const { container } = render(<LinkPreviewCard url="https://example.com" compact={true} />);
            
            await waitFor(() => {
                expect(screen.getByText('Example')).toBeInTheDocument();
            });
            
            // Compact version should not have the image section
            expect(container.querySelector('.h-32')).not.toBeInTheDocument();
        });
    });

    describe('custom className', () => {
        it('applies custom className', async () => {
            mockFetch.mockResolvedValueOnce({
                ok: true,
                json: () => Promise.resolve({
                    url: 'https://example.com',
                    title: 'Test',
                    description: null,
                    image: null,
                    site_name: null,
                    favicon: null,
                }),
            });

            const { container } = render(
                <LinkPreviewCard url="https://example.com" className="custom-class" />
            );
            
            await waitFor(() => {
                const wrapper = container.firstChild as HTMLElement;
                expect(wrapper.className).toContain('custom-class');
            });
        });
    });
});

describe('URL parsing', () => {
    it('extracts domain correctly', () => {
        const getDomain = (url: string) => {
            try {
                return new URL(url).hostname;
            } catch {
                return url;
            }
        };

        expect(getDomain('https://example.com/path')).toBe('example.com');
        expect(getDomain('http://sub.domain.org')).toBe('sub.domain.org');
        expect(getDomain('https://test.io:8080/page')).toBe('test.io');
    });

    it('handles invalid URLs gracefully', () => {
        const getDomain = (url: string) => {
            try {
                return new URL(url).hostname;
            } catch {
                return url;
            }
        };

        expect(getDomain('not-a-url')).toBe('not-a-url');
    });
});

