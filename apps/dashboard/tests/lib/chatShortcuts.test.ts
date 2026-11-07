import { describe, expect, test } from "vitest";
import { matchChatShortcut } from "../../src/lib/shortcuts/definitions";
import { formatShortcutKeys } from "../../src/lib/shortcuts/format";

function keydown(init: KeyboardEventInit): KeyboardEvent {
  return new KeyboardEvent("keydown", init);
}

describe("matchChatShortcut", () => {
  test("matches letter shortcuts by physical key", () => {
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyJ", key: "j" }))).toEqual({
      action: "nextChat",
    });
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyK", key: "k" }))).toEqual({
      action: "previousChat",
    });
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyN", key: "n" }))).toEqual({
      action: "newChat",
    });
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyL", key: "l" }))).toEqual({
      action: "focusInput",
    });
  });

  test("matches macOS Option+letter, which reports a symbol in key", () => {
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyJ", key: "∆" }))).toEqual({
      action: "nextChat",
    });
    expect(matchChatShortcut(keydown({ altKey: true, code: "Digit2", key: "™" }))).toEqual({
      action: "openChatByPosition",
      position: "2",
    });
  });

  test("matches the help shortcut", () => {
    expect(
      matchChatShortcut(keydown({ altKey: true, shiftKey: true, code: "Slash", key: "?" })),
    ).toEqual({
      action: "showHelp",
    });
  });

  test("ignores other modifiers, digit 0, and IME composition", () => {
    expect(matchChatShortcut(keydown({ code: "KeyJ", key: "j" }))).toBeNull();
    expect(
      matchChatShortcut(keydown({ altKey: true, ctrlKey: true, code: "KeyJ", key: "j" })),
    ).toBeNull();
    expect(
      matchChatShortcut(keydown({ altKey: true, metaKey: true, code: "KeyJ", key: "j" })),
    ).toBeNull();
    expect(
      matchChatShortcut(keydown({ altKey: true, shiftKey: true, code: "KeyJ", key: "J" })),
    ).toBeNull();
    expect(matchChatShortcut(keydown({ altKey: true, code: "Digit0", key: "0" }))).toBeNull();
    expect(
      matchChatShortcut(keydown({ altKey: true, isComposing: true, code: "KeyJ", key: "j" })),
    ).toBeNull();
  });
});

describe("formatShortcutKeys", () => {
  test("uses macOS symbols without separators on Apple platforms", () => {
    expect(formatShortcutKeys(["Mod", "B"], true)).toBe("⌘B");
    expect(formatShortcutKeys(["Alt", "J"], true)).toBe("⌥J");
    expect(formatShortcutKeys(["Alt", "1-9"], true)).toBe("⌥1-9");
  });

  test("uses Ctrl and Alt joined by + elsewhere", () => {
    expect(formatShortcutKeys(["Mod", "B"], false)).toBe("Ctrl+B");
    expect(formatShortcutKeys(["Alt", "?"], false)).toBe("Alt+?");
  });
});
