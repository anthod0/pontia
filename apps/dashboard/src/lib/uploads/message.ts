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
export function uploadMessage(text: string, files: UploadedFile[]): string {
  return [
    text.trim(),
    ...files.map(
      (file) => `<user_upload path="${escapeXml(file.path)}">${escapeXml(file.name)}</user_upload>`,
    ),
  ]
    .filter(Boolean)
    .join("\n");
}

export function parseUploadMessage(content: string): { text: string; files: UploadedFile[] } {
  const files: UploadedFile[] = [];
  const lines = content.split("\n").filter((line) => {
    if (!/^\s*<user_upload\s[^]*<\/user_upload>\s*$/.test(line)) return true;
    const document = new DOMParser().parseFromString(line, "application/xml");
    if (document.querySelector("parsererror")) return true;
    const node = document.documentElement;
    if (
      node.tagName !== "user_upload" ||
      node.attributes.length !== 1 ||
      !node.hasAttribute("path") ||
      Array.from(node.childNodes).some((child) => child.nodeType !== 3)
    )
      return true;
    const path = node.getAttribute("path") ?? "";
    const name = node.textContent ?? "";
    if (!path.startsWith("/") || !name || name === "." || name === ".." || /[/\\\0]/.test(name))
      return true;
    files.push({ path, name });
    return false;
  });
  return { text: lines.join("\n"), files };
}
