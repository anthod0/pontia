const appleKeySymbols: Record<string, string> = {
  Mod: "⌘",
  Ctrl: "⌃",
  Alt: "⌥",
  Shift: "⇧",
};

const otherKeyLabels: Record<string, string> = {
  Mod: "Ctrl",
};

export function isApplePlatform(): boolean {
  const platform =
    (navigator as Navigator & { userAgentData?: { platform?: string } }).userAgentData?.platform ||
    navigator.platform;
  return /mac|iphone|ipad|ipod/i.test(platform);
}

export function formatShortcutKeys(keys: string[], apple = isApplePlatform()): string {
  if (apple) return keys.map((key) => appleKeySymbols[key] ?? key).join("");
  return keys.map((key) => otherKeyLabels[key] ?? key).join("+");
}
