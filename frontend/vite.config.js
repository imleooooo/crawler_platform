import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// https://vite.dev/config/
export default defineConfig({
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
            const key = process.env.API_KEY ?? ''
            if (key) proxyReq.setHeader('Authorization', `Bearer ${key}`)
          })
        },
      },
    },
  },
})
