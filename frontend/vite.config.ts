import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import Prerenderer from '@prerenderer/rollup-plugin'
import puppeteerRenderer from '@prerenderer/renderer-puppeteer'

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
  '/login',
  '/register',
]

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  const isProduction = mode === 'production'
  const skipPrerender = process.env.SKIP_PRERENDER === 'true'
  
  return {
    plugins: [
      react(),
      // Replace GA ID placeholder in HTML
      {
        name: 'html-transform',
        transformIndexHtml(html) {
          return html.replace(/%VITE_GA_ID%/g, env.VITE_GA_ID || '')
        },
      },
      // Prerender static pages in production build (skip if SKIP_PRERENDER=true)
      isProduction && !skipPrerender && Prerenderer({
        routes: PRERENDER_ROUTES,
        renderer: puppeteerRenderer({
          maxConcurrentRoutes: 4,
          renderAfterTime: 500, // Wait for React to render
          headless: true,
        }),
        postProcess(renderedRoute) {
          // Add data attribute to mark prerendered pages
          renderedRoute.html = renderedRoute.html.replace(
            '<div id="root">',
            '<div id="root" data-prerendered="true">'
          )
          return renderedRoute
        },
      }),
    ].filter(Boolean),
    build: {
      rollupOptions: {
        output: {
          manualChunks: {
            // Vendor chunks - split large libraries
            'vendor-react': ['react', 'react-dom', 'react-router-dom'],
            'vendor-charts': ['recharts'],
            'vendor-motion': ['framer-motion'],
            'vendor-icons': ['lucide-react'],
          },
        },
      },
      // Increase chunk size warning limit (since we're splitting)
      chunkSizeWarningLimit: 500,
    },
  }
})
