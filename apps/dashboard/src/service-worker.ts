/// <reference lib="webworker" />

import { base, build, files, version } from '$service-worker';

const worker = self as unknown as ServiceWorkerGlobalScope;
const CACHE_NAME = `pontia-dashboard-${version}`;
const APP_SHELL = `${base}/`;
const PRECACHED_ASSETS = new Set([...build, ...files]);

worker.addEventListener('install', (event) => {
  event.waitUntil(
    caches.open(CACHE_NAME).then((cache) => cache.addAll([...PRECACHED_ASSETS, APP_SHELL])),
  );
});

worker.addEventListener('activate', (event) => {
  event.waitUntil(
    caches.keys().then(async (cacheNames) => {
      await Promise.all(
        cacheNames
          .filter((cacheName) => cacheName.startsWith('pontia-dashboard-') && cacheName !== CACHE_NAME)
          .map((cacheName) => caches.delete(cacheName)),
      );
      await worker.clients.claim();
    }),
  );
});

worker.addEventListener('fetch', (event) => {
  const { request } = event;
  if (request.method !== 'GET') return;

  const url = new URL(request.url);
  const isDashboardRequest =
    url.origin === worker.location.origin &&
    (url.pathname === base || url.pathname.startsWith(`${base}/`));
  if (!isDashboardRequest) return;

  if (request.mode === 'navigate') {
    event.respondWith(networkFirstNavigation(request));
    return;
  }

  if (PRECACHED_ASSETS.has(url.pathname)) {
    event.respondWith(cacheFirst(url.pathname));
  }
});

async function networkFirstNavigation(request: Request): Promise<Response> {
  try {
    const response = await fetch(request);
    if (response.ok) {
      const cache = await caches.open(CACHE_NAME);
      await cache.put(APP_SHELL, response.clone());
    }
    return response;
  } catch (error) {
    const cache = await caches.open(CACHE_NAME);
    const cached = await cache.match(APP_SHELL);
    if (cached) return cached;
    throw error;
  }
}

async function cacheFirst(pathname: string): Promise<Response> {
  const cache = await caches.open(CACHE_NAME);
  const cached = await cache.match(pathname);
  return cached ?? fetch(pathname);
}
