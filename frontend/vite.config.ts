import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  const env = loadEnv(mode, process.cwd(), '')
  
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
    ],
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
