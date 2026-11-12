import { englishOnlyPages, localePrefixes } from "../src/lib/locales";

const translated = Object.entries(localePrefixes);
const englishOnly = `/:page(${englishOnlyPages.map((page) => page.slice(1)).join("|")})`;

export default {
  outdir: "./src/lib/paraglide",
  outputStructure: "message-modules",
  strategy: ["url", "baseLocale"],
  // API requests carry no locale in the URL; sign-in redirects use the locale of the page that started them.
  routeStrategies: [{ match: "/api/:path(.*)", strategy: ["cookie", "baseLocale"] }],
  urlPatterns: [
    {
      pattern: "/",
      localized: [["en", "/"], ...translated.map(([locale, prefix]) => [locale, `/${prefix}`])],
    },
    {
      pattern: englishOnly,
      localized: [["en", englishOnly], ...translated.map(([locale]) => [locale, englishOnly])],
    },
    {
      pattern: "/:path(.*)?",
      localized: [
        ...translated.map(([locale, prefix]) => [locale, `/${prefix}/:path(.*)?`]),
        ["en", "/:path(.*)?"],
      ],
    },
  ],
};
