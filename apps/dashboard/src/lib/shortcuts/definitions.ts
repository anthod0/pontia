import type { MessageKey } from "$lib/i18n";

export const messageInputShortcuts: { keyGroups: string[][]; label: MessageKey }[] = [
  { keyGroups: [["Enter"]], label: "common.send" },
  { keyGroups: [["Tab"], ["Alt", "Enter"]], label: "shortcuts.queueMessage" },
  {
    keyGroups: [
      ["Shift", "Enter"],
      ["Mod", "Enter"],
    ],
    label: "shortcuts.newline",
  },
  { keyGroups: [["Enter"]], label: "shortcuts.newlineAfterBackslash" },
];
