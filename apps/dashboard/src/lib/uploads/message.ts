export type UploadedFile = { path: string; name: string };

function escapeXml(value: string): string {
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

export function uploadMessage(text: string, files: UploadedFile[]): string {
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

export function parseUploadMessage(content: string): {
  text: string;
  files: UploadedFile[];
} {
  const lines = content.split("\n");
  const end = lines.indexOf(uploadsClose);
  if (lines[0] !== uploadsOpen || end < 2) return { text: content, files: [] };
  const files = lines.slice(1, end).map(parseUploadTag);
  if (files.some((file) => !file)) return { text: content, files: [] };
  return {
    text: lines.slice(end + 1).join("\n"),
    files: files as UploadedFile[],
  };
}
