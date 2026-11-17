import { get } from "svelte/store";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import en from "../src/lib/i18n/en.json";
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
  [["fr-FR"], "en"],
])("resolves browser preferences %j to %s", async (languages, expected) => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(languages as string[]);
  const { currentLocale, languagePreference } = await import("../src/lib/i18n");
  expect(get(languagePreference)).toBe("system");
  expect(get(currentLocale)).toBe(expected);
});

test("saved choice overrides the browser and survives initialization", async () => {
  vi.spyOn(navigator, "languages", "get").mockReturnValue(["en-US"]);
  localStorage.setItem("pontia.locale", "zh-CN");
  const { currentLocale, languagePreference } = await import("../src/lib/i18n");
  expect(get(currentLocale)).toBe("zh-CN");
  expect(get(languagePreference)).toBe("zh-CN");
  languagePreference.set("en");
  expect(localStorage.getItem("pontia.locale")).toBe("en");
  expect(get(currentLocale)).toBe("en");
});

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

test("both catalogs contain the same keys and interpolation parameters", () => {
  expect(Object.keys(zhCN).sort()).toEqual(Object.keys(en).sort());
  const parameters = (message: string) =>
    [
      ...new Set(
        [...message.matchAll(/\{(\w+)(?:\}|,\s*(?:number|plural|select|date|time)\b)/g)].map(
          (match) => match[1],
        ),
      ),
    ].sort();
  for (const key of Object.keys(en) as (keyof typeof en)[]) {
    expect(parameters(zhCN[key]), key).toEqual(parameters(en[key]));
  }
});
