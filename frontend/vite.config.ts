import { readFileSync } from 'node:fs'
import { fileURLToPath } from 'node:url'
import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

// 版本号单一来源：Cargo.toml（后端与桌面壳共用）
const cargoVersion = readFileSync(fileURLToPath(new URL('../Cargo.toml', import.meta.url)), 'utf8')
  .match(/^version = "([^"]+)"/m)?.[1] ?? '0.0.0'

export default defineConfig({
  // 构建时间戳：关于分组要显示"这份界面是什么时候构建的"，运行时问不出来
  define: {
    __APP_VERSION__: JSON.stringify(cargoVersion),
    __BUILD_TIME__: JSON.stringify(new Date().toISOString()),
  },
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
