export function localFilePath(href: string | undefined): string | null {
  if (!href?.startsWith("/") || href.startsWith("//")) return null;
  const path = href.replace(/(?::\d+(?::\d+)?|#L\d+(?:C\d+)?(?:-L?\d+(?:C\d+)?)?)$/, "");
  try {
    return decodeURIComponent(path);
  } catch {
    return null;
  }
}
