import type { MessageKey } from "$lib/i18n";
import { isApplePlatform } from "./format";

export type KeyCombo = {
  code: string;
  mod?: boolean;
  alt?: boolean;
  shift?: boolean;
  secondary?: boolean; // Control on Apple keyboards, Meta elsewhere.
};

const pages = { newChat: 1, session: 2, bot: 4, other: 8 };
const scopes = {
  all: 15,
  chat: pages.newChat | pages.session,
  input: pages.newChat | pages.session | pages.bot,
  newChat: pages.newChat,
  session: pages.session,
  bot: pages.bot,
};

type Definition = {
  label: MessageKey;
  scope: keyof typeof scopes;
  combo: KeyCombo;
  shiftReverses?: boolean;
};
const alt = (code: string): KeyCombo => ({ code, alt: true });

const definitions = {
  toggleSidebar: {
    label: "shortcuts.toggleSidebar",
    scope: "all",
    combo: { code: "KeyB", mod: true },
  },
  showHelp: { label: "shortcuts.title", scope: "all", combo: { ...alt("Slash"), shift: true } },
  nextChat: { label: "shortcuts.nextChat", scope: "chat", combo: alt("KeyJ") },
  previousChat: { label: "shortcuts.previousChat", scope: "chat", combo: alt("KeyK") },
  openChatByPosition: { label: "shortcuts.openByPosition", scope: "chat", combo: alt("Digit1-9") },
  newChat: { label: "chat.new", scope: "chat", combo: alt("KeyN") },
  focusInput: { label: "shortcuts.focusInput", scope: "input", combo: alt("KeyL") },
  toggleGit: { label: "shortcuts.toggleGit", scope: "session", combo: alt("KeyG") },
  previousDevice: { label: "shortcuts.previousDevice", scope: "all", combo: alt("KeyM") },
  switchAgentClient: {
    label: "shortcuts.switchAgentClient",
    scope: "newChat",
    combo: alt("KeyC"),
    shiftReverses: true,
  },
  switchWorkspace: {
    label: "shortcuts.switchWorkspace",
    scope: "newChat",
    combo: alt("KeyW"),
    shiftReverses: true,
  },
  switchBot: {
    label: "shortcuts.switchBot",
    scope: "bot",
    combo: alt("KeyB"),
    shiftReverses: true,
  },
  switchBotPanelTab: {
    label: "shortcuts.switchBotPanelTab",
    scope: "bot",
    combo: alt("KeyP"),
    shiftReverses: true,
  },
} satisfies Record<string, Definition>;
export type ShortcutId = keyof typeof definitions;
export const registry: Record<ShortcutId, Definition> = definitions;
export const shortcutIds = (Object.keys(definitions) as ShortcutId[]).filter(
  (id) => id !== "previousDevice" || import.meta.env.VITE_DASHBOARD_MODE === "public",
);

export function activeOnPath(id: ShortcutId, path: string): boolean {
  const page =
    path === "/" || path === "/new"
      ? pages.newChat
      : path.startsWith("/chat/")
        ? pages.session
        : path.startsWith("/bots/")
          ? pages.bot
          : pages.other;
  return Boolean(scopes[registry[id].scope] & page);
}

export function eventCombo(event: KeyboardEvent, apple = isApplePlatform()): KeyCombo {
  return {
    code: event.code,
    mod: apple ? event.metaKey : event.ctrlKey,
    secondary: apple ? event.ctrlKey : event.metaKey,
    shift: event.shiftKey,
    alt: event.altKey,
  };
}

export function ignoredEvent(event: KeyboardEvent): boolean {
  return event.defaultPrevented || event.isComposing || event.keyCode === 229;
}

export function sameCombo(a: KeyCombo, b: KeyCombo): boolean {
  return (
    a.code === b.code &&
    !!a.mod === !!b.mod &&
    !!a.secondary === !!b.secondary &&
    !!a.shift === !!b.shift &&
    !!a.alt === !!b.alt
  );
}

function variants(id: ShortcutId, combo: KeyCombo): KeyCombo[] {
  if (id === "openChatByPosition")
    return Array.from({ length: 9 }, (_, index) => ({ ...combo, code: "Digit" + (index + 1) }));
  return registry[id].shiftReverses ? [combo, { ...combo, shift: true }] : [combo];
}

export function matches(id: ShortcutId, binding: KeyCombo, pressed: KeyCombo): boolean {
  return variants(id, binding).some((combo) => sameCombo(combo, pressed));
}

export function overlaps(
  id: ShortcutId,
  combo: KeyCombo,
  other: ShortcutId,
  binding: KeyCombo,
): boolean {
  return (
    Boolean(scopes[registry[id].scope] & scopes[registry[other].scope]) &&
    variants(id, combo).some((pressed) => matches(other, binding, pressed))
  );
}

export function recordError(id: ShortcutId, combo: KeyCombo): MessageKey | null {
  const { code, mod, secondary, shift, alt } = combo;
  if (
    !code ||
    /^(Unidentified|Escape|Tab|CapsLock|(?:Shift|Control|Alt|Meta|OS)(?:Left|Right))$/.test(code)
  )
    return "shortcuts.invalid";
  if (!mod && !secondary && !alt && !shift && !/^F([1-9]|1[0-2])$/.test(code))
    return "shortcuts.modifierRequired";
  if (
    code === "F5" ||
    (mod && !secondary && !alt && !shift && /^(Key[WTNQRL]|Digit[1-9])$/.test(code))
  )
    return "shortcuts.reserved";
  if (shift && registry[id].shiftReverses) return "shortcuts.reverseShift";
  return null;
}

const keyLabels: Record<string, string> = {
  Slash: "/",
  "Digit1-9": "1-9",
  Backquote: "`",
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
  Backslash: "\\",
  Semicolon: ";",
  Quote: "'",
  Comma: ",",
  Period: ".",
};
export function comboKeys(combo: KeyCombo): string[] {
  const question = combo.shift && combo.code === "Slash";
  const key = question ? "?" : (keyLabels[combo.code] ?? combo.code.replace(/^(Key|Digit)/, ""));
  return [
    ...(combo.mod ? ["Mod"] : []),
    ...(combo.secondary ? [isApplePlatform() ? "Ctrl" : "Meta"] : []),
    ...(combo.alt ? ["Alt"] : []),
    ...(combo.shift && !question ? ["Shift"] : []),
    key,
  ];
}
