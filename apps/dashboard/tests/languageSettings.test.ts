import userEvent from "@testing-library/user-event";
import { render, screen, waitFor } from "@testing-library/svelte";
import { get } from "svelte/store";
import { afterEach, expect, test } from "vitest";
import { currentLocale, languagePreference } from "../src/lib/i18n";
import LanguageSettingsHarness from "./components/LanguageSettingsHarness.svelte";

afterEach(() => languagePreference.set("system"));

test("changing language preserves the editor and draft while updating the document language", async () => {
  const user = userEvent.setup();
  languagePreference.set("en");
  const { container } = render(LanguageSettingsHarness);
  const editor = container.querySelector<HTMLElement>('[contenteditable="true"]')!;
  await user.click(editor);
  await user.type(editor, "Keep this unsent draft");

  screen.getByRole("combobox", { name: "Language" }).focus();
  await user.keyboard("{Enter}{Home}{ArrowDown}{Enter}");

  await waitFor(() => expect(document.documentElement.lang).toBe("zh-CN"));
  expect(get(currentLocale)).toBe("zh-CN");
  expect(localStorage.getItem("pontia.locale")).toBe("zh-CN");
  expect(container.querySelector('[contenteditable="true"]')).toBe(editor);
  expect(editor.textContent).toBe("Keep this unsent draft");

  screen.getByRole("combobox", { name: "语言" }).focus();
  await user.keyboard("{Enter}{End}{Enter}");
  await waitFor(() => expect(document.documentElement.lang).toBe("en"));
  expect(editor.textContent).toBe("Keep this unsent draft");
});
