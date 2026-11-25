import type { BotMessage } from "../api/types";
import { messageText, parseQuotedMessage, plainLine } from "./botQuote";
import { attachedPaths } from "./attachmentBlocks";
import { parseUploadMessage } from "./uploads/message";
import { resolveLocalFileLink } from "./downloads/localFilePath";

export type SnippetPart = { text: string; match: boolean };

const context = 40;

function oneLine(text: string): string {
  return text.replace(/\s+/g, " ").trim();
}

/** Folds case like SQLite LIKE: ASCII letters only, keeping every index in place. */
function fold(text: string): string {
  return text.replace(/[A-Z]/g, (letter) => letter.toLowerCase());
}

/** Absolute local paths of a reply's Markdown links and images. */
function linkedPaths(content: string): string[] {
  return [...content.matchAll(/\]\(\s*(?:<([^>\n]+)>|([^)\s]+))/g)].flatMap(
    ([, angled, href]) => resolveLocalFileLink(angled ?? href)?.path ?? [],
  );
}

/**
 * The first readable part of a message that contains `query`, as shown in the chat:
 * its text, then its quote, then its file and link paths. Falls back to the stored content
 * when the match is only in markup.
 */
function matchedText(message: BotMessage, needle: string): string {
  const quoted = message.kind === "user_input" ? parseQuotedMessage(message.content) : null;
  const quote = quoted?.quote;
  const parts = quoted && parseUploadMessage(quoted.text);
  const text = plainLine(parts?.text ?? messageText(message));
  const candidates = [
    message.kind === "bot_reply" ? text.replace(/!?\[([^\]]+)\]\([^)]+\)/g, "$1") : text,
    plainLine(quote?.kind === "message" ? quote.text : (quote?.name ?? "")),
    ...(message.kind === "bot_reply"
      ? [...attachedPaths(message.content), ...linkedPaths(message.content)]
      : [
          ...(parts?.files.flatMap((file) => [file.name, file.path]) ?? []),
          ...(parts?.secrets.flatMap((secret) => [secret.label ?? "", secret.masked]) ?? []),
        ]),
  ];
  return (
    candidates.find((text) => fold(text).includes(needle)) ??
    (parts?.secrets.length ? candidates.filter(Boolean).join(" ") : oneLine(message.content))
  );
}

/** A one-line excerpt around the first match, with every match marked. */
export function searchSnippet(message: BotMessage, query: string): SnippetPart[] {
  const needle = fold(query.trim());
  const text = matchedText(message, needle);
  if (!needle) return [{ text, match: false }];
  const first = fold(text).indexOf(needle);
  const start = first > context ? first - context : 0;
  return [...(start > 0 ? [{ text: "…", match: false }] : []), ...marks(text, needle, start)];
}

/** The whole text with every match of `query` marked. */
export function markMatches(text: string, query: string): SnippetPart[] {
  const needle = fold(query.trim());
  return needle ? marks(text, needle, 0) : [{ text, match: false }];
}

function marks(text: string, needle: string, start: number): SnippetPart[] {
  const lower = fold(text);
  const parts: SnippetPart[] = [];
  let index = start;
  while (index < text.length) {
    const found = lower.indexOf(needle, index);
    if (found < 0) {
      parts.push({ text: text.slice(index), match: false });
      break;
    }
    if (found > index) parts.push({ text: text.slice(index, found), match: false });
    parts.push({ text: text.slice(found, found + needle.length), match: true });
    index = found + needle.length;
  }
  return parts;
}
