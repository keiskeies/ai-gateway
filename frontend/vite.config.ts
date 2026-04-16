import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  server: {
    port: 5173,
    proxy: {
      '/api': 'http://localhost:1994',
      '/v1': 'http://localhost:1994',
      '/health': 'http://localhost:1994',
    },
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
  },
})
