import { hasControl } from "./secretMask";
export type UploadedFile = { path: string; name: string };

export type SecretFile = { path: string; masked: string; label?: string };

export function escapeXml(value: string): string {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&apos;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll("\n", "&#10;")
    .replaceAll("\r", "&#13;")
    .replaceAll("\t", "&#9;");
}
const uploadsOpen = '<user_uploads note="Temporary files; copy any you need into the workspace.">';
const uploadsClose = "</user_uploads>";

const secretsOpen =
  '<user_secrets note="Secret files; read directly by tools/scripts without printing, logging, or copying their plaintext into responses or the workspace.">';
const secretsClose = "</user_secrets>";

export function uploadMessage(
  text: string,
  files: UploadedFile[],
  secrets: SecretFile[] = [],
): string {
  return [
    ...(files.length
      ? [
          uploadsOpen,
          ...files.map(
            (file) =>
              `<user_upload path="${escapeXml(file.path)}">${escapeXml(file.name)}</user_upload>`,
          ),
          uploadsClose,
        ]
      : []),
    ...(secrets.length
      ? [
          secretsOpen,
          ...secrets.map(
            ({ path, label, masked }) =>
              `<user_secret path="${escapeXml(path)}"${label ? ` label="${escapeXml(label)}"` : ""}>${escapeXml(masked)}</user_secret>`,
          ),
          secretsClose,
        ]
      : []),
    text.trim(),
  ]
    .filter(Boolean)
    .join("\n");
}

function parseUploadTag(line: string): UploadedFile | null {
  if (!/^\s*<user_upload\s[^]*<\/user_upload>\s*$/.test(line)) return null;
  const document = new DOMParser().parseFromString(line, "application/xml");
  if (document.querySelector("parsererror")) return null;
  const node = document.documentElement;
  if (
    node.tagName !== "user_upload" ||
    node.attributes.length !== 1 ||
    !node.hasAttribute("path") ||
    Array.from(node.childNodes).some((child) => child.nodeType !== 3)
  )
    return null;
  const path = node.getAttribute("path") ?? "";
  const name = node.textContent ?? "";
  if (!path.startsWith("/") || !name || name === "." || name === ".." || /[/\\\0]/.test(name))
    return null;
  return { path, name };
}

function parseSecretTag(line: string): SecretFile | null {
  if (!/^\s*<user_secret\s[^]*<\/user_secret>\s*$/.test(line)) return null;
  // XML normalizes literal tabs and carriage returns in attributes before validation.
  if (
    /\slabel\s*=\s*(?:"[^"]*[\t\r][^"]*"|'[^']*[\t\r][^']*')/.test(line.slice(0, line.indexOf(">")))
  )
    return null;
  const doc = new DOMParser().parseFromString(line, "application/xml");
  const node = doc.documentElement;
  const path = node.getAttribute("path");
  const label = node.getAttribute("label");
  const masked = node.textContent;
  if (
    doc.querySelector("parsererror") ||
    node.tagName !== "user_secret" ||
    !path?.startsWith("/") ||
    !masked ||
    (label !== null && (!label.trim() || hasControl(label))) ||
    Array.from(node.attributes).some((attr) => attr.name !== "path" && attr.name !== "label") ||
    Array.from(node.childNodes).some((child) => child.nodeType !== Node.TEXT_NODE)
  )
    return null;
  return { path, masked, ...(label ? { label } : {}) };
}

export function parseUploadMessage(content: string) {
  const plain = { text: content, files: [] as UploadedFile[], secrets: [] as SecretFile[] };
  const lines = content.split("\n");
  let index = 0;
  function block<T>(open: string, close: string, parse: (line: string) => T | null): T[] | null {
    const items: T[] = [];
    if (lines[index] !== open) return items;
    index++;
    while (index < lines.length && lines[index] !== close) {
      const item = parse(lines[index++]);
      if (!item) return null;
      items.push(item);
    }
    if (!items.length || lines[index++] !== close) return null;
    return items;
  }
  const files = block(uploadsOpen, uploadsClose, parseUploadTag);
  const secrets = block(secretsOpen, secretsClose, parseSecretTag);
  return files && secrets ? { text: lines.slice(index).join("\n"), files, secrets } : plain;
}
