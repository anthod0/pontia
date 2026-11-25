import type { DemoBotEntry } from "./mockData";

export type InlineToken =
  | { kind: "text" | "strong" | "code"; value: string }
  | { kind: "link"; value: string; path: string }
  | { kind: "preview"; value: string; url: string };

// Mock text only uses **bold**, `code` and [label](target). Targets on localhost open the Preview tab.
export function inlineTokens(text: string): InlineToken[] {
  return text
    .split(/(\*\*[^*]+\*\*|`[^`]+`|\[[^\]]+\]\([^)]+\))/)
    .filter(Boolean)
    .map((part): InlineToken => {
      if (part.startsWith("**")) return { kind: "strong", value: part.slice(2, -2) };
      if (part.startsWith("`")) return { kind: "code", value: part.slice(1, -1) };
      const link = part.match(/^\[([^\]]+)\]\(([^)]+)\)$/);
      if (!link) return { kind: "text", value: part };
      return link[2].startsWith("http://localhost")
        ? { kind: "preview", value: link[1], url: link[2] }
        : { kind: "link", value: link[1], path: link[2] };
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

export type MarkdownBlock =
  | { kind: "h1" | "h2" | "h3" | "p" | "quote"; text: string }
  | { kind: "ul"; items: string[] }
  | { kind: "code"; language: string; code: string }
  | { kind: "table"; head: string[]; rows: string[][] };

/** The Markdown subset the demo's files use: headings, paragraphs, lists, quotes, fenced code and tables. */
export function markdownBlocks(source: string): MarkdownBlock[] {
  const lines = source.split("\n");
  const blocks: MarkdownBlock[] = [];
  const cells = (line: string) =>
    line
      .slice(1, -1)
      .split("|")
      .map((cell) => cell.trim());
  for (let index = 0; index < lines.length; index++) {
    const line = lines[index];
    if (!line.trim()) continue;
    const heading = line.match(/^(#{1,3}) (.*)$/);
    if (heading)
      blocks.push({ kind: (["h1", "h2", "h3"] as const)[heading[1].length - 1], text: heading[2] });
    else if (line.startsWith("```")) {
      const code: string[] = [];
      while (++index < lines.length && !lines[index].startsWith("```")) code.push(lines[index]);
      blocks.push({ kind: "code", language: line.slice(3), code: code.join("\n") });
    } else if (line.startsWith("- ")) {
      const items = [line.slice(2)];
      while (lines[index + 1]?.startsWith("- ")) items.push(lines[++index].slice(2));
      blocks.push({ kind: "ul", items });
    } else if (line.startsWith("> ")) blocks.push({ kind: "quote", text: line.slice(2) });
    else if (line.startsWith("|")) {
      const head = cells(line);
      const rows: string[][] = [];
      index++;
      while (lines[index + 1]?.startsWith("|")) rows.push(cells(lines[++index]));
      blocks.push({ kind: "table", head, rows });
    } else blocks.push({ kind: "p", text: line });
  }
  return blocks;
}

export type SourceToken = { text: string; tone?: "heading" | "mark" | "code" | "strong" };

/** Source view colouring for Markdown, close to the Dashboard's Shiki theme. */
export function markdownSource(line: string): SourceToken[] {
  const heading = line.match(/^(#{1,3} )(.*)$/);
  if (heading)
    return [
      { text: heading[1], tone: "mark" },
      { text: heading[2], tone: "heading" },
    ];
  const marker = line.match(/^(- |> |\| ?)(.*)$/);
  const head = marker ? [{ text: marker[1], tone: "mark" as const }] : [];
  const rest = marker ? marker[2] : line;
  return [
    ...head,
    ...rest
      .split(/(\*\*[^*]+\*\*|`[^`]+`)/)
      .filter(Boolean)
      .map((part): SourceToken => {
        if (part.startsWith("**")) return { text: part, tone: "strong" };
        if (part.startsWith("`")) return { text: part, tone: "code" };
        return { text: part };
      }),
  ];
}
