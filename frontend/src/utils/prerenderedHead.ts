/**
 * Attribute carried by every <head> tag the SEO component renders.
 */
export const SEO_TAG_ATTRIBUTE = 'data-seo';

/**
 * Remove the <head> tags a prerendered page was shipped with.
 *
 * The production build prerenders each marketing page, so its HTML already
 * holds the title, description, canonical and social tags of the route it was
 * rendered for. App routes (dashboard, settings, short-link pages, 404) are
 * served the prerendered home page as their SPA shell, so they arrive with the
 * home page's tags. createRoot does not adopt any of these: the client render
 * adds its own set next to them, which left two descriptions and two titles on
 * every page, and the home canonical on noindex app pages.
 *
 * Call this once, before the first render. The SEO component then renders the
 * only set the page keeps.
 */
export function dropPrerenderedHeadTags(doc: Document = document): void {
    doc.head.querySelectorAll(`[${SEO_TAG_ATTRIBUTE}]`).forEach((el) => el.remove());
}
