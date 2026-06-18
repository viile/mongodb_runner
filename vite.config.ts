import { defineConfig } from 'vite';
import vue from '@vitejs/plugin-vue';

/**
 * Tauri-only：前端是纯静态资源，所有 MongoDB / LLM 调用都走 Rust 侧的 tauri invoke。
 *
 * - dev: tauri 通过 `beforeDevCommand: npm run dev:web` 起 vite，再把 WebView 指向 5174
 * - build: tauri 通过 `beforeBuildCommand: npm run build` 出 dist/，再打成桌面包
 */
export default defineConfig({
  plugins: [vue()],
  clearScreen: false,
  server: {
    port: 5174,
    strictPort: true,
  },
  envPrefix: ['VITE_', 'TAURI_'],
  build: {
    target: 'es2021',
    minify: 'esbuild',
    sourcemap: false,
  },
});
