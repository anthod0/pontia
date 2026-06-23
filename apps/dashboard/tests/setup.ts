import '@testing-library/jest-dom/vitest';
import { cleanup } from '@testing-library/svelte';
import { afterEach, vi } from 'vitest';

Object.defineProperty(window, 'matchMedia', {
  writable: true,
  value: (query: string) => ({
    matches: false,
    media: query,
    onchange: null,
    addListener: () => {},
    removeListener: () => {},
    addEventListener: () => {},
    removeEventListener: () => {},
    dispatchEvent: () => false,
  }),
});

class TestResizeObserver {
  observe() {}
  unobserve() {}
  disconnect() {}
}

Object.defineProperty(window, 'ResizeObserver', { writable: true, value: TestResizeObserver });
Object.defineProperty(globalThis, 'ResizeObserver', { writable: true, value: TestResizeObserver });
Object.defineProperty(window, 'scrollTo', { writable: true, value: () => {} });

afterEach(async () => {
  cleanup();

  // bits-ui schedules body scroll-lock cleanup with a short timeout when overlays unmount.
  // Let that cleanup run before Vitest tears down jsdom, otherwise it can fire after
  // `document` has been removed and surface as an unhandled error.
  vi.useRealTimers();
  await new Promise((resolve) => window.setTimeout(resolve, 30));
});
