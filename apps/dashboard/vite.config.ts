import tailwindcss from '@tailwindcss/vite';
import { sveltekit } from '@sveltejs/kit/vite';
import { defineConfig } from 'vitest/config';

// https://vite.dev/config/
export default defineConfig({
  plugins: [tailwindcss(), sveltekit()],
  resolve: {
    conditions: process.env.VITEST ? ['browser'] : undefined,
  },
  test: {
    environment: 'jsdom',
    setupFiles: ['./tests/setup.ts'],
  },
  server: {
    host: '127.0.0.1',
    proxy: {
      '/external': 'http://127.0.0.1:8080',
    },
  },
});
