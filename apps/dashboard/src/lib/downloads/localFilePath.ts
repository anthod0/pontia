export type LocalFileLink = { path: string; anchor: string | null };

const LINE_SUFFIX = /:\d+(?::\d+)?$/;
const LINE_FRAGMENT = /^L\d+(?:C\d+)?(?:-L?\d+(?:C\d+)?)?$/;
const SCHEME = /^[a-z][a-z\d+.-]*:/i;
const IMAGE_EXTENSION = /\.(?:png|jpe?g|gif|webp)$/i;

/**
 * Resolves a Markdown link to a local file. `base` is the absolute path of the
 * document containing the link, or a directory ending in `/`; without it only
 * absolute paths resolve.
 */
export function resolveLocalFileLink(
  href: string | undefined,
  base: string | null = null,
): LocalFileLink | null {
  if (!href) return null;
  const hashIndex = href.indexOf("#");
  const target = (hashIndex < 0 ? href : href.slice(0, hashIndex)).replace(LINE_SUFFIX, "");
  const fragment = hashIndex < 0 ? "" : href.slice(hashIndex + 1);
  if (SCHEME.test(target) || target.startsWith("//") || target.startsWith("?")) return null;
  if (!target.startsWith("/") && !base) return null;
  try {
    const url = new URL(target, base ? fileUrl(base) : "file:///");
    if (url.pathname.endsWith("/")) return null;
    const anchor = fragment && !LINE_FRAGMENT.test(fragment) ? decodeURIComponent(fragment) : null;
    return { path: decodeURIComponent(url.pathname), anchor };
  } catch {
    return null;
  }
}

export function isImagePath(path: string): boolean {
  return IMAGE_EXTENSION.test(path);
}

function fileUrl(path: string): string {
  return `file://${path.split("/").map(encodeURIComponent).join("/")}`;
}
