import { render, screen } from "@testing-library/svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { languagePreference } from "../../src/lib/i18n";
import MessageTime from "../../src/lib/components/MessageTime.svelte";

const now = new Date(2026, 10, 23, 15, 30);
const today = new Date(2026, 10, 23, 9, 5).toISOString();
const earlier = new Date(2026, 10, 20, 9, 5).toISOString();

beforeEach(() => {
  vi.useFakeTimers({ now, toFake: ["Date"] });
});

afterEach(() => {
  vi.useRealTimers();
  languagePreference.set("system");
});

test.each([
  ["en", today, "09:05 AM"],
  ["en", earlier, "Nov 20, 09:05 AM"],
  ["zh-CN", today, "09:05"],
  ["zh-CN", earlier, "11月20日 09:05"],
] as const)("in %s shows %s as %s", (locale, value, label) => {
  languagePreference.set(locale);
  render(MessageTime, { value });
  const time = screen.getByText(label);
  expect(time.tagName).toBe("TIME");
  expect(time).toHaveAttribute("datetime", value);
  expect(time).toHaveAttribute("title", new Date(value).toLocaleString(locale));
});
