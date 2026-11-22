// Every Dashboard read and write of localStorage and sessionStorage goes through this module.
//
// Key names: new keys use `pontia.<camelCaseName>`, the form most existing keys use.
// A key kept per device or per Bot appends its scope with `scopedKey`.
// Existing keys keep their names and value formats so saved data stays readable.

export type StorageArea = "local" | "session";

type Options = { area?: StorageArea };

function storage(area: StorageArea = "local"): Storage {
  return area === "local" ? window.localStorage : window.sessionStorage;
}

/** `name:scope` for one scope part, `name:["a","b"]` for several. */
export function scopedKey(name: string, ...scope: [string, ...string[]]): string {
  return `${name}:${scope.length === 1 ? scope[0] : JSON.stringify(scope)}`;
}

/** The saved string, or null when the key is missing or storage is unavailable. */
export function readStored(key: string, { area }: Options = {}): string | null {
  try {
    return storage(area).getItem(key);
  } catch {
    return null;
  }
}

/** The saved value read by `parse`, or null when it is missing, unreadable or storage is unavailable. */
export function readStoredWith<T>(
  key: string,
  parse: (saved: string) => T | undefined,
  options: Options = {},
): T | null {
  const saved = readStored(key, options);
  if (saved === null) return null;
  try {
    return parse(saved) ?? null;
  } catch {
    return null;
  }
}

/**
 * Saves `value`, or removes the key when it is null, and reports whether that succeeded.
 * Unavailable or full storage is ignored unless `required` is set, for data whose loss would
 * lose user work; then the error is thrown.
 */
export function writeStored(
  key: string,
  value: string | null,
  { area, required = false }: Options & { required?: boolean } = {},
): boolean {
  try {
    if (value === null) storage(area).removeItem(key);
    else storage(area).setItem(key, value);
    return true;
  } catch (error) {
    if (required) throw error;
    return false;
  }
}
