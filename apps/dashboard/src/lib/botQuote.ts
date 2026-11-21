import type { BotEntry, BotMessage } from "../api/types";
import { escapeXml, parseUploadMessage } from "./uploads/message";

export type QuoteSpeaker = "User" | "Bot";
export type BotQuote =
  | { kind: "message"; messageId: string; speaker: QuoteSpeaker; text: string; excerpt: boolean }
  | { kind: "file"; fileId: string; path: string; name: string };

export const QUOTE_CHARACTER_LIMIT = 2000;

export function limitQuote(quote: BotQuote): BotQuote {
  if (quote.kind !== "message") return quote;
  const characters = Array.from(quote.text);
  return characters.length > QUOTE_CHARACTER_LIMIT
    ? { ...quote, text: characters.slice(0, QUOTE_CHARACTER_LIMIT).join("") + "…", excerpt: true }
    : quote;
}

const open = "<user_quote ";
const close = "</user_quote>";
// Bot message and shared file IDs are a prefix plus a lowercase UUIDv7.
const uuidV7 = "[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}";
const messageIdPattern = new RegExp(`^bmsg_${uuidV7}$`);
const fileIdPattern = new RegExp(`^file_${uuidV7}$`);

function escapeText(value: string): string {
  return value.replaceAll("&", "&amp;").replaceAll("<", "&lt;").replaceAll(">", "&gt;");
}

export function quoteId(quote: BotQuote): string {
  return quote.kind === "message" ? quote.messageId : quote.fileId;
}

export function quoteText(quote: BotQuote): string {
  return quote.kind === "message" ? quote.text : quote.name;
}

/** Quoted text as one plain line, without common Markdown markers. */
export function quoteSummary(quote: BotQuote): string {
  return quoteText(quote)
    .replace(/^\s{0,3}(#{1,6}|>|[-*+]|\d+[.)])\s+/gm, "")
    .replace(/\*\*|__|`/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

/** Places the quote element on its own line before the user's text. */
export function quotedMessage(quote: BotQuote, text: string): string {
  quote = limitQuote(quote);
  const attributes =
    quote.kind === "message"
      ? [
          ["message_id", quote.messageId],
          ["speaker", quote.speaker],
          ...(quote.excerpt ? [["excerpt", "true"]] : []),
        ]
      : [
          ["file_id", quote.fileId],
          ["speaker", "Bot"],
          ["path", quote.path],
        ];
  const element = `${open}${attributes.map(([name, value]) => `${name}="${escapeXml(value)}"`).join(" ")}>${escapeText(quoteText(quote))}${close}`;
  return `${element}\n${text.trim()}`;
}

function parseElement(xml: string): BotQuote | null {
  const document = new DOMParser().parseFromString(xml, "application/xml");
  if (document.querySelector("parsererror")) return null;
  const node = document.documentElement;
  if (
    node.tagName !== "user_quote" ||
    Array.from(node.childNodes).some((child) => child.nodeType !== Node.TEXT_NODE)
  )
    return null;
  const text = node.textContent ?? "";
  const attributes = Object.fromEntries(
    Array.from(node.attributes, (attribute) => [attribute.name, attribute.value]),
  );
  const names = Object.keys(attributes).sort().join(" ");
  if (!text) return null;
  if (
    (names === "message_id speaker" || names === "excerpt message_id speaker") &&
    messageIdPattern.test(attributes.message_id) &&
    (attributes.speaker === "User" || attributes.speaker === "Bot") &&
    (attributes.excerpt ?? "true") === "true"
  )
    return {
      kind: "message",
      messageId: attributes.message_id,
      speaker: attributes.speaker,
      text,
      excerpt: "excerpt" in attributes,
    };
  if (
    names === "file_id path speaker" &&
    fileIdPattern.test(attributes.file_id) &&
    attributes.speaker === "Bot" &&
    attributes.path.startsWith("/")
  )
    return { kind: "file", fileId: attributes.file_id, path: attributes.path, name: text };
  return null;
}

/**
 * Splits a leading quote from user text. Anything that is not exactly one
 * valid quote element followed by a line break and non-empty text stays text.
 */
export function parseQuotedMessage(text: string): { quote: BotQuote | null; text: string } {
  const end = text.indexOf(close);
  if (!text.startsWith(open) || end === -1) return { quote: null, text };
  const rest = text.slice(end + close.length);
  if (!rest.startsWith("\n") || !rest.slice(1).trim()) return { quote: null, text };
  const quote = parseElement(text.slice(0, end + close.length));
  return quote ? { quote, text: rest.slice(1) } : { quote: null, text };
}

/** The visible text of a Bot chat message, without attachments or its own quote. */
export function messageText(message: BotMessage): string {
  if (message.kind === "bot_reply") return message.content;
  return parseUploadMessage(parseQuotedMessage(message.content).text).text;
}

/** A quote of a message, or of one file of a share. */
export function entryQuote(
  entry: BotEntry,
  excerpt: string | null = null,
  fileId: string | null = null,
): BotQuote | null {
  if (entry.kind === "shared_files") {
    const file = entry.files.find((file) => file.file_id === fileId);
    return file ? { kind: "file", fileId: file.file_id, path: file.path, name: file.name } : null;
  }
  if (!("content" in entry)) return null;
  const text =
    excerpt ??
    (messageText(entry) ||
      parseUploadMessage(entry.content)
        .files.map((file) => file.name)
        .join("\n"));
  return limitQuote({
    kind: "message",
    messageId: entry.id,
    speaker: entry.kind === "user_input" ? "User" : "Bot",
    text,
    excerpt: excerpt !== null,
  });
}
