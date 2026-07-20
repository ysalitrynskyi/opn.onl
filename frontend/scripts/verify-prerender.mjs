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

console.log(`verify-prerender: OK — ${routes.length} prerendered routes present and valid.`)
