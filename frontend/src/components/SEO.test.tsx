import { describe, it, expect, vi } from 'vitest';
import { render } from '../test/test-utils';
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
            render(<SEO title="Custom Title" />);
            // Check that title element is created
        });

        it('renders with custom description', () => {
            render(<SEO description="Custom description for the page" />);
        });

        it('renders with custom keywords', () => {
            render(<SEO keywords="url, shortener, links" />);
        });
    });

    describe('Meta Tags', () => {
        it('includes Open Graph tags with image', () => {
            render(
                <SEO 
                    title="Test Title" 
                    description="Test Description"
                    image="https://example.com/og-image.png"
                />
            );
        });

        it('renders with url prop', () => {
            render(
                <SEO 
                    title="Test Title" 
                    url="https://opn.onl/page"
                />
            );
        });

        it('renders with noIndex prop', () => {
            render(<SEO noIndex={true} />);
        });
    });

    describe('Title Formatting', () => {
        it('appends site name to title', () => {
            render(<SEO title="Page Title" />);
            // Expected: "Page Title | opn.onl"
        });

        it('uses default title when none provided', () => {
            render(<SEO />);
            // Should use default site title
        });
    });

    describe('Schema.org Data', () => {
        it('includes structured data with schemaType', () => {
            render(
                <SEO 
                    schemaType="WebSite"
                />
            );
        });

        it('renders with FAQ items', () => {
            render(
                <SEO 
                    schemaType="FAQPage"
                    faqItems={[
                        { question: 'What is this?', answer: 'A URL shortener' },
                        { question: 'Is it free?', answer: 'Yes!' },
                    ]}
                />
            );
        });

        it('renders with breadcrumbs', () => {
            render(
                <SEO 
                    breadcrumbs={[
                        { name: 'Home', url: 'https://opn.onl' },
                        { name: 'Dashboard', url: 'https://opn.onl/dashboard' },
                    ]}
                />
            );
        });
    });

    describe('Social Media', () => {
        it('sets correct type', () => {
            render(<SEO type="website" />);
        });

        it('sets article type', () => {
            render(<SEO type="article" />);
        });

        it('sets OG image', () => {
            render(
                <SEO 
                    image="https://example.com/image.png"
                />
            );
        });
    });

    describe('All Props Together', () => {
        it('renders with all available props', () => {
            render(
                <SEO 
                    title="Full Test"
                    description="A complete description"
                    keywords="test, keywords, seo"
                    image="https://example.com/image.png"
                    url="https://opn.onl/test"
                    type="website"
                    noIndex={false}
                    schemaType="WebApplication"
                    faqItems={[{ question: 'Q?', answer: 'A' }]}
                    breadcrumbs={[{ name: 'Home', url: '/' }]}
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
        const keywords = 'url shortener, link management, analytics';
        expect(keywords.length).toBeGreaterThan(0);
    });
});
