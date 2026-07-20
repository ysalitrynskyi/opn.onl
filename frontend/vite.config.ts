import { defineConfig, loadEnv } from 'vite'
import type { ResolvedConfig } from 'vite'
import react from '@vitejs/plugin-react'
import Prerenderer from '@prerenderer/rollup-plugin'
import PuppeteerRenderer from '@prerenderer/renderer-puppeteer'
import { existsSync, writeFileSync } from 'node:fs'
import { join, resolve } from 'node:path'

// Static routes to prerender (no auth required, mostly static content)
const PRERENDER_ROUTES = [
  '/',
  '/features',
  '/pricing',
  '/about',
  '/privacy',
  '/terms',
  '/contact',
  '/faq',
  '/docs',
  '/developers',
  '/login',
  '/register',
]

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const isProduction = mode === 'production'

  // @prerenderer/rollup-plugin treats the root route "/" specially: it deletes
  // the bundle's index.html and re-emits it from the "/" render. In practice
  // that re-emit does not persist — every production build ships a dist/ with
  // NO index.html, all the /subroute pages present. nginx then serves its stock
  // "Welcome to nginx!" page at "/", with a fully green build. This took prod
  // down once. We capture the home render here and re-materialize dist/index.html
  // ourselves in a post writeBundle step below, so the home page (with its SEO
  // prerender) can never silently vanish.
  let homeHtml: string | null = null
  let resolvedOutDir = 'dist'

  return {
    // Dev server: honor an externally assigned port (preview tooling sets
    // PORT); fall back to Vite's default otherwise.
    server: {
      port: Number(process.env.PORT) || 5173,
    },
    plugins: [
      react(),
      // Replace GA ID placeholder in HTML
      {
        name: 'html-transform',
        transformIndexHtml(html: string) {
          return html.replace(/%VITE_GA_ID%/g, env.VITE_GA_ID || '')
        },
      },
      // Prerender static pages in production build
      isProduction && Prerenderer({
        routes: PRERENDER_ROUTES,
        renderer: new PuppeteerRenderer({
          // Render routes one at a time so each Puppeteer page gets full CPU and
          // react-helmet-async reliably flushes per-page <title>/meta/JSON-LD
          // into the captured HTML before snapshot.
          maxConcurrentRoutes: 1,
          renderAfterTime: 2000,
          headless: true,
          // The build runs as root inside the Docker builder stage; newer
          // Debian Chromium refuses to start its sandbox there, so launch
          // fails with an empty "Failed to launch the browser process".
          // The container is throwaway and only renders our own pages, so
          // disabling the sandbox (the standard CI/Docker setup) is safe.
          args: [
            '--no-sandbox',
            '--disable-setuid-sandbox',
            '--disable-dev-shm-usage',
            '--disable-gpu',
          ],
        }),
        postProcess(renderedRoute) {
          // Add data attribute to mark prerendered pages
          renderedRoute.html = renderedRoute.html.replace(
            '<div id="root">',
            '<div id="root" data-prerendered="true">'
          )
          // Stash the home render so we can guarantee dist/index.html below,
          // regardless of the plugin dropping the root route's re-emit.
          if (renderedRoute.route === '/' || renderedRoute.route === '') {
            homeHtml = renderedRoute.html
          }
        },
      }),
      // Safety net: guarantee dist/index.html exists after the prerenderer runs.
      // Runs post-writeBundle (all files already on disk); if the prerenderer
      // dropped the root route's index.html, write the captured home render back.
      // If the home route never prerendered at all, fail loudly rather than ship
      // a build that would serve the stock nginx page at "/".
      isProduction && {
        name: 'ensure-home-index-html',
        apply: 'build' as const,
        configResolved(cfg: ResolvedConfig) {
          resolvedOutDir = resolve(cfg.root, cfg.build.outDir)
        },
        writeBundle: {
          order: 'post' as const,
          handler() {
            const indexPath = join(resolvedOutDir, 'index.html')
            if (existsSync(indexPath)) return
            if (!homeHtml) {
              throw new Error(
                'ensure-home-index-html: dist/index.html is missing and the home ' +
                  'route ("/") produced no prerender output — refusing to ship a build ' +
                  'that would serve the stock nginx page at "/".'
              )
            }
            writeFileSync(indexPath, homeHtml)
            console.log(
              'ensure-home-index-html: prerenderer dropped dist/index.html; ' +
                'restored it from the home render.'
            )
          },
        },
      },
    ].filter(Boolean),
    build: {
      rollupOptions: {
        output: {
          manualChunks(id) {
            // Vendor chunks - split large libraries
            if (
              id.includes('/node_modules/react/') ||
              id.includes('/node_modules/react-dom/') ||
              id.includes('/node_modules/react-router-dom/')
            ) {
              return 'vendor-react'
            }
            if (id.includes('/node_modules/recharts/')) {
              return 'vendor-charts'
            }
            if (id.includes('/node_modules/framer-motion/')) {
              return 'vendor-motion'
            }
            if (id.includes('/node_modules/lucide-react/')) {
              return 'vendor-icons'
            }
          },
        },
      },
      // Increase chunk size warning limit (since we're splitting)
      chunkSizeWarningLimit: 500,
    },
  }
})
