export interface LocalhostPreviewTarget {
  port: number;
  redirect: string;
  rawHref: string;
}

export function parseLocalhostPreviewTarget(href?: string): LocalhostPreviewTarget | null {
  if (
    !href ||
    !/^http:\/\/(?:localhost|127\.0\.0\.1|\[::1\]|0\.0\.0\.0)(?::\d+)?(?=[/?#]|$)/i.test(href)
  )
    return null;
  const url = URL.parse(href);
  if (!url) return null;
  const port = Number(url.port || 80);
  return port ? { port, redirect: url.pathname + url.search + url.hash, rawHref: href } : null;
}
