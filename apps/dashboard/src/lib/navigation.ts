import { goto } from '$app/navigation';
import { base } from '$app/paths';

export function dashboardPath(path: string): string {
  if (!path.startsWith('/')) throw new Error(`Dashboard paths must be absolute: ${path}`);
  return `${base}${path === '/' ? '/' : path}`;
}

export async function navigate(path: string, query?: Record<string, string | null | undefined>): Promise<void> {
  const url = new URL(dashboardPath(path), window.location.origin);
  for (const [key, value] of Object.entries(query ?? {})) {
    if (value != null) url.searchParams.set(key, value);
  }
  await goto(`${url.pathname}${url.search}${url.hash}`);
  window.dispatchEvent(new PopStateEvent('popstate'));
}

export function dashboardRelativePath(pathname = window.location.pathname): string {
  return pathname.startsWith(base) ? pathname.slice(base.length) || '/' : pathname;
}

export function routeParam(segment: string, pathname = dashboardRelativePath()): string | null {
  const parts = pathname.split('/').filter(Boolean);
  const index = parts.indexOf(segment);
  return index >= 0 && index + 1 < parts.length ? decodeURIComponent(parts[index + 1]) : null;
}
