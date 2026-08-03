import { svelte } from '@sveltejs/vite-plugin-svelte';
import { svelteTesting } from '@testing-library/svelte/vite';
import { defineConfig } from 'vitest/config';

const tauriDevHost = process.env.TAURI_DEV_HOST;

export default defineConfig(({ mode }) => ({
  clearScreen: false,
  plugins: [svelte(), svelteTesting()],
  server: {
    host: tauriDevHost || false,
    port: 5173,
    strictPort: true,
    hmr: tauriDevHost
      ? {
          host: tauriDevHost,
          port: 1421,
          protocol: 'ws',
        }
      : undefined,
    watch: {
      ignored: ['**/src-tauri/**'],
    },
  },
  envPrefix: ['VITE_', 'TAURI_ENV_*'],
  ...(mode === 'test'
    ? {}
    : {
        esbuild: {
          // Svelte emits destructuring that WKWebView supports; esbuild cannot lower it for Safari 13.
          supported: {
            destructuring: true,
          },
        },
      }),
  build: {
    minify: process.env.TAURI_ENV_DEBUG ? false : 'esbuild',
    sourcemap: Boolean(process.env.TAURI_ENV_DEBUG),
    target: process.env.TAURI_ENV_PLATFORM === 'windows' ? 'chrome105' : 'safari13',
  },
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
}));
