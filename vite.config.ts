import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri cần dev server ở cổng cố định để `beforeDevCommand` khớp `devUrl`.
const HOST = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  // Testing-library renders Svelte components through the browser runtime;
  // explicitly prefer its browser condition over Svelte's server entry.
  resolve: {
    conditions: ['browser'],
  },
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    host: HOST || false,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  build: {
    outDir: 'dist',
    emptyOutDir: true,
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts', 'scripts/**/*.test.mjs'],
  },
});
