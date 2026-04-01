import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  base: '/vldb-demo/',
  plugins: [react()],
  server: {
    proxy: {
      '/api': {
        target: 'https://guixu.org',
        changeOrigin: true,
        secure: true,
      },
      '/api/reviews': {
        target: 'https://guixu.org',
        changeOrigin: true,
        secure: true,
      },
    },
  },
})
