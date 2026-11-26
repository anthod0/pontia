import { responseSize } from "./save";
import { FILE_PREVIEW_LIMIT } from "./preview";

export class TextPreviewError extends Error {
  constructor(readonly reason: "tooLarge" | "notText") {
    super(reason === "tooLarge" ? "Text preview limit exceeded" : "File is not UTF-8 text");
  }
}

export async function readTextPreview(response: Response): Promise<string> {
  if ((responseSize(response) ?? 0) > FILE_PREVIEW_LIMIT) {
    await response.body?.cancel();
    throw new TextPreviewError("tooLarge");
  }
  if (!response.body) return "";
  const reader = response.body.getReader();
  const decoder = new TextDecoder("utf-8", { fatal: true });
  let received = 0;
  let text = "";
  function decode(value?: Uint8Array, stream = false): string {
    try {
      return decoder.decode(value, { stream });
    } catch {
      throw new TextPreviewError("notText");
    }
  }
  try {
    while (true) {
      const { done, value } = await reader.read();
      if (done) {
        text += decode();
        break;
      }
      received += value.byteLength;
      if (received > FILE_PREVIEW_LIMIT) throw new TextPreviewError("tooLarge");
      const chunk = decode(value, true);
      if (chunk.includes("\0")) throw new TextPreviewError("notText");
      text += chunk;
    }
    return text;
  } finally {
    await reader.cancel();
    reader.releaseLock();
  }
}

const languages: Record<string, string> = {
  js: "javascript",
  mjs: "javascript",
  cjs: "javascript",
  ts: "typescript",
  mts: "typescript",
  cts: "typescript",
  jsx: "jsx",
  tsx: "tsx",
  py: "python",
  rs: "rust",
  go: "go",
  java: "java",
  c: "c",
  h: "c",
  cpp: "cpp",
  hpp: "cpp",
  cc: "cpp",
  rb: "ruby",
  sh: "bash",
  bash: "bash",
  zsh: "bash",
  json: "json",
  yaml: "yaml",
  yml: "yaml",
  toml: "toml",
  html: "html",
  htm: "html",
  css: "css",
  xml: "xml",
  svg: "xml",
  sql: "sql",
  svelte: "svelte",
  md: "markdown",
  markdown: "markdown",
};

export function textFileLanguage(path: string): string {
  const name = path.slice(path.lastIndexOf("/") + 1).toLowerCase();
  return languages[name.slice(name.lastIndexOf(".") + 1)] ?? "text";
}
