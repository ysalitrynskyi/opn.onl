import { Helmet } from 'react-helmet-async';

interface SEOProps {
  title?: string;
  description?: string;
  keywords?: string;
  image?: string;
  url?: string;
  type?: 'website' | 'article';
  noIndex?: boolean;
  schemaType?: 'WebSite' | 'WebApplication' | 'SoftwareApplication' | 'Organization' | 'FAQPage';
  faqItems?: { question: string; answer: string }[];
  breadcrumbs?: { name: string; url: string }[];
}

const BASE_URL = import.meta.env.VITE_FRONTEND_URL || 'https://opn.onl';
const DEFAULT_IMAGE = `${BASE_URL}/og-image.png`;
const GITHUB_URL = 'https://github.com/ysalitrynskyi/opn.onl';

export default function SEO({
  title = 'opn.onl - Open Source URL Shortener',
  description = 'Create short, memorable links with advanced analytics. Self-hostable, privacy-focused URL shortener built with Rust and React.',
  keywords = 'url shortener, link shortener, short links, analytics, open source, privacy, rust, react, free url shortener',
  image = DEFAULT_IMAGE,
  url = BASE_URL,
  type = 'website',
  noIndex = false,
  schemaType = 'WebApplication',
  faqItems,
  breadcrumbs,
}: SEOProps) {
  const fullTitle = title === 'opn.onl - Open Source URL Shortener' 
    ? title 
    : `${title} | opn.onl`;

  // Main schema
  const mainSchema = {
    '@context': 'https://schema.org',
    '@type': schemaType,
    name: 'opn.onl',
    description,
    url,
    ...(schemaType === 'WebApplication' && {
      applicationCategory: 'UtilityApplication',
      operatingSystem: 'Web',
      browserRequirements: 'Requires JavaScript',
      softwareVersion: '1.0.0',
      author: {
        '@type': 'Person',
        name: 'Yevhen Salitrynskyi',
        url: 'https://github.com/ysalitrynskyi',
      },
      offers: {
        '@type': 'Offer',
        price: '0',
        priceCurrency: 'USD',
        availability: 'https://schema.org/InStock',
      },
      featureList: [
        'URL shortening with custom aliases',
        'Detailed click analytics',
        'Geographic visitor tracking',
        'QR code generation',
        'Password protection',
        'Link expiration',
        'Team collaboration',
        'API access',
        'Bulk link creation',
        'CSV export',
      ],
      screenshot: `${BASE_URL}/og-image.png`,
      aggregateRating: {
        '@type': 'AggregateRating',
        ratingValue: '5',
        ratingCount: '1',
        bestRating: '5',
        worstRating: '1',
      },
    }),
    ...(schemaType === 'WebSite' && {
      potentialAction: {
        '@type': 'SearchAction',
        target: {
          '@type': 'EntryPoint',
          urlTemplate: `${BASE_URL}/dashboard?search={search_term_string}`,
        },
        'query-input': 'required name=search_term_string',
      },
    }),
  };

  // Organization schema (always included)
  const orgSchema = {
    '@context': 'https://schema.org',
    '@type': 'Organization',
    name: 'opn.onl',
    url: BASE_URL,
    logo: `${BASE_URL}/favicon.png`,
    description: 'Open source URL shortener with analytics',
    sameAs: [
      GITHUB_URL,
    ],
    founder: {
      '@type': 'Person',
      name: 'Yevhen Salitrynskyi',
    },
    foundingDate: '2024',
  };

  // FAQ schema (if faqItems provided)
  const faqSchema = faqItems ? {
    '@context': 'https://schema.org',
    '@type': 'FAQPage',
    mainEntity: faqItems.map(item => ({
      '@type': 'Question',
      name: item.question,
      acceptedAnswer: {
        '@type': 'Answer',
        text: item.answer,
      },
    })),
  } : null;

  // Breadcrumb schema (if breadcrumbs provided)
  const breadcrumbSchema = breadcrumbs ? {
    '@context': 'https://schema.org',
    '@type': 'BreadcrumbList',
    itemListElement: breadcrumbs.map((crumb, index) => ({
      '@type': 'ListItem',
      position: index + 1,
      name: crumb.name,
      item: crumb.url,
    })),
  } : null;

  // SoftwareSourceCode schema for open source
  const sourceCodeSchema = {
    '@context': 'https://schema.org',
    '@type': 'SoftwareSourceCode',
    name: 'opn.onl',
    codeRepository: GITHUB_URL,
    programmingLanguage: ['Rust', 'TypeScript', 'React'],
    license: 'https://www.gnu.org/licenses/agpl-3.0.en.html',
    runtimePlatform: 'Docker',
  };

  return (
    <Helmet>
      {/* Basic Meta Tags */}
      <title>{fullTitle}</title>
      <meta name="description" content={description} />
      <meta name="keywords" content={keywords} />
      <meta name="author" content="Yevhen Salitrynskyi" />
      <meta name="generator" content="opn.onl" />
      {noIndex && <meta name="robots" content="noindex, nofollow" />}
      
      {/* Open Graph / Facebook */}
      <meta property="og:type" content={type} />
      <meta property="og:url" content={url} />
      <meta property="og:title" content={fullTitle} />
      <meta property="og:description" content={description} />
      <meta property="og:image" content={image} />
      <meta property="og:image:width" content="1200" />
      <meta property="og:image:height" content="630" />
      <meta property="og:site_name" content="opn.onl" />
      <meta property="og:locale" content="en_US" />
      
      {/* Twitter */}
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:url" content={url} />
      <meta name="twitter:title" content={fullTitle} />
      <meta name="twitter:description" content={description} />
      <meta name="twitter:image" content={image} />
      <meta name="twitter:creator" content="@ysalitrynskyi" />
      
      {/* Additional SEO */}
      <meta name="theme-color" content="#3b82f6" />
      <meta name="application-name" content="opn.onl" />
      <meta name="apple-mobile-web-app-title" content="opn.onl" />
      <meta name="apple-mobile-web-app-capable" content="yes" />
      <meta name="mobile-web-app-capable" content="yes" />
      
      {/* Canonical URL */}
      <link rel="canonical" href={url} />
      
      {/* DNS Prefetch for external resources */}
      <link rel="dns-prefetch" href="//www.google-analytics.com" />
      
      {/* Schema.org JSON-LD - Multiple schemas */}
      <script type="application/ld+json">
        {JSON.stringify(mainSchema)}
      </script>
      <script type="application/ld+json">
        {JSON.stringify(orgSchema)}
      </script>
      <script type="application/ld+json">
        {JSON.stringify(sourceCodeSchema)}
      </script>
      {faqSchema && (
        <script type="application/ld+json">
          {JSON.stringify(faqSchema)}
        </script>
      )}
      {breadcrumbSchema && (
        <script type="application/ld+json">
          {JSON.stringify(breadcrumbSchema)}
        </script>
      )}
    </Helmet>
  );
}

