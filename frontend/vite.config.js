import { defineConfig, loadEnv } from 'vite'
import react from '@vitejs/plugin-react'
import { fileURLToPath } from 'node:url'

// https://vite.dev/config/
export default defineConfig(({ mode }) => {
  // loadEnv reads .env, .env.local, .env.<mode>, etc. from the project root.
  // The third argument '' makes it load ALL variables, not only VITE_* ones,
  // so API_KEY (no VITE_ prefix, never exposed to the browser) is available.
  // fileURLToPath converts the file: URL to a native OS path (handles Windows
  // drive letters and percent-encoded characters that .pathname would not).
  const repoRoot = fileURLToPath(new URL('..', import.meta.url))
  const env = loadEnv(mode, repoRoot, '')

  return {
    plugins: [react()],
    server: {
      proxy: {
        // In dev mode Vite proxies /api/* to the backend and injects the bearer
        // token here (Node.js process), so the key never enters the browser bundle.
        // In production the same injection is done by the nginx reverse proxy.
        '/api': {
          target: 'http://localhost:8000',
          changeOrigin: true,
          configure: (proxy) => {
            proxy.on('proxyReq', (proxyReq) => {
              if (env.API_KEY) proxyReq.setHeader('Authorization', `Bearer ${env.API_KEY}`)
            })
          },
        },
      },
    },
  }
})
