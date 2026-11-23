import { Lexer, type Tokens } from "marked";
import type { AttachedFile } from "../api/types";

const LANGUAGE = "attachments";

/** A run of message text, or the files of one attachments block, in message order. */
export type MessagePart = { kind: "text"; text: string } | { kind: "files"; paths: string[] };

/**
 * Splits an Agent or Bot reply at its attachments blocks. An attachments block is a
 * top-level ``` fenced code block with the language `attachments` and one absolute
 * path per line; blank lines are ignored and a path repeated within a block counts
 * once. A block with any other line, or one left unclosed when the message ends,
 * stays a code block. While streaming, a trailing fence that is or may become an
 * attachments block is hidden until it closes. Blank text parts are left out.
 */
export function splitAttachments(content: string, streaming = false): MessagePart[] {
  const whole = (): MessagePart[] => (content.trim() ? [{ kind: "text", text: content }] : []);
  if (!content.includes("```")) return whole();
  const tokens = Lexer.lex(content.replace(/\r\n?/g, "\n"));
  const parts: MessagePart[] = [];
  let text = "";
  let changed = false;
  const flush = () => {
    // Leading spaces can be an indented code block, so only blank lines are trimmed.
    const trimmed = text.replace(/^\n+/, "").trimEnd();
    if (trimmed) parts.push({ kind: "text", text: trimmed });
    text = "";
  };
  tokens.forEach((token, index) => {
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
      const paths = closed && code.lang === LANGUAGE ? blockPaths(code.text) : null;
      if (pending || paths) {
        changed = true;
        flush();
        if (paths) parts.push({ kind: "files", paths });
        return;
      }
    }
    text += token.raw;
  });
  if (!changed) return whole();
  flush();
  return parts;
}

/** Every attached path of a reply, in order and without duplicates. */
export function attachedPaths(content: string): string[] {
  return [
    ...new Set(
      splitAttachments(content).flatMap((part) => (part.kind === "files" ? part.paths : [])),
    ),
  ];
}

/** Paths of a block without duplicates, or null when any non-blank line is not an absolute path. */
function blockPaths(text: string): string[] | null {
  const lines = text
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
  return lines.length && lines.every((line) => line.startsWith("/")) ? [...new Set(lines)] : null;
}

function closes(raw: string, fence: string): boolean {
  const lines = raw.replace(/\n+$/, "").split("\n");
  return lines.length > 1 && new RegExp(`^ {0,3}\`{${fence.length},}[ \\t]*$`).test(lines.at(-1)!);
}

export function attachedFile(path: string): AttachedFile {
  return { path, name: path.slice(path.lastIndexOf("/") + 1) || path };
}
