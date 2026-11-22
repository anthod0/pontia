import { describe, expect, test } from "vitest";
import { matchChatShortcut } from "../../src/lib/shortcuts/definitions";
import { formatShortcutKeys } from "../../src/lib/shortcuts/format";
import {
  adjacentSessionId,
  adjacentValue,
  sessionIdAtShortcutIndex,
} from "../../src/lib/shortcuts/sessionNavigation";
import type { SessionView } from "../../src/api/types";

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
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyM", key: "µ" }))).toEqual({
      action: "previousDevice",
    });
    expect(
      matchChatShortcut(keydown({ altKey: true, shiftKey: true, code: "KeyM", key: "M" })),
    ).toBeNull();
  });

  test("matches switch shortcuts, with Shift reversing the direction", () => {
    expect(
      matchChatShortcut(keydown({ altKey: true, shiftKey: true, code: "KeyC", key: "C" })),
    ).toEqual({ action: "switchAgentClient", direction: -1 });
    expect(matchChatShortcut(keydown({ altKey: true, code: "KeyW", key: "∑" }))).toEqual({
      action: "switchWorkspace",
      direction: 1,
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

describe("session navigation", () => {
  const sessions = ["top", "middle", "bottom"].map((session_id) => ({ session_id }) as SessionView);

  test("Alt+J moves down and Alt+K moves up the list as given, wrapping at the ends", () => {
    expect(adjacentSessionId(sessions, "top", 1)).toBe("middle");
    expect(adjacentSessionId(sessions, "middle", -1)).toBe("top");
    expect(adjacentSessionId(sessions, "bottom", 1)).toBe("top");
    expect(adjacentSessionId(sessions, "top", -1)).toBe("bottom");
    expect(adjacentSessionId(sessions, null, 1)).toBe("top");
    expect(adjacentSessionId(sessions, null, -1)).toBe("bottom");
  });

  test("opens sessions by their position in the list", () => {
    expect(sessionIdAtShortcutIndex(sessions, "2")).toBe("middle");
    expect(sessionIdAtShortcutIndex(sessions, "4")).toBeNull();
  });

  test("adjacentValue wraps and starts from the ends for an unknown value", () => {
    expect(adjacentValue(["a", "b", "c"], "c", 1)).toBe("a");
    expect(adjacentValue(["a", "b", "c"], "a", -1)).toBe("c");
    expect(adjacentValue(["a", "b"], "missing", -1)).toBe("b");
    expect(adjacentValue([], "a", 1)).toBeNull();
  });
});
