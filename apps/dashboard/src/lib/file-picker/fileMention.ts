export interface ActiveFileMention {
  start: number;
  end: number;
  query: string;
}

const TOKEN_BOUNDARY = /[\s()[\]{}<>"'`]/;

export function activeFileMention(value: string, cursor: number): ActiveFileMention | null {
  const beforeCursor = value.slice(0, cursor);
  const at = beforeCursor.lastIndexOf('@');
  if (at < 0) return null;
  if (at > 0 && !TOKEN_BOUNDARY.test(value[at - 1] ?? '')) return null;
  const between = value.slice(at + 1, cursor);
  if (/\s/.test(between)) return null;
  return { start: at, end: cursor, query: between };
}

export function replaceFileMention(value: string, mention: ActiveFileMention, path: string): { value: string; cursor: number } {
  const replacement = `@${path}`;
  const nextValue = `${value.slice(0, mention.start)}${replacement}${value.slice(mention.end)}`;
  return { value: nextValue, cursor: mention.start + replacement.length };
}
