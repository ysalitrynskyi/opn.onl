import { Helmet } from 'react-helmet-async';

interface SEOProps {
  title?: string;
  description?: string;
  keywords?: string;
  image?: string;
  url?: string;
  type?: 'website' | 'article';
  noIndex?: boolean;
  schemaType?: 'WebSite' | 'WebApplication' | 'SoftwareApplication' | 'Organization';
}

const BASE_URL = import.meta.env.VITE_FRONTEND_URL || 'https://opn.onl';
// Use PNG for OG image (better compatibility with Twitter/LinkedIn)
// Falls back to SVG if PNG not available
const DEFAULT_IMAGE = `${BASE_URL}/og-image.png`;

export default function SEO({
  title = 'opn.onl - Open Source URL Shortener',
  description = 'Create short, memorable links with advanced analytics. Self-hostable, privacy-focused URL shortener built with Rust and React.',
  keywords = 'url shortener, link shortener, short links, analytics, open source, privacy, rust, react',
  image = DEFAULT_IMAGE,
  url = BASE_URL,
  type = 'website',
  noIndex = false,
  schemaType = 'WebApplication',
}: SEOProps) {
  const fullTitle = title === 'opn.onl - Open Source URL Shortener' 
    ? title 
    : `${title} | opn.onl`;

  const schema = {
    '@context': 'https://schema.org',
    '@type': schemaType,
    name: 'opn.onl',
    description,
    url,
    ...(schemaType === 'WebApplication' && {
      applicationCategory: 'UtilityApplication',
      operatingSystem: 'Web',
      offers: {
        '@type': 'Offer',
        price: '0',
        priceCurrency: 'USD',
      },
      featureList: [
        'URL shortening',
        'Link analytics',
        'QR code generation',
        'Password protection',
        'Expiration dates',
        'Custom aliases',
        'Team collaboration',
      ],
    }),
    ...(schemaType === 'Organization' && {
      logo: `${BASE_URL}/logo.png`,
      sameAs: [
        'https://github.com/opn-onl/opn.onl',
      ],
    }),
  };

  return (
    <Helmet>
      {/* Basic Meta Tags */}
      <title>{fullTitle}</title>
      <meta name="description" content={description} />
      <meta name="keywords" content={keywords} />
      {noIndex && <meta name="robots" content="noindex, nofollow" />}
      
      {/* Open Graph / Facebook */}
      <meta property="og:type" content={type} />
      <meta property="og:url" content={url} />
      <meta property="og:title" content={fullTitle} />
      <meta property="og:description" content={description} />
      <meta property="og:image" content={image} />
      <meta property="og:site_name" content="opn.onl" />
      
      {/* Twitter */}
      <meta name="twitter:card" content="summary_large_image" />
      <meta name="twitter:url" content={url} />
      <meta name="twitter:title" content={fullTitle} />
      <meta name="twitter:description" content={description} />
      <meta name="twitter:image" content={image} />
      
      {/* Canonical URL */}
      <link rel="canonical" href={url} />
      
      {/* Schema.org JSON-LD */}
      <script type="application/ld+json">
        {JSON.stringify(schema)}
      </script>
    </Helmet>
  );
}

