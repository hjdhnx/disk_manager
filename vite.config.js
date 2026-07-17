import { defineConfig } from 'vite'
import vue from '@vitejs/plugin-vue'

// Tauri dev server 固定端口，strictPort 防止 Vite 自动换端口导致 Tauri 连不上
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true
  },
  build: {
    target: 'es2021',
    outDir: 'dist',
    emptyOutDir: true
  }
})