import { extractLocaleFromUrl, type Locale } from "$lib/paraglide/runtime";

/**
 * Locale of a page URL. Load functions use this instead of `getLocale()`, which reads the
 * browser's current location and lags behind during client-side navigation.
 */
export function pageLocale(url: URL): Locale {
  // The URL patterns match every page path, so a page URL always has a locale.
  return extractLocaleFromUrl(url)!;
}
