import { afterEach, expect, test } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/svelte";
import ShortcutSettings from "../../src/components/settings/ShortcutSettings.svelte";
import ShortcutManager from "../../src/lib/components/shortcuts/ShortcutManager.svelte";
import { shortcuts } from "../../src/lib/shortcuts/store.svelte";

afterEach(() => {
  cleanup();
  shortcuts.reset();
});

function open() {
  render(ShortcutManager);
  render(ShortcutSettings);
  return screen.getByRole("button", { name: "Change shortcut for Toggle sidebar" });
}

test("records, cancels or explicitly replaces a conflict and restores defaults", async () => {
  const button = open();
  async function conflict() {
    await fireEvent.click(button);
    await fireEvent.keyDown(button, { code: "KeyJ", altKey: true });
  }
  await conflict();
  await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(shortcuts.label("toggleSidebar")).toBe("Ctrl+B");
  await conflict();
  await fireEvent.click(screen.getByRole("button", { name: "Replace" }));
  expect(JSON.parse(localStorage.getItem("pontia.shortcuts")!).bindings).toMatchObject({
    nextChat: null,
    toggleSidebar: { code: "KeyJ", alt: true },
  });
  await fireEvent.click(screen.getByRole("button", { name: "Restore default for Toggle sidebar" }));
  expect(shortcuts.changed("toggleSidebar")).toBe(false);
  expect(shortcuts.binding("nextChat")).toBeNull();
  await fireEvent.click(screen.getByRole("button", { name: "Restore all defaults" }));
  expect(shortcuts.label("nextChat")).toBe("Alt+J");
  expect(localStorage.getItem("pontia.shortcuts")).toBeNull();
});

test("cancels recording on Escape or blur and disables a binding with Delete", async () => {
  const button = open();
  await fireEvent.click(button);
  await fireEvent.keyDown(button, { code: "KeyR", ctrlKey: true });
  expect(button).toHaveAttribute("aria-pressed", "true");
  await fireEvent.keyDown(button, { code: "Escape" });
  expect(shortcuts.changed("toggleSidebar")).toBe(false);
  await fireEvent.click(button);
  await fireEvent.blur(button);
  expect(button).toHaveAttribute("aria-pressed", "false");
  await fireEvent.click(button);
  await fireEvent.keyDown(button, { code: "Delete" });
  expect(shortcuts.binding("toggleSidebar")).toBeNull();
});

test("the position switch and changes from other tabs update active bindings", async () => {
  open();
  const toggle = screen.getByRole("switch");
  await fireEvent.click(toggle);
  expect(shortcuts.binding("openChatByPosition")).toBeNull();
  localStorage.setItem(
    "pontia.shortcuts",
    JSON.stringify({ version: 1, bindings: { showHelp: null } }),
  );
  await fireEvent(
    window,
    new StorageEvent("storage", { key: "pontia.shortcuts", storageArea: localStorage }),
  );
  expect(toggle).toHaveAttribute("aria-checked", "true");
  expect(shortcuts.binding("showHelp")).toBeNull();
});

test("canceling a position shortcut conflict keeps the switch off", async () => {
  const sidebar = open();
  const positions = screen.getByRole("switch");
  await fireEvent.click(positions);
  await fireEvent.click(sidebar);
  await fireEvent.keyDown(sidebar, { code: "Digit2", altKey: true });
  await fireEvent.click(positions);
  await fireEvent.click(screen.getByRole("button", { name: "Cancel" }));
  expect(positions).toHaveAttribute("aria-checked", "false");
  expect(shortcuts.binding("openChatByPosition")).toBeNull();
  await fireEvent.click(positions);
  await fireEvent.click(screen.getByRole("button", { name: "Replace" }));
  expect(positions).toHaveAttribute("aria-checked", "true");
  expect(shortcuts.binding("toggleSidebar")).toBeNull();
});
