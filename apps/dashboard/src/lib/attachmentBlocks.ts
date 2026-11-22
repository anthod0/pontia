import { Lexer, type Tokens } from "marked";
import type { AttachedFile } from "../api/types";

const LANGUAGE = "attachments";

export interface MessageAttachments {
  /** The message without its attachments blocks. */
  text: string;
  /** Absolute paths from every attachments block, in order and without duplicates. */
  paths: string[];
}

/**
 * Splits attachments blocks from an Agent or Bot reply. An attachments block is a
 * top-level ``` fenced code block with the language `attachments` and one absolute
 * path per line; blank lines are ignored. A block with any other line, or one left
 * unclosed when the message ends, stays a code block. While streaming, a trailing
 * fence that is or may become an attachments block is hidden until it closes.
 */
export function splitAttachments(content: string, streaming = false): MessageAttachments {
  if (!content.includes("```")) return { text: content, paths: [] };
  const tokens = Lexer.lex(content.replace(/\r\n?/g, "\n"));
  const paths = new Set<string>();
  let text = "";
  let changed = false;
  let removed = false;
  tokens.forEach((token, index) => {
    // Neighbours of a removed block stay separate blocks, joined by one blank line.
    if (removed && token.type === "space") return;
    if (removed && text && !text.endsWith("\n\n")) text = text.replace(/\n?$/, "\n\n");
    removed = false;
    if (token.type === "code") {
      const code = token as Tokens.Code;
      const fence = /^ {0,3}(`{3,})/.exec(code.raw)?.[1];
      const closed = fence !== undefined && closes(code.raw, fence);
      const pending =
        streaming &&
        index === tokens.length - 1 &&
        fence !== undefined &&
        !closed &&
        (code.lang === LANGUAGE ||
          (!code.raw.includes("\n") && LANGUAGE.startsWith(code.lang ?? "")));
      const block = closed && code.lang === LANGUAGE ? blockPaths(code.text) : null;
      if (pending || block) {
        for (const path of block ?? []) paths.add(path);
        changed = removed = true;
        return;
      }
    }
    text += token.raw;
  });
  // Leading spaces can be an indented code block, so only blank lines are trimmed.
  return changed
    ? { text: text.replace(/^\n+/, "").trimEnd(), paths: [...paths] }
    : { text: content, paths: [] };
}

/** Paths of a block, or null when any non-blank line is not an absolute path. */
function blockPaths(text: string): string[] | null {
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  return lines.length && lines.every((line) => line.startsWith("/")) ? lines : null;
}

function closes(raw: string, fence: string): boolean {
  const lines = raw.replace(/\n+$/, "").split("\n");
  return lines.length > 1 && new RegExp(`^ {0,3}\`{${fence.length},}[ \\t]*$`).test(lines.at(-1)!);
}

export function attachedFile(path: string): AttachedFile {
  return { path, name: path.slice(path.lastIndexOf("/") + 1) || path };
}
