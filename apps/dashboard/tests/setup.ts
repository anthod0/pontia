import '@testing-library/jest-dom/vitest';
import 'fake-indexeddb/auto';
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
Object.defineProperty(document, 'elementFromPoint', { configurable: true, value: () => null });
Object.defineProperty(Range.prototype, 'getClientRects', { configurable: true, value: () => [] });
Object.defineProperty(Range.prototype, 'getBoundingClientRect', {
  configurable: true,
  value: () => ({ bottom: 0, height: 0, left: 0, right: 0, top: 0, width: 0, x: 0, y: 0, toJSON: () => ({}) }),
});

afterEach(async () => {
  cleanup();

  // bits-ui schedules body scroll-lock cleanup with a short timeout when overlays unmount.
  // Let that cleanup run before Vitest tears down jsdom, otherwise it can fire after
  // `document` has been removed and surface as an unhandled error.
  vi.useRealTimers();
  await new Promise((resolve) => window.setTimeout(resolve, 30));
});
