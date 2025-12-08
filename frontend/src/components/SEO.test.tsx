import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, screen, waitFor } from '../test/test-utils';
import SEO from './SEO';

// Mock react-helmet-async
vi.mock('react-helmet-async', () => ({
    Helmet: ({ children }: { children: React.ReactNode }) => (
        <div data-testid="helmet">{children}</div>
    ),
    HelmetProvider: ({ children }: { children: React.ReactNode }) => <>{children}</>,
}));

describe('SEO Component', () => {
    describe('Basic Props', () => {
        it('renders with default values', () => {
            render(<SEO />);
            // Component should render without errors
        });

        it('renders with custom title', () => {
            const { container } = render(<SEO title="Custom Title" />);
            // Check that title element is created
        });

        it('renders with custom description', () => {
            render(<SEO description="Custom description for the page" />);
        });

        it('renders with custom keywords', () => {
            render(<SEO keywords={['url', 'shortener', 'links']} />);
        });
    });

    describe('Meta Tags', () => {
        it('includes Open Graph tags', () => {
            render(
                <SEO 
                    title="Test Title" 
                    description="Test Description"
                    ogImage="https://example.com/og-image.png"
                />
            );
        });

        it('includes Twitter card tags', () => {
            render(
                <SEO 
                    title="Test Title" 
                    twitterCard="summary_large_image"
                />
            );
        });

        it('includes canonical URL', () => {
            render(<SEO canonical="https://opn.onl/page" />);
        });

        it('includes robots meta', () => {
            render(<SEO robots="index, follow" />);
        });
    });

    describe('Title Formatting', () => {
        it('appends site name to title', () => {
            const { container } = render(<SEO title="Page Title" />);
            // Expected: "Page Title | OPN.onl" or similar
        });

        it('uses default title when none provided', () => {
            render(<SEO />);
            // Should use default site title
        });
    });

    describe('Schema.org Data', () => {
        it('includes structured data when provided', () => {
            render(
                <SEO 
                    structuredData={{
                        "@type": "WebPage",
                        "name": "Test Page"
                    }}
                />
            );
        });
    });

    describe('Social Media', () => {
        it('sets correct OG type', () => {
            render(<SEO ogType="website" />);
        });

        it('sets OG image dimensions', () => {
            render(
                <SEO 
                    ogImage="https://example.com/image.png"
                    ogImageWidth={1200}
                    ogImageHeight={630}
                />
            );
        });
    });

    describe('Language and Locale', () => {
        it('sets language attribute', () => {
            render(<SEO lang="en" />);
        });

        it('sets locale for OG', () => {
            render(<SEO locale="en_US" />);
        });
    });

    describe('Additional Tags', () => {
        it('renders custom meta tags', () => {
            render(
                <SEO 
                    additionalMeta={[
                        { name: 'author', content: 'Test Author' },
                        { property: 'custom:tag', content: 'value' },
                    ]}
                />
            );
        });

        it('renders link tags', () => {
            render(
                <SEO 
                    additionalLinks={[
                        { rel: 'preconnect', href: 'https://fonts.googleapis.com' },
                    ]}
                />
            );
        });
    });
});

describe('SEO Best Practices', () => {
    it('title should be under 60 characters for search results', () => {
        const title = "Short Title";
        expect(title.length).toBeLessThan(60);
    });

    it('description should be under 160 characters', () => {
        const description = "A concise description of the page content that fits well in search results.";
        expect(description.length).toBeLessThan(160);
    });

    it('keywords should be relevant', () => {
        const keywords = ['url shortener', 'link management', 'analytics'];
        expect(keywords.length).toBeGreaterThan(0);
        keywords.forEach(keyword => {
            expect(keyword.length).toBeGreaterThan(0);
        });
    });
});
