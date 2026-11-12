import { expect, test } from "bun:test";
import { deLocalizeUrl, extractLocaleFromUrl, localizeHref } from "../src/lib/paraglide/runtime";

test.each([
  ["en", ""],
  ["zh-Hans", "/zh"],
  ["ja", "/ja"],
  ["pt-BR", "/pt-br"],
] as const)("round-trips %s page URLs", (locale, prefix) => {
  expect(localizeHref("/", { locale })).toBe(prefix || "/");
  const href = localizeHref("/docs/platforms/linux?a=1#install", { locale });
  expect(href).toBe(`${prefix}/docs/platforms/linux?a=1#install`);
  const url = new URL(href, "https://pontia.dev");
  expect(extractLocaleFromUrl(url)).toBe(locale);
  expect(deLocalizeUrl(url).href).toBe("https://pontia.dev/docs/platforms/linux?a=1#install");
});

test.each(["en", "zh-Hans", "ja", "pt-BR"] as const)(
  "keeps English-only and API URLs unprefixed for %s",
  (locale) => {
    for (const href of [
      "/privacy",
      "/privacy?source=docs#cookies",
      "/terms#terms",
      "/api",
      "/api/",
      "/api/auth/github?code=123",
    ]) {
      expect(localizeHref(href, { locale })).toBe(href);
      expect(deLocalizeUrl(new URL(href, "https://pontia.dev")).href).toBe(
        `https://pontia.dev${href}`,
      );
    }
  },
);

test("recognizes translated home pages with and without a trailing slash", () => {
  for (const path of ["/zh", "/zh/"]) {
    const url = new URL(path, "https://pontia.dev");
    expect(extractLocaleFromUrl(url)).toBe("zh-Hans");
    expect(deLocalizeUrl(url).pathname).toBe("/");
  }
  expect(extractLocaleFromUrl(new URL("https://pontia.dev/ZH"))).toBe("en");
});
