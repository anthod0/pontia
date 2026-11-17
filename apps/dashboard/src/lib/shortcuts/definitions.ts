import type { MessageKey } from "$lib/i18n";
import { SIDEBAR_KEYBOARD_SHORTCUT } from "$lib/components/ui/sidebar/constants";

export type ChatShortcutAction =
  | "nextChat"
  | "previousChat"
  | "openChatByPosition"
  | "newChat"
  | "focusInput"
  | "showHelp";

// "Mod" is the platform's primary modifier: Cmd on Apple platforms, Ctrl elsewhere.
export type ShortcutKey = "Mod" | "Alt" | "Shift" | (string & {});

export type ShortcutDefinition = {
  keys: ShortcutKey[];
  label: MessageKey;
};

export type ChatShortcutMatch =
  | { action: Exclude<ChatShortcutAction, "openChatByPosition"> }
  | { action: "openChatByPosition"; position: string };

export const chatShortcuts: Record<ChatShortcutAction, ShortcutDefinition> = {
  nextChat: { keys: ["Alt", "J"], label: "shortcuts.nextChat" },
  previousChat: { keys: ["Alt", "K"], label: "shortcuts.previousChat" },
  openChatByPosition: { keys: ["Alt", "1-9"], label: "shortcuts.openByPosition" },
  newChat: { keys: ["Alt", "N"], label: "chat.new" },
  focusInput: { keys: ["Alt", "L"], label: "shortcuts.focusInput" },
  showHelp: { keys: ["Alt", "?"], label: "shortcuts.title" },
};

// Handled by the shadcn sidebar provider; listed here so the help dialog stays in sync.
export const sidebarShortcut: ShortcutDefinition = {
  keys: ["Mod", SIDEBAR_KEYBOARD_SHORTCUT.toUpperCase()],
  label: "shortcuts.toggleSidebar",
};

export const helpShortcuts: ShortcutDefinition[] = [
  ...Object.values(chatShortcuts),
  sidebarShortcut,
];

export const messageInputShortcuts: ShortcutDefinition[] = [
  { keys: ["Enter"], label: "common.send" },
  { keys: ["Tab"], label: "shortcuts.queueMessage" },
  { keys: ["Alt", "Enter"], label: "shortcuts.queueMessage" },
  { keys: ["Shift", "Enter"], label: "shortcuts.newline" },
  { keys: ["Mod", "Enter"], label: "shortcuts.newline" },
  { keys: ["Enter"], label: "shortcuts.newlineAfterBackslash" },
];

const letterActions: Record<
  string,
  Exclude<ChatShortcutAction, "openChatByPosition" | "showHelp">
> = {
  KeyJ: "nextChat",
  KeyK: "previousChat",
  KeyN: "newChat",
  KeyL: "focusInput",
};

// Matches on event.code so Option+letter on macOS, which produces symbols in event.key, still works.
export function matchChatShortcut(event: KeyboardEvent): ChatShortcutMatch | null {
  if (!event.altKey || event.ctrlKey || event.metaKey || event.isComposing) return null;

  if (event.key === "?" || (event.code === "Slash" && event.shiftKey))
    return { action: "showHelp" };
  if (event.shiftKey) return null;

  const letterAction = letterActions[event.code];
  if (letterAction) return { action: letterAction };

  const digit = /^Digit([1-9])$/.exec(event.code)?.[1];
  return digit ? { action: "openChatByPosition", position: digit } : null;
}
