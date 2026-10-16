import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
  base: './',
  server: {
    port: 5173,
    proxy: {
      // 可通过 VITE_PROXY_TARGET 指向其他端口的后端实例（默认 1994）
      '/api': process.env.VITE_PROXY_TARGET || 'http://localhost:1994',
      '/v1': process.env.VITE_PROXY_TARGET || 'http://localhost:1994',
      '/health': process.env.VITE_PROXY_TARGET || 'http://localhost:1994',
    },
  },
  build: {
    outDir: '../static',
    emptyOutDir: true,
  },
})
