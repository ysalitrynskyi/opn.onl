// Post-build gate: assert the production build actually emitted every
// prerendered page — most importantly the home page at dist/index.html.
//
// Why this exists: @prerenderer/rollup-plugin handles the root route "/" by
// deleting the bundle's index.html and re-emitting it from the "/" render
// (node_modules/@prerenderer/rollup-plugin/dist/RollupPrerenderPlugin.js). If
// that re-emit is ever dropped, `vite build` still exits 0 and ships a dist
// with NO index.html. nginx then serves the base image's stock
// "Welcome to nginx!" page at "/". That took production down once (home page
// broken while every /subroute worked) with a completely green build. This
// turns that silent failure into a hard, loud build failure.
//
// Single source of truth: the route list is parsed out of vite.config.ts so it
// can never drift from what the prerenderer was told to render.

import { readFileSync, existsSync } from 'node:fs'
import { join, dirname } from 'node:path'
import { fileURLToPath } from 'node:url'

const here = dirname(fileURLToPath(import.meta.url))
const root = join(here, '..')
const distDir = join(root, 'dist')

function parsePrerenderRoutes() {
  const config = readFileSync(join(root, 'vite.config.ts'), 'utf8')
  const block = config.match(/const PRERENDER_ROUTES\s*=\s*\[([\s\S]*?)\]/)
  if (!block) {
    console.error('verify-prerender: could not find PRERENDER_ROUTES in vite.config.ts')
    process.exit(1)
  }
  const routes = [...block[1].matchAll(/['"]([^'"]+)['"]/g)].map((m) => m[1])
  if (routes.length === 0) {
    console.error('verify-prerender: PRERENDER_ROUTES parsed as empty')
    process.exit(1)
  }
  return routes
}

// Map a route to the file the prerenderer writes: "/" -> index.html,
// "/features" -> features/index.html (mirrors the plugin's own path logic).
function routeToFile(route) {
  const clean = route.replace(/^\/+/, '')
  return clean === '' ? 'index.html' : join(clean, 'index.html')
}

const routes = parsePrerenderRoutes()
const failures = []
const chartFailures = new Set()
const headFailures = []

// The sitemap that ships. Each prerendered page must canonicalise to exactly
// its own entry, or search engines get two answers for the same URL.
const sitemapPath = join(distDir, 'sitemap.xml')
const sitemap = existsSync(sitemapPath)
  ? [...readFileSync(sitemapPath, 'utf8').matchAll(/<loc>([^<]+)<\/loc>/g)].map((m) => m[1])
  : []

for (const route of routes) {
  const rel = routeToFile(route)
  const abs = join(distDir, rel)

  if (!existsSync(abs)) {
    failures.push(`${route} -> dist/${rel} MISSING`)
    continue
  }

  const html = readFileSync(abs, 'utf8')

  if (/Welcome to nginx/i.test(html)) {
    failures.push(`${route} -> dist/${rel} is the stock nginx welcome page`)
    continue
  }
  // The SPA mount point — every real page (shell or prerendered) has it.
  if (!html.includes('id="root"')) {
    failures.push(`${route} -> dist/${rel} has no SPA root div (not a real page)`)
    continue
  }
  // postProcess() in vite.config.ts stamps this onto every successfully
  // prerendered route; its absence means the render silently produced only a
  // raw shell (or nothing useful).
  if (!html.includes('data-prerendered')) {
    failures.push(`${route} -> dist/${rel} was not prerendered (no data-prerendered marker)`)
  }
  // Charts are drawn only on the lazy Analytics and Admin pages. A static
  // path from the entry into vendor-charts (it once swallowed React itself)
  // makes every page download ~340 KB it never runs.
  for (const [, asset] of html.matchAll(/<(?:link|script)\b[^>]*\b(?:href|src)="\/assets\/([^"]+\.js)"/g)) {
    if (asset.startsWith('vendor-charts-')) {
      chartFailures.add(`dist/${rel} loads ${asset} up front`)
      continue
    }
    const code = readFileSync(join(distDir, 'assets', asset), 'utf8')
    if (/\bfrom\s*["']\.\/vendor-charts-/.test(code) || /\bimport\s*["']\.\/vendor-charts-/.test(code)) {
      chartFailures.add(`${asset} statically imports vendor-charts`)
    }
  }
  // One set of head tags per page. index.html once carried its own
  // description, robots, Open Graph and title next to the ones SEO renders,
  // so every page shipped two of each.
  const head = html.slice(0, html.indexOf('</head>'))
  const count = (re) => (head.match(re) || []).length
  const titles = count(/<title\b/g)
  const descriptions = count(/<meta\b[^>]*\bname="description"/g)
  const canonicals = [...head.matchAll(/<link\b[^>]*\brel="canonical"[^>]*>/g)].map(
    (m) => (m[0].match(/\bhref="([^"]*)"/) || [])[1],
  )
  const expected = sitemap.find((loc) => new URL(loc).pathname === route)
  if (titles !== 1) headFailures.push(`${route}: ${titles} <title> elements`)
  if (descriptions !== 1) headFailures.push(`${route}: ${descriptions} meta descriptions`)
  if (count(/<meta\b[^>]*\bname="robots"/g) > 0) headFailures.push(`${route}: has a robots meta tag`)
  if (canonicals.length !== 1 || canonicals[0] !== expected) {
    headFailures.push(`${route}: canonical ${JSON.stringify(canonicals)}, sitemap entry ${expected ?? '(none)'}`)
  }
  const faqPages = (html.match(/"@type":"FAQPage"/g) || []).length
  if (faqPages > 1) headFailures.push(`${route}: ${faqPages} FAQPage JSON-LD blocks`)
}

if (failures.length > 0) {
  console.error('\nverify-prerender: FAILED — production build is missing prerendered pages:\n')
  for (const f of failures) console.error(`  ✗ ${f}`)
  console.error(
    '\nThis build must not be shipped: at least one route (often "/") would serve a\n' +
      'broken or stock page in production. Re-run the build; if it keeps failing,\n' +
      'the failing route likely crashes during headless prerender.\n'
  )
  process.exit(1)
}

if (headFailures.length > 0) {
  console.error('\nverify-prerender: FAILED — prerendered pages have duplicate or wrong head tags:\n')
  for (const f of headFailures) console.error(`  ✗ ${f}`)
  console.error(
    '\nPage-level tags come from src/components/SEO.tsx only. Check index.html for a\n' +
      'static copy, and public/sitemap.xml against the url passed to <SEO>.\n'
  )
  process.exit(1)
}

if (chartFailures.size > 0) {
  console.error('\nverify-prerender: FAILED — prerendered pages load the charts chunk up front:\n')
  for (const f of chartFailures) console.error(`  ✗ ${f}`)
  console.error(
    '\nOnly the lazy Analytics and Admin pages draw charts. Check the codeSplitting\n' +
      'groups in vite.config.ts: a module the entry needs has been pulled into\n' +
      'vendor-charts (React was, once).\n'
  )
  process.exit(1)
}

console.log(`verify-prerender: OK — ${routes.length} prerendered routes present and valid.`)
