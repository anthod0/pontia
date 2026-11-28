import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import en from "../src/lib/i18n/en.json";
import ja from "../src/lib/i18n/ja.json";
import zhCN from "../src/lib/i18n/zh-CN.json";

beforeEach(() => {
  vi.resetModules();
  localStorage.removeItem("pontia.locale");
});

afterEach(() => {
  localStorage.removeItem("pontia.locale");
  vi.restoreAllMocks();
});

test.each([
  [["zh-CN"], "zh-CN"],
  [["zh-Hans-SG", "en"], "zh-CN"],
  [["en-GB", "zh-CN"], "en"],
  [["fr-FR", "zh-CN"], "zh-CN"],
  [["ja"], "ja"],
  [["fr-FR", "ja-JP", "en"], "ja"],
  [["fr-FR"], "en"],
])("resolves browser preferences %j to %s", async (languages, expected) => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(languages as string[]);
  const { currentLocale, languagePreference } = await import("../src/lib/i18n");
  expect(get(languagePreference)).toBe("system");
  expect(get(currentLocale)).toBe(expected);
});

test.each(["zh-CN", "ja"])(
  "saved choice %s overrides the browser and survives initialization",
  async (saved) => {
    vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
    localStorage.setItem("pontia.locale", saved);
    const { currentLocale, languagePreference } = await import("../src/lib/i18n");
    expect(get(currentLocale)).toBe(saved);
    expect(get(languagePreference)).toBe(saved);
    languagePreference.set("en");
    expect(localStorage.getItem("pontia.locale")).toBe("en");
    expect(get(currentLocale)).toBe("en");
  },
);

test("following the browser removes the override and responds to browser language changes", async () => {
  const languages = vi.spyOn(navigator, "languages", "get").mockReturnValue(["zh-CN"]);
  const { currentLocale, languagePreference, updateBrowserLanguage } =
    await import("../src/lib/i18n");
  languagePreference.set("en");
  updateBrowserLanguage();
  expect(get(currentLocale)).toBe("en");
  languagePreference.set("system");
  expect(localStorage.getItem("pontia.locale")).toBeNull();
  expect(get(currentLocale)).toBe("zh-CN");
  languages.mockReturnValue(["en-US"]);
  updateBrowserLanguage();
  expect(get(currentLocale)).toBe("en");
});

test("unrecognized stored preference uses browser detection", async () => {
  localStorage.setItem("pontia.locale", "unsupported");
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["zh-CN"]);
  const { currentLocale, languagePreference } = await import("../src/lib/i18n");
  expect(get(languagePreference)).toBe("system");
  expect(get(currentLocale)).toBe("zh-CN");
});

test.each([
  ["zh-CN", zhCN],
  ["ja", ja],
] as const)("%s matches English keys and interpolation parameters", (_locale, catalog) => {
  expect(Object.keys(catalog).sort()).toEqual(Object.keys(en).sort());
  const parameters = (message: string) =>
    [
      ...new Set(
        [...message.matchAll(/\{(\w+)(?:\}|,\s*(?:number|plural|select|date|time)\b)/g)].map(
          (match) => match[1],
        ),
      ),
    ].sort();
  for (const key of Object.keys(en) as (keyof typeof en)[]) {
    expect(parameters(catalog[key]), key).toEqual(parameters(en[key]));
  }
});
