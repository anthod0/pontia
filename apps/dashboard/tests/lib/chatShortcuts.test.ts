import { describe, expect, test } from "vitest";
import {
  activeOnPath,
  eventCombo,
  ignoredEvent,
  matches,
  overlaps,
  recordError,
  registry,
  type ShortcutId,
} from "../../src/lib/shortcuts/registry";
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

test.each([
  ["nextChat", "KeyJ"],
  ["previousChat", "KeyK"],
  ["newChat", "KeyN"],
  ["focusInput", "KeyL"],
  ["previousDevice", "KeyM"],
  ["toggleGit", "KeyG"],
  ["openChatByPosition", "Digit2"],
] as const)("matches %s by physical key with exactly Alt", (id, code) => {
  const combo = registry[id].combo;
  expect(matches(id, combo, eventCombo(keydown({ code, key: "∆", altKey: true })))).toBe(true);
  expect(matches(id, combo, eventCombo(keydown({ code, altKey: true, shiftKey: true })))).toBe(
    false,
  );
});

test("matches the platform primary modifier and rejects extra modifiers", () => {
  const binding = registry.toggleSidebar.combo;
  for (const apple of [true, false]) {
    const init = { code: "KeyB", metaKey: apple, ctrlKey: !apple };
    expect(matches("toggleSidebar", binding, eventCombo(keydown(init), apple))).toBe(true);
    expect(
      matches("toggleSidebar", binding, eventCombo(keydown({ ...init, shiftKey: true }), apple)),
    ).toBe(false);
    expect(matches("toggleSidebar", binding, eventCombo(keydown(init), !apple))).toBe(false);
  }
});

test("ignores composition and consumed events", () => {
  const consumed = keydown({ cancelable: true });
  consumed.preventDefault();
  for (const event of [consumed, keydown({ isComposing: true }), keydown({ keyCode: 229 })])
    expect(ignoredEvent(event)).toBe(true);
});

test.each(["KeyW", "KeyT", "KeyN", "KeyQ", "KeyR", "KeyL", "Digit1", "Digit9", "Tab"])(
  "rejects reserved Mod+%s",
  (code) => {
    expect(recordError("newChat", { code, mod: true })).toBeTruthy();
  },
);

test("recording accepts function keys and reserves Shift for reverse actions", () => {
  for (const code of ["ControlLeft", "Escape", "Tab", "KeyA", "F5", "F13"])
    expect(recordError("newChat", { code })).toBeTruthy();
  expect(recordError("newChat", { code: "F12" })).toBeNull();
  expect(recordError("switchWorkspace", { code: "KeyX", alt: true, shift: true })).toBeTruthy();
});

test("Bot shortcuts use their own scope and Shift to reverse direction", () => {
  for (const id of ["switchBot", "switchBotPanelTab"] as const) {
    const binding = registry[id].combo;
    expect(activeOnPath(id, "/bots/a")).toBe(true);
    expect(activeOnPath(id, "/chat/a")).toBe(false);
    expect(matches(id, binding, eventCombo(keydown({ code: binding.code, altKey: true })))).toBe(
      true,
    );
    expect(
      matches(
        id,
        binding,
        eventCombo(keydown({ code: binding.code, altKey: true, shiftKey: true })),
      ),
    ).toBe(true);
  }
});

test("conflicts include shifted reverse actions and each position in overlapping scopes", () => {
  const conflict = (id: ShortcutId, code: string, shift: boolean, other: ShortcutId) =>
    overlaps(id, { code, alt: true, shift }, other, registry[other].combo);
  expect(conflict("newChat", "KeyC", true, "switchAgentClient")).toBe(true);
  expect(conflict("toggleGit", "KeyC", true, "switchAgentClient")).toBe(false);
  expect(conflict("switchWorkspace", "Digit9", false, "openChatByPosition")).toBe(true);
  expect(conflict("newChat", "Digit0", false, "openChatByPosition")).toBe(false);
  expect(activeOnPath("showHelp", "/settings/common")).toBe(true);
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
