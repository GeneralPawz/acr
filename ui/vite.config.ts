/// <reference types="vitest/config" />
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vite';

// One ES entry: dist/acr-ui.js (+ acr-ui.css). public/index.html is the native host page.
export default defineConfig({
  base: './',
  plugins: [svelte(), svelteTesting()],
  define: { 'process.env.NODE_ENV': '"production"' },
  build: {
    lib: { entry: 'src/main.ts', formats: ['es'], fileName: () => 'acr-ui.js', cssFileName: 'acr-ui' },
    sourcemap: false,
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
});
