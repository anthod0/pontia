import type { DemoBotEntry } from "./mockData";

export type InlineToken =
  | { kind: "text" | "strong" | "code"; value: string }
  | { kind: "link"; value: string; path: string };

// Mock text only uses **bold**, `code` and [label](path).
export function inlineTokens(text: string): InlineToken[] {
  return text
    .split(/(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/)
    .filter(Boolean)
    .map((part): InlineToken => {
      if (part.startsWith("**")) return { kind: "strong", value: part.slice(2, -2) };
      if (part.startsWith("`")) return { kind: "code", value: part.slice(1, -1) };
      const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      return link ? { kind: "link", value: link[1], path: link[2] } : { kind: "text", value: part };
    });
}

/** A message as one plain line, as the side panel and quotes show it. */
export function plainText(entry: DemoBotEntry): string {
  if (entry.kind !== "bot") return entry.text;
  const text = entry.blocks
    .map((block) => (block.kind === "p" ? block.text : block.items.join(" ")))
    .join(" ");
  return inlineTokens(text)
    .map((token) => token.value)
    .join("");
}
