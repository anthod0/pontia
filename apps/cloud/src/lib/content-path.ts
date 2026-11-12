import { localeInfo } from "./locales";

export function parseContentPath(path: string) {
  const match = path
    .replaceAll("\\", "/")
    .match(/\/src\/content\/(?:docs|blog)\/([^/]+)\/(.+)\.svx$/);
  if (!match) return;
  const [, locale, slug] = match;
  if (!Object.hasOwn(localeInfo, locale)) {
    throw new Error(`Unknown content locale "${locale}" in ${path}`);
  }
  return { locale: locale as keyof typeof localeInfo, slug };
}
