import { render } from "@testing-library/svelte";
import userEvent from "@testing-library/user-event";
import { get } from "svelte/store";
import { afterEach, expect, test, vi } from "vitest";
import AppearanceSettings from "../../src/components/settings/AppearanceSettings.svelte";
import { applyTheme, resolvedTheme, themePreference } from "../../src/lib/theme";

afterEach(() => {
  themePreference.set("system");
  document.documentElement.classList.remove("dark");
  vi.unstubAllGlobals();
});

test("stores an explicit theme and clears it for system", () => {
  themePreference.set("dark");
  expect(localStorage.getItem("pontia.theme")).toBe("dark");
  expect(get(resolvedTheme)).toBe("dark");

  themePreference.set("system");
  expect(localStorage.getItem("pontia.theme")).toBeNull();
});

test("applies the dark class and browser theme color", () => {
  const meta = document.createElement("meta");
  meta.name = "theme-color";
  document.head.append(meta);

  applyTheme("dark");
  expect(document.documentElement).toHaveClass("dark");
  expect(meta.content).toBe("#1d2021");

  applyTheme("light");
  expect(document.documentElement).not.toHaveClass("dark");
  expect(meta.content).toBe("#fafaf8");
  meta.remove();
});

test("selects a theme from Settings", async () => {
  const user = userEvent.setup();
  const { getByRole } = render(AppearanceSettings);

  await user.click(getByRole("radio", { name: "Dark" }));
  expect(get(themePreference)).toBe("dark");

  // Pressing the active option again keeps it selected.
  await user.click(getByRole("radio", { name: "Dark" }));
  expect(get(themePreference)).toBe("dark");
});
